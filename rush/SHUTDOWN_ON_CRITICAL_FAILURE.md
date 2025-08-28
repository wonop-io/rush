# System-Wide Shutdown on Critical Container Failure

## Executive Summary

This document outlines a comprehensive plan for implementing system-wide shutdown when a critical Docker container exits with a non-zero exit code. The proposed architecture builds upon Rush's existing infrastructure including the `ShutdownCoordinator`, event system, container monitoring, and component priority/dependency mechanisms.

## Problem Statement

Currently, when a container exits with a non-zero exit code, Rush handles it as an isolated failure. However, in production-like environments, certain containers are so critical that their failure should trigger a complete system shutdown to prevent partial state, data corruption, or undefined behavior.

**Use Cases:**
- **Database container crashes**: Application containers should not continue without data access
- **Authentication service fails**: System security compromised  
- **Core API gateway exits**: Traffic routing impossible
- **Critical infrastructure components**: System cannot function properly

## Current Architecture Analysis

### Existing Components

1. **Container Status Detection** (`rush-docker/src/status.rs`)
   - `ContainerStatus::Exited(i32)` captures exit codes
   - `container_status()` API provides real-time status

2. **Event System** (`rush-container/src/events/types.rs`)
   - `ContainerEvent::ContainerStopped` with exit code and reason
   - Event propagation via `EventBus`

3. **Health Monitoring** (`rush-container/src/lifecycle/monitor.rs`)
   - Continuous container status checking
   - Health status transitions and failure detection

4. **Global Shutdown** (`rush-core/src/shutdown.rs`)
   - `ShutdownCoordinator` with `ShutdownReason` enum
   - Cancellation tokens and graceful termination

5. **Component Priority System** (`rush-config/src/product/types.rs`)
   - `priority: u64` field for startup/shutdown ordering
   - `depends_on: Vec<String>` for dependency tracking

### Current Gaps

1. **No Critical Component Classification**: Components aren't marked as system-critical
2. **No Failure Impact Analysis**: Exit code failures don't trigger shutdown assessment
3. **No Dependency-Based Shutdown**: Component failures don't consider dependents
4. **No Configurable Criticality**: Users can't specify which failures should cause system shutdown

## Proposed Architecture

### 1. Component Criticality Classification

#### Stack Configuration Extension

```yaml
backend:
  build_type: "RustBinary"
  location: "backend/server"
  dockerfile: "backend/Dockerfile"
  priority: 50
  critical: true                    # NEW: Mark as system-critical
  failure_policy: "shutdown_all"    # NEW: What to do on failure
  grace_period: "30s"              # NEW: Time before forced shutdown

database:
  build_type: "LocalService"
  service_type: "postgresql"
  critical: true
  failure_policy: "shutdown_dependents"
  critical_exit_codes: [1, 2, 3]   # NEW: Which exit codes trigger shutdown

frontend:
  build_type: "TrunkWasm" 
  location: "frontend/webui"
  critical: false                   # Non-critical component
  failure_policy: "restart"        # Just restart on failure
```

#### Configuration Types

```rust
#[derive(Debug, Clone)]
pub enum FailurePolicy {
    /// Restart the component only (current behavior)
    Restart,
    /// Shutdown all dependent components
    ShutdownDependents,
    /// Shutdown entire system
    ShutdownAll,
    /// Custom script/command to execute
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct CriticalityConfig {
    /// Whether this component is critical to system operation
    pub critical: bool,
    /// What to do when this component fails
    pub failure_policy: FailurePolicy,
    /// Exit codes that trigger the failure policy (default: any non-zero)
    pub critical_exit_codes: Vec<i32>,
    /// Grace period before forced shutdown
    pub grace_period: Duration,
    /// Max restart attempts before applying failure policy
    pub max_restart_attempts: u32,
}
```

### 2. Enhanced Container Monitoring

#### Failure Detection Engine

