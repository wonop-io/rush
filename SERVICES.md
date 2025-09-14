# Adding Observability Services to Rush Local Services

## Executive Summary

This document outlines the implementation plan for adding three observability services to Rush's local services framework:
- **Prometheus**: Metrics collection and monitoring
- **Grafana**: Metrics visualization and dashboarding
- **Tempo**: Distributed tracing storage and querying

These services will integrate with the existing local services architecture and provide a complete observability stack for local development.

## Current State Analysis

### Existing Architecture

Rush's local services framework is already well-structured with:
- `LocalServiceType` enum defining available services
- `DockerLocalService` providing generic Docker container management
- `LocalServiceConfig` for service configuration
- Automatic environment variable injection
- Health checking and dependency management

### Integration Points

The observability services will integrate at several levels:
1. **Type System**: Add new variants to `LocalServiceType`
2. **Default Configurations**: Add service-specific defaults and environment variables
3. **Health Checks**: Define appropriate health check commands
4. **Inter-service Dependencies**: Configure proper startup order
5. **Documentation**: Update docs with usage examples

## Implementation Plan

### Phase 1: Core Type Definitions

#### 1.1 Update LocalServiceType Enum

**File**: `rush/crates/rush-local-services/src/types.rs`

Add new variants to the enum:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LocalServiceType {
    // ... existing variants ...

    // Observability Services
    Prometheus,
    Grafana,
    Tempo,
}
```

#### 1.2 Implement Service Defaults

Add to the `impl LocalServiceType` block:

```rust
impl LocalServiceType {
    pub fn default_image(&self) -> String {
        match self {
            // ... existing matches ...
            Self::Prometheus => "prom/prometheus:latest".to_string(),
            Self::Grafana => "grafana/grafana:latest".to_string(),
            Self::Tempo => "grafana/tempo:latest".to_string(),
        }
    }

    pub fn default_port(&self) -> Option<u16> {
        match self {
            // ... existing matches ...
            Self::Prometheus => Some(9090),
            Self::Grafana => Some(3000),
            Self::Tempo => Some(3200),
        }
    }

    pub fn default_health_check(&self) -> Option<String> {
        match self {
            // ... existing matches ...
            Self::Prometheus => Some("curl -f http://localhost:9090/-/healthy".to_string()),
            Self::Grafana => Some("curl -f http://localhost:3000/api/health".to_string()),
            Self::Tempo => Some("curl -f http://localhost:3200/ready".to_string()),
        }
    }

