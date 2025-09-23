# Rush Container Orchestration Fix Plan
## Dependency-Aware Startup with Health Checks

### Goal
Implement a robust container startup system that:
1. Respects dependency order (topological sort)
2. Waits for dependencies to be healthy before starting dependents
3. Provides configurable health checks for each component
4. Offers clear visibility into startup progress and failures

### Phase 1: Add Health Check Infrastructure

#### 1.1 Define Health Check Types in ComponentBuildSpec
**File**: `rush/crates/rush-build/src/spec.rs`

Add health check configuration:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Type of health check
    pub check_type: HealthCheckType,
    /// Initial delay before first check (seconds)
    pub initial_delay: u32,
    /// Interval between checks (seconds)
    pub interval: u32,
    /// Number of consecutive successes to be considered healthy
    pub success_threshold: u32,
    /// Number of consecutive failures to be considered unhealthy
    pub failure_threshold: u32,
    /// Timeout for each check (seconds)
    pub timeout: u32,
    /// Maximum number of retries before giving up
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthCheckType {
    /// HTTP GET request to a path
    Http { path: String, expected_status: u16 },
    /// TCP port check
    Tcp { port: u16 },
    /// Execute command in container
    Exec { command: Vec<String> },
    /// DNS resolution check (for ingress)
    Dns { hosts: Vec<String> },
}

// Add to ComponentBuildSpec:
pub struct ComponentBuildSpec {
    // ... existing fields ...
    pub health_check: Option<HealthCheckConfig>,
    pub startup_probe: Option<HealthCheckConfig>, // Separate probe for startup
}
```

#### 1.2 Update YAML Parser
**File**: `rush/crates/rush-build/src/spec.rs`

Parse health check configuration from stack.spec.yaml:
```yaml
backend:
  # ... existing config ...
  health_check:
    type: http
    path: /health
    expected_status: 200
    initial_delay: 5
    interval: 10
    success_threshold: 1
    failure_threshold: 3
    timeout: 5
    max_retries: 30

ingress:
  # ... existing config ...
  startup_probe:
    type: dns
    hosts: ["backend.docker", "frontend.docker"]
    initial_delay: 2
    interval: 1
    success_threshold: 1
    failure_threshold: 5
    timeout: 3
    max_retries: 60
```

### Phase 2: Implement Dependency Graph

#### 2.1 Create Dependency Graph Module
**New File**: `rush/crates/rush-container/src/dependency_graph.rs`

```rust
use std::collections::{HashMap, HashSet, VecDeque};
use rush_build::ComponentBuildSpec;
use rush_core::error::{Error, Result};

pub struct DependencyGraph {
    nodes: HashMap<String, Node>,
    edges: HashMap<String, Vec<String>>, // component -> dependents
}

struct Node {
    name: String,
    spec: ComponentBuildSpec,
    state: NodeState,
}

#[derive(Debug, Clone, PartialEq)]
enum NodeState {
    Pending,
    Starting,
    WaitingForHealth,
    Healthy,
    Failed(String),
}

impl DependencyGraph {
    pub fn from_specs(specs: Vec<ComponentBuildSpec>) -> Result<Self> {
        let mut graph = Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
        };

        // Create nodes
        for spec in specs {
            let node = Node {
                name: spec.component_name.clone(),
                spec: spec.clone(),
                state: NodeState::Pending,
            };
            graph.nodes.insert(spec.component_name.clone(), node);

            // Build reverse edges (for easy traversal)
            for dep in &spec.depends_on {
                graph.edges.entry(dep.clone())
                    .or_insert_with(Vec::new)
                    .push(spec.component_name.clone());
            }
        }

        // Validate no cycles
        graph.validate_acyclic()?;