```rust
// rush-container/src/lifecycle/failure_detector.rs
pub struct FailureDetector {
    config: Arc<Config>,
    shutdown_coordinator: Arc<ShutdownCoordinator>,
    event_bus: EventBus,
    component_registry: ComponentRegistry,
}

impl FailureDetector {
    /// Process container exit event and determine if system shutdown is required
    pub async fn handle_container_exit(
        &self,
        component: &str,
        container_id: &str,
        exit_code: i32,
        reason: StopReason,
    ) -> Result<ShutdownDecision> {
        
        let component_spec = self.component_registry.get_component(component)?;
        let criticality = component_spec.criticality_config();
        
        // Check if this exit code should trigger the failure policy
        if self.should_trigger_policy(exit_code, criticality) {
            match criticality.failure_policy {
                FailurePolicy::ShutdownAll => {
                    return Ok(ShutdownDecision::ShutdownSystem {
                        reason: format!("Critical component '{}' exited with code {}", 
                                      component, exit_code),
                        grace_period: criticality.grace_period,
                    });
                }
                
                FailurePolicy::ShutdownDependents => {
                    let dependents = self.find_dependents(component).await?;
                    return Ok(ShutdownDecision::ShutdownComponents {
                        components: dependents,
                        reason: format!("Dependency '{}' failed", component),
                    });
                }
                
                FailurePolicy::Restart => {
                    // Check restart attempts
                    let attempts = self.get_restart_count(component).await;
                    if attempts >= criticality.max_restart_attempts {
                        return Ok(ShutdownDecision::ShutdownSystem {
                            reason: format!("Component '{}' exceeded restart limit", component),
                            grace_period: criticality.grace_period,
                        });
                    }
                }
                
                FailurePolicy::Custom(script) => {
                    return Ok(ShutdownDecision::ExecuteCustom {
                        script: script.clone(),
                        component: component.to_string(),
                        exit_code,
                    });
                }
            }
        }
        
        Ok(ShutdownDecision::Continue)
    }
}

#[derive(Debug)]
pub enum ShutdownDecision {
    /// Continue normal operation
    Continue,
    /// Shutdown entire system
    ShutdownSystem { reason: String, grace_period: Duration },
    /// Shutdown specific components
    ShutdownComponents { components: Vec<String>, reason: String },
    /// Execute custom failure handling
    ExecuteCustom { script: String, component: String, exit_code: i32 },
}
```

### 3. Dependency Analysis Engine

#### Component Dependency Graph

```rust
// rush-container/src/lifecycle/dependency_graph.rs
pub struct DependencyGraph {
    components: HashMap<String, ComponentNode>,
    edges: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    /// Find all components that depend on this component (directly or transitively)
    pub fn find_dependents(&self, component: &str) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut dependents = Vec::new();
        self.dfs_dependents(component, &mut visited, &mut dependents);
        dependents
    }
    
    /// Find components that must be shut down first (reverse topological order)
    pub fn shutdown_order(&self, failed_component: &str) -> Vec<String> {
        let dependents = self.find_dependents(failed_component);
        self.topological_sort_reverse(&dependents)
    }
    
    /// Analyze failure impact and generate shutdown plan
    pub fn analyze_failure_impact(&self, component: &str) -> FailureImpact {
        let dependents = self.find_dependents(component);
        let critical_dependents = dependents.iter()
            .filter(|c| self.is_critical(c))
            .cloned()
            .collect();
            
        FailureImpact {
            failed_component: component.to_string(),
            affected_components: dependents,
            critical_components_affected: critical_dependents,
            recommended_action: self.recommend_action(component),
        }
    }
}

#[derive(Debug)]
pub struct FailureImpact {
    pub failed_component: String,
    pub affected_components: Vec<String>,
    pub critical_components_affected: Vec<String>,
    pub recommended_action: RecommendedAction,
}

#[derive(Debug)]
pub enum RecommendedAction {
    RestartComponent,
    ShutdownDependents,
    ShutdownSystem,
    PartialShutdown(Vec<String>),
}
```

### 4. Enhanced Event System

#### New Event Types

```rust
// rush-container/src/events/types.rs (additions)
#[derive(Debug, Clone)]
pub enum ContainerEvent {
    // ... existing events ...
    
    /// Critical container failure detected
    CriticalFailureDetected {
        component: String,
        container_id: String,
        exit_code: i32,
        failure_policy: FailurePolicy,
        impact_analysis: FailureImpact,
    },
    
    /// System shutdown initiated due to critical failure
    SystemShutdownTriggered {
        trigger_component: String,
        reason: String,
        grace_period: Duration,
        affected_components: Vec<String>,
    },
    
    /// Dependency-based shutdown initiated
    DependencyShutdownTriggered {
        failed_dependency: String,
        components_to_shutdown: Vec<String>,
        reason: String,
    },
    
    /// Custom failure handler executed
    CustomFailureHandlerExecuted {
        component: String,
        script: String,
        exit_code: i32,
        success: bool,
    },
}
```

### 5. Shutdown Orchestration Engine

#### Coordinated Shutdown Process