    pub fn env_var_suffix(&self) -> &str {
        match self {
            // ... existing matches ...
            Self::Prometheus => "PROMETHEUS",
            Self::Grafana => "GRAFANA",
            Self::Tempo => "TEMPO",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            // ... existing matches ...
            "prometheus" | "prom" => Self::Prometheus,
            "grafana" => Self::Grafana,
            "tempo" => Self::Tempo,
        }
    }
}
```

### Phase 2: Configuration Defaults

#### 2.1 Update LocalServiceConfig

**File**: `rush/crates/rush-local-services/src/config.rs`

Add service-specific defaults in the `with_defaults()` method:

```rust
impl LocalServiceConfig {
    pub fn with_defaults(mut self) -> Self {
        // ... existing logic ...

        match &self.service_type {
            // ... existing matches ...

            LocalServiceType::Prometheus => {
                // Default command with basic configuration
                if self.command.is_none() {
                    self.command = Some(concat!(
                        "--config.file=/etc/prometheus/prometheus.yml ",
                        "--storage.tsdb.path=/prometheus ",
                        "--web.console.libraries=/usr/share/prometheus/console_libraries ",
                        "--web.console.templates=/usr/share/prometheus/consoles ",
                        "--storage.tsdb.retention.time=15d ",
                        "--web.enable-lifecycle"
                    ).to_string());
                }

                // Default volumes for configuration and data
                if self.persist_data && self.volumes.is_empty() {
                    self.volumes.push(VolumeMapping::new(
                        "./data/prometheus".to_string(),
                        "/prometheus".to_string(),
                        false
                    ));
                }
            }

            LocalServiceType::Grafana => {
                // Default environment variables
                self.env.entry("GF_SECURITY_ADMIN_USER".to_string())
                    .or_insert("admin".to_string());
                self.env.entry("GF_SECURITY_ADMIN_PASSWORD".to_string())
                    .or_insert("admin".to_string());
                self.env.entry("GF_USERS_ALLOW_SIGN_UP".to_string())
                    .or_insert("false".to_string());
                self.env.entry("GF_LOG_LEVEL".to_string())
                    .or_insert("info".to_string());

                // Default volumes for data persistence
                if self.persist_data && self.volumes.is_empty() {
                    self.volumes.push(VolumeMapping::new(
                        "./data/grafana".to_string(),
                        "/var/lib/grafana".to_string(),
                        false
                    ));
                }
            }

            LocalServiceType::Tempo => {
                // Default command with basic configuration
                if self.command.is_none() {
                    self.command = Some("-config.file=/etc/tempo/tempo.yaml".to_string());
                }

                // Default volumes for configuration and data
                if self.persist_data && self.volumes.is_empty() {
                    self.volumes.push(VolumeMapping::new(
                        "./data/tempo".to_string(),
                        "/var/tempo".to_string(),
                        false
                    ));
                }

                // Additional ports for different protocols
                if !self.ports.iter().any(|p| p.container_port == 14268) {
                    self.ports.push(PortMapping::new(14268, 14268)); // Jaeger ingest
                }
                if !self.ports.iter().any(|p| p.container_port == 9411) {
                    self.ports.push(PortMapping::new(9411, 9411)); // Zipkin
                }
                if !self.ports.iter().any(|p| p.container_port == 4317) {
                    self.ports.push(PortMapping::new(4317, 4317)); // OTLP gRPC
                }
                if !self.ports.iter().any(|p| p.container_port == 4318) {
                    self.ports.push(PortMapping::new(4318, 4318)); // OTLP HTTP
                }
            }
        }

        self
    }
}
```

#### 2.2 Add Service Factory Methods

Add convenience constructors to `ServiceDefaults`:

```rust
impl ServiceDefaults {
    pub fn prometheus(name: String) -> LocalServiceConfig {
        LocalServiceConfig::new(name, LocalServiceType::Prometheus).with_defaults()
    }

    pub fn grafana(name: String) -> LocalServiceConfig {
        LocalServiceConfig::new(name, LocalServiceType::Grafana).with_defaults()
    }

    pub fn tempo(name: String) -> LocalServiceConfig {
        LocalServiceConfig::new(name, LocalServiceType::Tempo).with_defaults()
    }

    /// Complete observability stack with proper dependencies
    pub fn observability_stack() -> Vec<LocalServiceConfig> {
        vec![
            Self::prometheus("prometheus".to_string()),
            Self::tempo("tempo".to_string()),
            {
                let mut grafana = Self::grafana("grafana".to_string());
                grafana.depends_on = vec!["prometheus".to_string(), "tempo".to_string()];
                grafana
            }
        ]
    }
}
```

### Phase 3: Connection String Generation

#### 3.1 Update DockerLocalService

**File**: `rush/crates/rush-local-services/src/docker_service.rs`

Add connection string generation in the `generate_connection_string()` method:

```rust
impl DockerLocalService {
    fn generate_connection_string(&self) -> Option<String> {
        match &self.service_type {
            // ... existing matches ...

            LocalServiceType::Prometheus => {
                let port = self.config.ports.first()
                    .map(|p| p.host_port)
                    .unwrap_or(9090);
                Some(format!("http://{}:{}", self.get_container_name(), port))
            }

            LocalServiceType::Grafana => {
                let port = self.config.ports.first()
                    .map(|p| p.host_port)
                    .unwrap_or(3000);
                Some(format!("http://{}:{}", self.get_container_name(), port))
            }

            LocalServiceType::Tempo => {
                let port = self.config.ports.first()
                    .map(|p| p.host_port)
                    .unwrap_or(3200);
                Some(format!("http://{}:{}", self.get_container_name(), port))
            }

            _ => None,
        }
    }