        Ok(graph)
    }

    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        // Calculate in-degrees
        for name in self.nodes.keys() {
            in_degree.insert(name.clone(), 0);
        }

        for node in self.nodes.values() {
            for dep in &node.spec.depends_on {
                *in_degree.get_mut(dep).ok_or_else(||
                    Error::Config(format!("Unknown dependency: {}", dep))
                )? += 1;
            }
        }

        // Find nodes with no dependencies
        let mut queue: VecDeque<String> = in_degree.iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(name, _)| name.clone())
            .collect();

        while let Some(name) = queue.pop_front() {
            result.push(name.clone());

            // Reduce in-degree for dependents
            if let Some(dependents) = self.edges.get(&name) {
                for dependent in dependents {
                    let degree = in_degree.get_mut(dependent).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dependent.clone());
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err(Error::Config("Circular dependency detected".to_string()));
        }

        Ok(result)
    }

    pub fn get_ready_components(&self) -> Vec<String> {
        let mut ready = Vec::new();

        for (name, node) in &self.nodes {
            if node.state != NodeState::Pending {
                continue;
            }

            // Check if all dependencies are healthy
            let deps_ready = node.spec.depends_on.iter().all(|dep| {
                self.nodes.get(dep)
                    .map(|n| n.state == NodeState::Healthy)
                    .unwrap_or(false)
            });

            if deps_ready {
                ready.push(name.clone());
            }
        }

        ready
    }

    pub fn mark_starting(&mut self, name: &str) {
        if let Some(node) = self.nodes.get_mut(name) {
            node.state = NodeState::Starting;
        }
    }

    pub fn mark_waiting_for_health(&mut self, name: &str) {
        if let Some(node) = self.nodes.get_mut(name) {
            node.state = NodeState::WaitingForHealth;
        }
    }

    pub fn mark_healthy(&mut self, name: &str) {
        if let Some(node) = self.nodes.get_mut(name) {
            node.state = NodeState::Healthy;
        }
    }

    pub fn mark_failed(&mut self, name: &str, error: String) {
        if let Some(node) = self.nodes.get_mut(name) {
            node.state = NodeState::Failed(error);
        }
    }

    fn validate_acyclic(&self) -> Result<()> {
        // DFS-based cycle detection
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for name in self.nodes.keys() {
            if !visited.contains(name) {
                if self.has_cycle_dfs(name, &mut visited, &mut rec_stack)? {
                    return Err(Error::Config("Dependency cycle detected".to_string()));
                }
            }
        }

        Ok(())
    }

    fn has_cycle_dfs(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> Result<bool> {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());

        if let Some(node_data) = self.nodes.get(node) {
            for dep in &node_data.spec.depends_on {
                if !visited.contains(dep) {
                    if self.has_cycle_dfs(dep, visited, rec_stack)? {
                        return Ok(true);
                    }
                } else if rec_stack.contains(dep) {
                    return Ok(true);
                }
            }
        }

        rec_stack.remove(node);
        Ok(false)
    }

    pub fn get_startup_order(&self) -> Result<Vec<Vec<String>>> {
        // Returns components grouped by startup wave (can start in parallel)
        let sorted = self.topological_sort()?;
        let mut waves = Vec::new();
        let mut processed = HashSet::new();

        while processed.len() < sorted.len() {
            let mut wave = Vec::new();

            for name in &sorted {
                if processed.contains(name) {
                    continue;
                }

                let node = self.nodes.get(name).unwrap();
                let deps_processed = node.spec.depends_on.iter()
                    .all(|dep| processed.contains(dep));

                if deps_processed {
                    wave.push(name.clone());
                }
            }

            for name in &wave {
                processed.insert(name.clone());
            }

            if !wave.is_empty() {
                waves.push(wave);
            }
        }

        Ok(waves)
    }
}
```

### Phase 3: Implement Health Check Manager

#### 3.1 Create Health Check Manager
**New File**: `rush/crates/rush-container/src/health_check.rs`

```rust
use crate::docker::DockerClient;
use rush_build::{ComponentBuildSpec, HealthCheckConfig, HealthCheckType};
use rush_core::error::{Error, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use log::{debug, info, warn, error};

pub struct HealthCheckManager {
    docker_client: Arc<dyn DockerClient>,
}

impl HealthCheckManager {
    pub fn new(docker_client: Arc<dyn DockerClient>) -> Self {
        Self { docker_client }
    }

    pub async fn wait_for_healthy(
        &self,
        container_id: &str,
        component_name: &str,
        config: &HealthCheckConfig,
    ) -> Result<()> {
        info!("Waiting for {} to become healthy", component_name);

        // Initial delay
        if config.initial_delay > 0 {
            info!("{}: Waiting {}s before first health check",
                component_name, config.initial_delay);
            tokio::time::sleep(Duration::from_secs(config.initial_delay as u64)).await;
        }

        let mut consecutive_successes = 0;
        let mut consecutive_failures = 0;
        let mut total_attempts = 0;
        let start_time = Instant::now();

        loop {
            total_attempts += 1;

            match self.perform_health_check(container_id, &config.check_type).await {
                Ok(true) => {
                    consecutive_successes += 1;
                    consecutive_failures = 0;
                    debug!("{}: Health check passed ({}/{})",
                        component_name, consecutive_successes, config.success_threshold);

                    if consecutive_successes >= config.success_threshold {
                        let elapsed = start_time.elapsed();
                        info!("{}: Healthy after {:?} ({} checks)",
                            component_name, elapsed, total_attempts);
                        return Ok(());
                    }
                }
                Ok(false) | Err(_) => {
                    consecutive_failures += 1;
                    consecutive_successes = 0;
                    debug!("{}: Health check failed ({}/{})",
                        component_name, consecutive_failures, config.failure_threshold);

                    if consecutive_failures >= config.failure_threshold {
                        if total_attempts >= config.max_retries {
                            return Err(Error::HealthCheck(format!(
                                "{}: Failed to become healthy after {} attempts",
                                component_name, total_attempts
                            )));
                        }

                        warn!("{}: Health check failed {} times, resetting",
                            component_name, consecutive_failures);
                        consecutive_failures = 0;
                    }
                }
            }

            // Check if we've exceeded max retries
            if total_attempts >= config.max_retries {
                return Err(Error::HealthCheck(format!(
                    "{}: Exceeded max retries ({})",
                    component_name, config.max_retries
                )));
            }

            // Wait before next check
            tokio::time::sleep(Duration::from_secs(config.interval as u64)).await;
        }
    }

    async fn perform_health_check(
        &self,
        container_id: &str,
        check_type: &HealthCheckType,
    ) -> Result<bool> {
        match check_type {
            HealthCheckType::Http { path, expected_status } => {
                self.check_http(container_id, path, *expected_status).await
            }
            HealthCheckType::Tcp { port } => {
                self.check_tcp(container_id, *port).await
            }
            HealthCheckType::Exec { command } => {
                self.check_exec(container_id, command).await
            }
            HealthCheckType::Dns { hosts } => {
                self.check_dns(container_id, hosts).await
            }
        }
    }

    async fn check_http(
        &self,
        container_id: &str,
        path: &str,
        expected_status: u16,
    ) -> Result<bool> {
        // Execute curl command in container
        let command = vec![
            "curl".to_string(),
            "-f".to_string(),
            "-s".to_string(),
            "-o".to_string(),
            "/dev/null".to_string(),
            "-w".to_string(),
            "%{http_code}".to_string(),
            format!("http://localhost{}", path),
        ];

        match self.docker_client.exec_in_container(container_id, &command).await {
            Ok(output) => {
                let status_code: u16 = output.trim().parse().unwrap_or(0);
                Ok(status_code == expected_status)
            }
            Err(_) => Ok(false),
        }
    }

    async fn check_tcp(
        &self,
        container_id: &str,
        port: u16,
    ) -> Result<bool> {
        // Use netcat or similar to check if port is open
        let command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("nc -z localhost {} || (command -v ncat && ncat -z localhost {}) || (command -v netcat && netcat -z localhost {})",
                port, port, port),
        ];

        match self.docker_client.exec_in_container(container_id, &command).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn check_exec(
        &self,
        container_id: &str,
        command: &[String],
    ) -> Result<bool> {
        match self.docker_client.exec_in_container(container_id, command).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn check_dns(
        &self,
        container_id: &str,
        hosts: &[String],
    ) -> Result<bool> {
        for host in hosts {
            let command = vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("nslookup {} || getent hosts {} || host {}", host, host, host),
            ];

            match self.docker_client.exec_in_container(container_id, &command).await {
                Ok(_) => continue,
                Err(_) => return Ok(false),
            }
        }
        Ok(true)
    }
}
```

### Phase 4: Update Lifecycle Manager

#### 4.1 Modify LifecycleManager
**File**: `rush/crates/rush-container/src/lifecycle/manager.rs`

```rust
use crate::dependency_graph::DependencyGraph;
use crate::health_check::HealthCheckManager;

impl LifecycleManager {
    /// Start services with dependency ordering and health checks
    pub async fn start_services_with_dependencies(
        &self,
        services: Vec<ContainerService>,
        component_specs: &[ComponentBuildSpec],
        built_images: &HashMap<String, String>,
    ) -> Result<Vec<DockerService>> {
        info!("Starting services with dependency ordering");

        // Build dependency graph
        let mut dep_graph = DependencyGraph::from_specs(component_specs.to_vec())?;

        // Get startup waves (components that can start in parallel)
        let startup_waves = dep_graph.get_startup_order()?;
        info!("Startup plan: {} waves", startup_waves.len());

        for (wave_num, wave) in startup_waves.iter().enumerate() {
            info!("Starting wave {}/{}: {:?}",
                wave_num + 1, startup_waves.len(), wave);
        }

        // Create health check manager
        let health_manager = HealthCheckManager::new(self.docker_client.clone());

        let mut running_services = Vec::new();

        // Process each wave
        for (wave_num, wave) in startup_waves.iter().enumerate() {
            info!("🌊 Starting wave {}/{}", wave_num + 1, startup_waves.len());

            // Start all components in this wave in parallel
            let mut wave_tasks = Vec::new();

            for component_name in wave {
                let service = services.iter()
                    .find(|s| &s.name == component_name);

                if service.is_none() {
                    debug!("Component {} not in services list (might be LocalService)",
                        component_name);
                    // Mark as healthy if it's a local service or redirected
                    dep_graph.mark_healthy(component_name);
                    continue;
                }

                let service = service.unwrap().clone();
                let component_spec = component_specs.iter()
                    .find(|s| &s.component_name == component_name)
                    .cloned();

                if component_spec.is_none() {
                    warn!("No spec found for component {}", component_name);
                    continue;
                }

                let component_spec = component_spec.unwrap();

                // Check if redirected
                if self.config.redirected_components.contains_key(component_name) {
                    info!("⏩ {} redirected to external service", component_name);
                    dep_graph.mark_healthy(component_name);
                    continue;
                }

                // Check if local service
                if matches!(component_spec.build_type, rush_build::BuildType::LocalService { .. }) {
                    info!("🏠 {} is a local service (managed separately)", component_name);
                    dep_graph.mark_healthy(component_name);
                    continue;
                }

                let dep_graph_clone = dep_graph.clone();
                let health_manager_clone = health_manager.clone();
                let self_clone = self.clone();
                let built_images_clone = built_images.clone();

                let task = tokio::spawn(async move {
                    self_clone.start_component_with_health_check(
                        service,
                        component_spec,
                        &built_images_clone,
                        health_manager_clone,
                        dep_graph_clone,
                    ).await
                });

                wave_tasks.push((component_name.clone(), task));
            }

            // Wait for all components in this wave to be healthy
            for (component_name, task) in wave_tasks {
                match task.await {
                    Ok(Ok(docker_service)) => {
                        info!("✅ {} is healthy", component_name);
                        dep_graph.mark_healthy(&component_name);
                        running_services.push(docker_service);
                    }
                    Ok(Err(e)) => {
                        error!("❌ {} failed to start: {}", component_name, e);
                        dep_graph.mark_failed(&component_name, e.to_string());

                        // Optionally continue or fail fast
                        return Err(e);
                    }
                    Err(e) => {
                        error!("❌ {} task panicked: {}", component_name, e);
                        dep_graph.mark_failed(&component_name, e.to_string());
                        return Err(Error::Internal(format!("Task panic: {}", e)));
                    }
                }
            }

            info!("✅ Wave {}/{} complete", wave_num + 1, startup_waves.len());
        }

        info!("🎉 All {} services started successfully", running_services.len());
        Ok(running_services)
    }

    async fn start_component_with_health_check(
        &self,
        service: ContainerService,
        spec: ComponentBuildSpec,
        built_images: &HashMap<String, String>,
        health_manager: HealthCheckManager,
        mut dep_graph: DependencyGraph,
    ) -> Result<DockerService> {
        info!("🚀 Starting {}", service.name);
        dep_graph.mark_starting(&service.name);

        // Start the container (existing logic)
        let docker_service = self.start_service(&service, &[spec.clone()], built_images).await?;

        info!("📦 {} container created ({})",
            service.name,
            &docker_service.id()[..12]
        );

        dep_graph.mark_waiting_for_health(&service.name);

        // Check for startup probe first, then health check
        let health_config = spec.startup_probe.or(spec.health_check);

        if let Some(config) = health_config {
            info!("🏥 Waiting for {} to become healthy", service.name);

            health_manager.wait_for_healthy(
                docker_service.id(),
                &service.name,
                &config,
            ).await?;

            info!("💚 {} is healthy", service.name);
        } else {
            // No health check configured, wait a bit for startup
            info!("⏱️  {} has no health check, waiting 2s", service.name);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        // Start log streaming
        self.start_log_streaming_if_needed(
            docker_service.id(),
            &service.name
        ).await;

        Ok(docker_service)
    }
}
```

### Phase 5: Update Docker Client

#### 5.1 Add exec_in_container method
**File**: `rush/crates/rush-docker/src/lib.rs`

```rust
#[async_trait]
pub trait DockerClient: Send + Sync + Debug {
    // ... existing methods ...

    /// Execute command in container and return output
    async fn exec_in_container(
        &self,
        container_id: &str,
        command: &[String]
    ) -> Result<String>;
}
```

**File**: `rush/crates/rush-container/src/docker.rs`

```rust
impl DockerClient for DockerCliClient {
    async fn exec_in_container(
        &self,
        container_id: &str,
        command: &[String],
    ) -> Result<String> {
        let mut args = vec!["exec", container_id];
        for cmd in command {
            args.push(cmd);
        }

        let output = Command::new(&self.docker_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to exec: {}", e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(Error::Docker(format!(
                "Command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }
}
```

### Phase 6: Update Reactor Integration

#### 6.1 Modify Reactor to Use New Startup
**File**: `rush/crates/rush-container/src/reactor/modular_core.rs`

```rust
impl Reactor {
    async fn initial_build(&mut self, components: std::collections::HashSet<String>) -> Result<()> {
        info!("Performing initial build for {} components", components.len());

        // ... existing build logic ...

        // Use new dependency-aware startup
        let running_services = self.lifecycle_manager
            .start_services_with_dependencies(
                self.services.clone(),
                &self.component_specs,
                &self.built_images,
            ).await?;

        info!("Started {} services with dependency ordering", running_services.len());

        // ... rest of method ...
    }
}
```

### Phase 7: Testing Plan

#### 7.1 Unit Tests
1. Test dependency graph construction and cycle detection
2. Test topological sort algorithm
3. Test health check logic for each type
4. Test wave-based startup ordering

#### 7.2 Integration Tests
```rust
#[tokio::test]
async fn test_dependent_startup_order() {
    // Create backend and ingress specs with dependency
    let backend_spec = ComponentBuildSpec {
        component_name: "backend".to_string(),
        depends_on: vec![],
        health_check: Some(HealthCheckConfig {
            check_type: HealthCheckType::Tcp { port: 8080 },
            initial_delay: 1,
            interval: 1,
            success_threshold: 1,
            failure_threshold: 3,
            timeout: 5,
            max_retries: 10,
        }),
        // ...
    };

    let ingress_spec = ComponentBuildSpec {
        component_name: "ingress".to_string(),
        depends_on: vec!["backend".to_string()],
        startup_probe: Some(HealthCheckConfig {
            check_type: HealthCheckType::Dns {
                hosts: vec!["backend.docker".to_string()]
            },
            // ...
        }),
        // ...
    };

    // Start services and verify order
    let services = manager.start_services_with_dependencies(
        services,
        &[backend_spec, ingress_spec],
        &images,
    ).await?;

    // Verify backend started before ingress
    // Verify both are healthy
}
```

### Phase 8: Configuration Examples

#### 8.1 Example stack.spec.yaml
```yaml
backend:
  build_type: "RustBinary"
  location: "backend/server"
  dockerfile: "backend/Dockerfile"
  port: 8129
  target_port: 8080
  mount_point: "/api"
  health_check:
    type: tcp
    port: 8080
    initial_delay: 3
    interval: 5
    success_threshold: 1
    failure_threshold: 3
    timeout: 5
    max_retries: 30

frontend:
  build_type: "TrunkWasm"
  location: "frontend/webui"
  dockerfile: "frontend/Dockerfile"
  port: 8130
  target_port: 80
  mount_point: "/"
  health_check:
    type: http
    path: "/"
    expected_status: 200
    initial_delay: 5
    interval: 5
    success_threshold: 1
    failure_threshold: 3
    timeout: 5
    max_retries: 30

ingress:
  build_type: "Ingress"
  context_dir: "./ingress"
  dockerfile: "./ingress/Dockerfile"
  port: 9000
  target_port: 80
  components:
    - "backend"
    - "frontend"
  depends_on:
    - "backend"
    - "frontend"
  startup_probe:
    type: dns
    hosts:
      - "backend.docker"
      - "frontend.docker"
    initial_delay: 2
    interval: 1
    success_threshold: 1
    failure_threshold: 5
    timeout: 3
    max_retries: 60
```

### Implementation Schedule

**Week 1:**
- [ ] Implement health check types and configuration parsing
- [ ] Add exec_in_container to Docker client
- [ ] Write unit tests for health checks

**Week 2:**
- [ ] Implement dependency graph with cycle detection
- [ ] Add topological sort and wave calculation
- [ ] Write unit tests for dependency graph

**Week 3:**
- [ ] Implement HealthCheckManager
- [ ] Update LifecycleManager with new startup logic
- [ ] Integration testing

**Week 4:**
- [ ] Update Reactor to use new startup
- [ ] Add comprehensive logging
- [ ] End-to-end testing with real containers

**Week 5:**
- [ ] Performance optimization
- [ ] Documentation
- [ ] Edge case handling

### Success Metrics

1. **Reliability**: 99% successful startups (from current ~70-80%)
2. **Startup Time**: Reduced by parallel wave execution
3. **Debuggability**: Clear logs showing dependency resolution
4. **Failure Recovery**: Graceful handling of component failures
5. **Test Coverage**: >90% coverage of new code

### Rollback Plan

If issues arise, we can:
1. Feature flag the new startup logic
2. Fall back to parallel startup (current behavior)
3. Gradually migrate components to use health checks

This plan provides a robust, production-ready solution for dependency-aware container startup with comprehensive health checking.