```rust
// rush-container/src/lifecycle/shutdown_orchestrator.rs
pub struct ShutdownOrchestrator {
    shutdown_coordinator: Arc<ShutdownCoordinator>,
    lifecycle_manager: Arc<LifecycleManager>,
    dependency_graph: DependencyGraph,
    event_bus: EventBus,
}

impl ShutdownOrchestrator {
    /// Execute system-wide shutdown due to critical failure
    pub async fn execute_critical_shutdown(
        &self,
        trigger_component: &str,
        reason: String,
        grace_period: Duration,
    ) -> Result<()> {
        info!("Executing critical shutdown: {} (triggered by {})", reason, trigger_component);
        
        // 1. Emit shutdown event
        self.event_bus.emit(ContainerEvent::SystemShutdownTriggered {
            trigger_component: trigger_component.to_string(),
            reason: reason.clone(),
            grace_period,
            affected_components: self.dependency_graph.all_components(),
        }).await?;
        
        // 2. Calculate shutdown order (reverse dependency order)
        let shutdown_order = self.dependency_graph.shutdown_order("all");
        
        // 3. Start graceful shutdown timer
        let shutdown_future = self.graceful_shutdown_sequence(shutdown_order);
        let timeout_future = tokio::time::sleep(grace_period);
        
        tokio::select! {
            result = shutdown_future => {
                match result {
                    Ok(_) => info!("Graceful shutdown completed successfully"),
                    Err(e) => error!("Graceful shutdown failed: {}", e),
                }
            }
            _ = timeout_future => {
                warn!("Grace period expired, forcing shutdown");
                self.force_shutdown().await?;
            }
        }
        
        // 4. Signal global shutdown
        self.shutdown_coordinator.shutdown(ShutdownReason::Error(reason));
        
        Ok(())
    }
    
    /// Execute dependency-based shutdown
    pub async fn execute_dependency_shutdown(
        &self,
        failed_component: &str,
        components_to_shutdown: Vec<String>,
        reason: String,
    ) -> Result<()> {
        info!("Shutting down dependents of '{}': {:?}", failed_component, components_to_shutdown);
        
        // Emit event
        self.event_bus.emit(ContainerEvent::DependencyShutdownTriggered {
            failed_dependency: failed_component.to_string(),
            components_to_shutdown: components_to_shutdown.clone(),
            reason: reason.clone(),
        }).await?;
        
        // Calculate proper shutdown order for affected components
        let shutdown_order = self.dependency_graph
            .topological_sort_reverse(&components_to_shutdown);
        
        // Shutdown components in order
        for component in shutdown_order {
            if let Err(e) = self.lifecycle_manager.stop_component(&component).await {
                error!("Failed to stop component '{}': {}", component, e);
            }
        }
        
        Ok(())
    }
}
```

### 6. Configuration Integration

#### Component Spec Extensions

```rust
// rush-build/src/spec.rs (additions)
#[derive(Debug, Clone)]
pub struct ComponentBuildSpec {
    // ... existing fields ...
    
    /// Criticality configuration for this component
    pub criticality_config: CriticalityConfig,
}

impl ComponentBuildSpec {
    /// Parse criticality configuration from YAML
    fn parse_criticality(yaml_section: &yaml_rust::Yaml) -> CriticalityConfig {
        CriticalityConfig {
            critical: yaml_section.get("critical")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            failure_policy: yaml_section.get("failure_policy")
                .and_then(|v| v.as_str())
                .map(|s| match s {
                    "restart" => FailurePolicy::Restart,
                    "shutdown_dependents" => FailurePolicy::ShutdownDependents,
                    "shutdown_all" => FailurePolicy::ShutdownAll,
                    custom => FailurePolicy::Custom(custom.to_string()),
                })
                .unwrap_or(FailurePolicy::Restart),
            critical_exit_codes: yaml_section.get("critical_exit_codes")
                .and_then(|v| v.as_vec())
                .map(|vec| vec.iter().filter_map(|v| v.as_i64().map(|i| i as i32)).collect())
                .unwrap_or_else(|| (1..256).collect()), // All non-zero by default
            grace_period: yaml_section.get("grace_period")
                .and_then(|v| v.as_str())
                .and_then(|s| parse_duration::parse(s).ok())
                .unwrap_or_else(|| Duration::from_secs(30)),
            max_restart_attempts: yaml_section.get("max_restart_attempts")
                .and_then(|v| v.as_i64())
                .map(|i| i as u32)
                .unwrap_or(3),
        }
    }
}
```

## Implementation Plan

### Phase 1: Foundation (Week 1-2)

1. **Create Basic Types**
   - Add `CriticalityConfig` and `FailurePolicy` enums
   - Extend `ComponentBuildSpec` with criticality fields
   - Update YAML parsing to handle new configuration options