    async fn generated_env_vars(&self) -> Result<HashMap<String, String>> {
        let mut vars = HashMap::new();

        // ... existing logic ...

        // Add observability-specific environment variables
        match &self.service_type {
            LocalServiceType::Prometheus => {
                if let Some(url) = self.generate_connection_string() {
                    vars.insert("PROMETHEUS_URL".to_string(), url.clone());
                    vars.insert("PROMETHEUS_ENDPOINT".to_string(), url);
                }
            }

            LocalServiceType::Grafana => {
                if let Some(url) = self.generate_connection_string() {
                    vars.insert("GRAFANA_URL".to_string(), url);
                }

                // Grafana admin credentials
                if let Some(user) = self.config.env.get("GF_SECURITY_ADMIN_USER") {
                    vars.insert("GRAFANA_ADMIN_USER".to_string(), user.clone());
                }
                if let Some(pass) = self.config.env.get("GF_SECURITY_ADMIN_PASSWORD") {
                    vars.insert("GRAFANA_ADMIN_PASSWORD".to_string(), pass.clone());
                }
            }

            LocalServiceType::Tempo => {
                if let Some(url) = self.generate_connection_string() {
                    vars.insert("TEMPO_URL".to_string(), url.clone());
                    vars.insert("TEMPO_ENDPOINT".to_string(), url);
                }

                // OTLP endpoints
                vars.insert(
                    "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT".to_string(),
                    format!("http://{}:4318/v1/traces", self.get_container_name())
                );
                vars.insert(
                    "OTEL_EXPORTER_OTLP_ENDPOINT".to_string(),
                    format!("http://{}:4317", self.get_container_name())
                );
            }

            _ => {}
        }

        Ok(vars)
    }
}
```

### Phase 4: Default Configuration Files

#### 4.1 Create Default Prometheus Configuration

**File**: `rush/crates/rush-local-services/src/defaults/prometheus.yml`

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

rule_files:
  # - "first_rules.yml"
  # - "second_rules.yml"

scrape_configs:
  - job_name: 'prometheus'
    static_configs:
      - targets: ['localhost:9090']

  # Scrape metrics from applications in the Rush network
  - job_name: 'rush-services'
    dns_sd_configs:
      - names:
          - 'tasks.rush-network'
        type: 'A'
        port: 8080
    scrape_interval: 5s
    metrics_path: /metrics

  # Auto-discovery for services with metrics endpoints
  - job_name: 'docker-services'
    docker_sd_configs:
      - host: unix:///var/run/docker.sock
        port: 8080
        filters:
          - name: network
            values: [rush-network]
    relabel_configs:
      - source_labels: [__meta_docker_container_label_metrics_port]
        target_label: __address__
        regex: (.+)
        replacement: ${1}
```

#### 4.2 Create Default Tempo Configuration

**File**: `rush/crates/rush-local-services/src/defaults/tempo.yaml`

```yaml
server:
  http_listen_port: 3200

distributor:
  receivers:
    jaeger:
      protocols:
        thrift_http:
          endpoint: 0.0.0.0:14268
        grpc:
          endpoint: 0.0.0.0:14250
        thrift_binary:
          endpoint: 0.0.0.0:6832
    zipkin:
      endpoint: 0.0.0.0:9411
    otlp:
      protocols:
        http:
          endpoint: 0.0.0.0:4318
        grpc:
          endpoint: 0.0.0.0:4317

ingester:
  trace_idle_period: 10s
  max_block_bytes: 1_000_000
  max_block_duration: 5m

compactor:
  compaction:
    block_retention: 1h

storage:
  trace:
    backend: local
    local:
      path: /var/tempo/traces
    wal:
      path: /var/tempo/wal
    pool:
      max_workers: 100
      queue_depth: 10000
```

### Phase 5: Documentation Updates

#### 5.1 Update Main Documentation

**File**: `rush/docs/local_services.md`

Add new sections for the observability services:

```markdown
### Prometheus (Metrics Collection)

```yaml
prometheus:
  build_type: "LocalService"
  service_type: "prometheus"
  persist_data: true
  env:
    PROMETHEUS_RETENTION_TIME: "15d"
  volumes:
    - "./prometheus.yml:/etc/prometheus/prometheus.yml:ro"
  health_check: "curl -f http://localhost:9090/-/healthy"
```

**Injected Environment Variables:**
- `PROMETHEUS_URL`: `http://prometheus:9090`
- `PROMETHEUS_ENDPOINT`: `http://prometheus:9090`

**Web Interface:** http://localhost:9090
**Data Location:** `./target/local-services/prometheus/prometheus.data/`

### Grafana (Metrics Visualization)

```yaml
grafana:
  build_type: "LocalService"
  service_type: "grafana"
  persist_data: true
  env:
    GF_SECURITY_ADMIN_USER: admin
    GF_SECURITY_ADMIN_PASSWORD: admin
    GF_USERS_ALLOW_SIGN_UP: "false"
  depends_on:
    - prometheus
    - tempo
  health_check: "curl -f http://localhost:3000/api/health"
```

**Injected Environment Variables:**
- `GRAFANA_URL`: `http://grafana:3000`
- `GRAFANA_ADMIN_USER`: `admin`
- `GRAFANA_ADMIN_PASSWORD`: `admin`

**Web Interface:** http://localhost:3000 (admin/admin)
**Data Location:** `./target/local-services/grafana/grafana.data/`

### Tempo (Distributed Tracing)

```yaml
tempo:
  build_type: "LocalService"
  service_type: "tempo"
  persist_data: true
  volumes:
    - "./tempo.yaml:/etc/tempo/tempo.yaml:ro"
  health_check: "curl -f http://localhost:3200/ready"
```

**Injected Environment Variables:**
- `TEMPO_URL`: `http://tempo:3200`
- `TEMPO_ENDPOINT`: `http://tempo:3200`
- `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`: `http://tempo:4318/v1/traces`
- `OTEL_EXPORTER_OTLP_ENDPOINT`: `http://tempo:4317`

**Ports:**
- 3200: Tempo API
- 14268: Jaeger HTTP
- 9411: Zipkin
- 4317: OTLP gRPC
- 4318: OTLP HTTP

**Data Location:** `./target/local-services/tempo/tempo.data/`
```

#### 5.2 Add Complete Observability Stack Example

```yaml
### Complete Observability Stack

```yaml
# Metrics collection
prometheus:
  build_type: "LocalService"
  service_type: "prometheus"
  persist_data: true
  volumes:
    - "./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml:ro"

# Distributed tracing
tempo:
  build_type: "LocalService"
  service_type: "tempo"
  persist_data: true
  volumes:
    - "./monitoring/tempo.yaml:/etc/tempo/tempo.yaml:ro"

# Visualization dashboard
grafana:
  build_type: "LocalService"
  service_type: "grafana"
  persist_data: true
  env:
    GF_SECURITY_ADMIN_PASSWORD: "secure_password"
  depends_on:
    - prometheus
    - tempo
  volumes:
    - "./monitoring/grafana/datasources:/etc/grafana/provisioning/datasources:ro"
    - "./monitoring/grafana/dashboards:/etc/grafana/provisioning/dashboards:ro"

# Your instrumented application
backend:
  build_type: "RustBinary"
  location: "./backend"
  env:
    # Metrics endpoint for Prometheus scraping
    METRICS_PORT: "8080"
    METRICS_PATH: "/metrics"

    # Tracing configuration (automatically injected)
    OTEL_SERVICE_NAME: "backend-service"
    OTEL_EXPORTER_OTLP_TRACES_ENDPOINT: "${OTEL_EXPORTER_OTLP_TRACES_ENDPOINT}"

    # Optional: Grafana API access
    GRAFANA_API_URL: "${GRAFANA_URL}/api"
    GRAFANA_API_KEY: "your_api_key_here"
```
```

### Phase 6: Example Configurations

#### 6.1 Create Example Stack Configuration

**File**: `rush/examples/observability-stack/stack.spec.yaml`

```yaml
# Complete observability stack example
# Includes metrics collection, visualization, and distributed tracing

# Metrics collection and alerting
prometheus:
  build_type: "LocalService"
  service_type: "prometheus"
  persist_data: true
  env:
    PROMETHEUS_RETENTION_TIME: "30d"
  volumes:
    - "./prometheus/prometheus.yml:/etc/prometheus/prometheus.yml:ro"
    - "./prometheus/rules:/etc/prometheus/rules:ro"
  post_startup_tasks:
    - "echo 'Prometheus started and ready for metrics collection'"

# Distributed tracing storage
tempo:
  build_type: "LocalService"
  service_type: "tempo"
  persist_data: true
  volumes:
    - "./tempo/tempo.yaml:/etc/tempo/tempo.yaml:ro"
  post_startup_tasks:
    - "echo 'Tempo ready for trace ingestion'"