2. **Enhance Event System**
   - Add new `ContainerEvent` variants for critical failures
   - Update event handlers to process criticality events

3. **Basic Failure Detection**
   - Create `FailureDetector` with basic exit code analysis
   - Integrate with existing container monitoring

### Phase 2: Core Logic (Week 3-4)

4. **Dependency Graph Engine**
   - Implement `DependencyGraph` for component relationship analysis
   - Add failure impact analysis
   - Create shutdown ordering algorithms

5. **Shutdown Orchestration**
   - Build `ShutdownOrchestrator` for coordinated shutdowns
   - Implement graceful vs. forced shutdown logic
   - Add timeout handling and partial failure recovery

### Phase 3: Integration & Testing (Week 5-6)

6. **Integration with Existing Systems**
   - Wire failure detector into health monitoring
   - Connect shutdown orchestrator with lifecycle manager
   - Update CLI commands to respect criticality settings

7. **Comprehensive Testing**
   - Unit tests for all new components
   - Integration tests for shutdown scenarios
   - End-to-end testing with real container failures

### Phase 4: Production Features (Week 7-8)

8. **Advanced Features**
   - Custom failure handler execution
   - Configurable restart policies and backoff
   - Metrics and observability for failure events

9. **Documentation and Examples**
   - Update user documentation
   - Create example configurations
   - Add troubleshooting guides

## Configuration Examples

### Basic Critical Component

```yaml
database:
  build_type: "LocalService"
  service_type: "postgresql"
  critical: true                    # This component is critical
  failure_policy: "shutdown_all"    # Any failure shuts down everything
  grace_period: "30s"              # Allow 30 seconds for graceful shutdown
```

### Dependency-Aware Failure Handling

```yaml
auth_service:
  build_type: "RustBinary"
  location: "auth/server"
  critical: true
  failure_policy: "shutdown_dependents"  # Only shutdown things that depend on auth
  critical_exit_codes: [1, 2, 3, 139]   # Specific exit codes that trigger policy

api_gateway:
  build_type: "RustBinary" 
  location: "gateway/server"
  depends_on: ["auth_service"]           # Will be shut down if auth fails
  failure_policy: "restart"             # Non-critical, just restart
  max_restart_attempts: 5
```

### Custom Failure Handling

```yaml
monitoring:
  build_type: "RustBinary"
  location: "monitoring/server"
  critical: true
  failure_policy: "custom:./scripts/monitoring_failure.sh"
  grace_period: "60s"
```

## Benefits

1. **Operational Safety**: Prevents partial system states that could cause data corruption
2. **Dependency Awareness**: Understands component relationships and cascading failures  
3. **Configurable Policies**: Teams can define their own criticality and failure handling
4. **Graceful Degradation**: Allows for controlled shutdown rather than hanging processes
5. **Observability**: Rich events and logging for failure analysis
6. **Backward Compatibility**: Non-breaking changes with sensible defaults

## Risks and Mitigations

### Risk: False Positive Shutdowns
**Mitigation:**
- Careful configuration of `critical_exit_codes` 
- Restart attempt limits before triggering shutdown
- Extensive testing of failure scenarios
- Configurable grace periods and confirmation mechanisms

### Risk: Cascading Failures
**Mitigation:** 
- Proper dependency analysis to avoid unnecessary shutdowns
- Timeout mechanisms to prevent infinite shutdown loops
- Circuit breaker patterns for flapping components
- Administrative override mechanisms

### Risk: Complex Configuration
**Mitigation:**
- Sensible defaults that work for most cases
- Clear documentation and examples
- Configuration validation and helpful error messages
- Gradual rollout with opt-in criticality

## Alternative Approaches Considered

1. **External Orchestrator**: Use Kubernetes or Docker Compose health checks
   - **Rejected**: Adds external dependency and reduces Rush's autonomy
   
2. **Process-Level Monitoring**: Monitor at OS process level instead of container
   - **Rejected**: Less reliable and doesn't leverage Docker's container lifecycle
   
3. **Simple All-or-Nothing**: Just shut down everything on any container failure
   - **Rejected**: Too aggressive, doesn't allow for partial degradation

## Conclusion

The proposed architecture provides a comprehensive, configurable solution for handling critical container failures in Rush. It builds naturally on existing infrastructure while adding sophisticated failure analysis and coordinated shutdown capabilities. The phased implementation approach allows for gradual rollout and validation of the system.

This solution transforms Rush from a simple container orchestrator into a production-ready system that can handle real-world failure scenarios with grace and intelligence.