# Metrics and traces visualization
grafana:
  build_type: "LocalService"
  service_type: "grafana"
  persist_data: true
  env:
    GF_SECURITY_ADMIN_PASSWORD: "observability123"
    GF_USERS_ALLOW_SIGN_UP: "false"
    GF_INSTALL_PLUGINS: "grafana-piechart-panel,grafana-worldmap-panel"
  depends_on:
    - prometheus
    - tempo
  volumes:
    - "./grafana/provisioning/datasources:/etc/grafana/provisioning/datasources:ro"
    - "./grafana/provisioning/dashboards:/etc/grafana/provisioning/dashboards:ro"
    - "./grafana/dashboards:/var/lib/grafana/dashboards:ro"
  post_startup_tasks:
    - "echo 'Grafana ready with Prometheus and Tempo datasources'"

# Example microservice with observability
service-a:
  build_type: "RustBinary"
  location: "./services/service-a"
  dockerfile: "./services/service-a/Dockerfile"
  port: 8081
  env:
    OTEL_SERVICE_NAME: "service-a"
    METRICS_PORT: "8081"
    LOG_LEVEL: "info"

service-b:
  build_type: "RustBinary"
  location: "./services/service-b"
  dockerfile: "./services/service-b/Dockerfile"
  port: 8082
  env:
    OTEL_SERVICE_NAME: "service-b"
    METRICS_PORT: "8082"
    LOG_LEVEL: "info"
    SERVICE_A_URL: "http://service-a:8081"
```

### Phase 7: Testing Strategy

#### 7.1 Unit Tests

**File**: `rush/crates/rush-local-services/tests/observability_services_test.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rush_local_services::types::LocalServiceType;
    use rush_local_services::config::{LocalServiceConfig, ServiceDefaults};

    #[test]
    fn test_prometheus_defaults() {
        let config = ServiceDefaults::prometheus("test-prometheus".to_string());
        assert_eq!(config.service_type, LocalServiceType::Prometheus);
        assert_eq!(config.ports[0].host_port, 9090);
        assert!(config.command.is_some());
        assert!(config.get_health_check().is_some());
    }

    #[test]
    fn test_grafana_defaults() {
        let config = ServiceDefaults::grafana("test-grafana".to_string());
        assert_eq!(config.service_type, LocalServiceType::Grafana);
        assert_eq!(config.ports[0].host_port, 3000);
        assert!(config.env.contains_key("GF_SECURITY_ADMIN_USER"));
    }

    #[test]
    fn test_tempo_defaults() {
        let config = ServiceDefaults::tempo("test-tempo".to_string());
        assert_eq!(config.service_type, LocalServiceType::Tempo);
        assert_eq!(config.ports.len(), 5); // Main + 4 protocol ports
        assert!(config.ports.iter().any(|p| p.host_port == 3200)); // Main API
        assert!(config.ports.iter().any(|p| p.host_port == 4317)); // OTLP gRPC
    }

    #[test]
    fn test_observability_stack_dependencies() {
        let stack = ServiceDefaults::observability_stack();
        assert_eq!(stack.len(), 3);

        let grafana = stack.iter().find(|s| s.service_type == LocalServiceType::Grafana).unwrap();
        assert!(grafana.depends_on.contains(&"prometheus".to_string()));
        assert!(grafana.depends_on.contains(&"tempo".to_string()));
    }

    #[test]
    fn test_service_type_parsing() {
        assert_eq!(LocalServiceType::parse("prometheus"), LocalServiceType::Prometheus);
        assert_eq!(LocalServiceType::parse("prom"), LocalServiceType::Prometheus);
        assert_eq!(LocalServiceType::parse("grafana"), LocalServiceType::Grafana);
        assert_eq!(LocalServiceType::parse("tempo"), LocalServiceType::Tempo);
    }
}
```

#### 7.2 Integration Tests

**File**: `rush/crates/rush-cli/tests/observability_integration_test.rs`

```rust
#[cfg(test)]
mod integration_tests {
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_observability_stack_startup() {
        // Test that all three services start in correct order
        // Verify health endpoints
        // Test metric collection
        // Test trace ingestion
        // Test Grafana datasource connectivity
    }

    #[tokio::test]
    #[serial]
    async fn test_prometheus_metrics_endpoint() {
        // Start Prometheus service
        // Verify /metrics endpoint responds
        // Test scraping configuration
    }

    #[tokio::test]
    #[serial]
    async fn test_tempo_trace_ingestion() {
        // Start Tempo service
        // Send test trace via OTLP
        // Verify trace is stored and retrievable
    }
}
```

### Phase 8: CLI Commands Enhancement

#### 8.1 Add Observability-Specific Commands

**File**: `rush/crates/rush-cli/src/commands/local_services.rs`

```rust
impl LocalServicesCommand {
    pub async fn observability_status(&self) -> Result<()> {
        // Check status of observability stack
        // Show metrics ingestion rate
        // Display trace ingestion stats
        // Show Grafana datasource health
    }

    pub async fn setup_observability(&self, product: &str) -> Result<()> {
        // Auto-generate observability stack configuration
        // Create default Prometheus, Grafana, Tempo configs
        // Set up basic dashboards and alerts
    }
}
```

## Configuration Examples

### Basic Observability Stack

```yaml
# Minimal setup - good for getting started
prometheus:
  build_type: "LocalService"
  service_type: "prometheus"
  persist_data: true

grafana:
  build_type: "LocalService"
  service_type: "grafana"
  persist_data: true
  depends_on: ["prometheus"]
```

### Advanced Setup with Custom Configurations

```yaml
# Production-like setup with custom configs
prometheus:
  build_type: "LocalService"
  service_type: "prometheus"
  persist_data: true
  env:
    PROMETHEUS_RETENTION_TIME: "30d"
  volumes:
    - "./config/prometheus.yml:/etc/prometheus/prometheus.yml:ro"
    - "./config/alerts.yml:/etc/prometheus/alerts.yml:ro"
  command: >
    --config.file=/etc/prometheus/prometheus.yml
    --storage.tsdb.path=/prometheus
    --storage.tsdb.retention.time=30d
    --web.enable-admin-api
    --web.enable-lifecycle

tempo:
  build_type: "LocalService"
  service_type: "tempo"
  persist_data: true
  volumes:
    - "./config/tempo.yaml:/etc/tempo/tempo.yaml:ro"
  env:
    TEMPO_MAX_BLOCK_BYTES: "100_000_000"

grafana:
  build_type: "LocalService"
  service_type: "grafana"
  persist_data: true
  env:
    GF_SECURITY_ADMIN_PASSWORD: "${GRAFANA_ADMIN_PASSWORD}"
    GF_USERS_ALLOW_SIGN_UP: "false"
    GF_AUTH_ANONYMOUS_ENABLED: "false"
    GF_INSTALL_PLUGINS: "grafana-piechart-panel"
  depends_on: ["prometheus", "tempo"]
  volumes:
    - "./config/grafana/datasources:/etc/grafana/provisioning/datasources:ro"
    - "./config/grafana/dashboards:/etc/grafana/provisioning/dashboards:ro"
```

## Benefits

1. **Complete Observability**: Metrics, logs (via application), and traces in one stack
2. **Zero Configuration**: Works out of the box with sensible defaults
3. **Development Focused**: Optimized for local development workflows
4. **Production Parity**: Same tools used in production environments
5. **Rush Integration**: Seamless integration with existing Rush services
6. **Auto-Discovery**: Automatic service discovery and configuration
7. **Persistent Data**: Retain dashboards and historical data between restarts

## Timeline

- **Phase 1-2** (Type definitions and configs): 2-3 days
- **Phase 3** (Connection strings and env vars): 1-2 days
- **Phase 4** (Default config files): 1 day
- **Phase 5** (Documentation): 2-3 days
- **Phase 6** (Examples): 1-2 days
- **Phase 7** (Testing): 2-3 days
- **Phase 8** (CLI enhancements): 1-2 days

**Total estimated time: 10-16 days**

## Success Criteria

1. All three services start correctly with default configurations
2. Services have proper health checks and dependency ordering
3. Environment variables are correctly injected
4. Grafana can connect to both Prometheus and Tempo as datasources
5. Example applications can send metrics to Prometheus and traces to Tempo
6. Documentation is complete with working examples
7. Integration tests pass consistently
8. Performance impact on Rush startup is minimal (<10% increase)

## Risk Mitigation

1. **Complex Dependencies**: Use proper `depends_on` configuration and health checks
2. **Resource Usage**: Set appropriate resource limits and document requirements
3. **Configuration Complexity**: Provide good defaults and comprehensive documentation
4. **Version Compatibility**: Pin specific versions and test compatibility
5. **Network Issues**: Use Rush's existing network management
6. **Data Persistence**: Follow existing patterns for volume management

This implementation will provide Rush users with a complete, production-ready observability stack for local development, making it easy to instrument applications and understand their behavior during development.