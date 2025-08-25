# Container Reactor Refactoring Plan

## Executive Summary

The `reactor.rs` file is currently 2,503 lines and contains 38+ methods handling multiple responsibilities. This plan outlines a systematic refactoring to split it into smaller, focused, testable components while addressing 11 existing TODOs.

## Current State Analysis

### File Statistics
- **Lines of Code**: 2,503
- **Methods**: 38+
- **TODOs**: 11
- **Responsibilities**: 8+ major areas

### Current Responsibilities
1. **Container Lifecycle Management** - Starting, stopping, monitoring containers
2. **Build Orchestration** - Building images, handling build errors
3. **File Watching** - Detecting changes and triggering rebuilds
4. **Kubernetes Operations** - Apply, deploy, rollout operations
5. **Secret Management** - Injecting secrets into containers
6. **Event Handling** - Processing container and file system events
7. **Service Collection Management** - Managing multiple services
8. **Output/Logging** - Handling container output streams

### Identified Problems
1. **God Object Anti-pattern**: Single struct managing too many responsibilities
2. **Poor Testability**: Large methods with multiple side effects
3. **Tight Coupling**: Direct dependencies on Docker, file system, and external tools
4. **TODO Debt**: 11 unimplemented features scattered throughout
5. **State Management**: Complex mutable state with unclear boundaries

## Proposed Architecture

### Core Design Principles
1. **Single Responsibility**: Each component handles one concern
2. **Dependency Injection**: Use traits for external dependencies
3. **Event-Driven**: Components communicate via events
4. **Testability First**: Small, pure functions with clear interfaces
5. **Async/Await Patterns**: Proper async boundaries and cancellation

### Component Breakdown

```
rush-container/src/
├── reactor/
│   ├── mod.rs                 # Public API and core reactor
│   ├── config.rs              # ContainerReactorConfig
│   ├── state.rs               # Reactor state management
│   └── errors.rs              # Reactor-specific errors
├── lifecycle/
│   ├── mod.rs                 # Lifecycle orchestration
│   ├── manager.rs             # Container lifecycle manager
│   ├── monitor.rs             # Container health monitoring
│   └── shutdown.rs            # Graceful shutdown handling
├── build/
│   ├── mod.rs                 # (existing)
│   ├── processor.rs           # (existing)
│   ├── error.rs               # (existing)
│   ├── orchestrator.rs        # NEW: Build orchestration
│   └── cache.rs               # NEW: Build caching
├── events/
│   ├── mod.rs                 # Event system core
│   ├── types.rs               # Event type definitions
│   ├── bus.rs                 # Event bus implementation
│   └── handlers.rs            # Event handler traits
├── kubernetes/
│   ├── mod.rs                 # Kubernetes operations
│   ├── client.rs              # Kubectl wrapper
│   ├── manifest.rs            # Manifest generation
│   └── operations.rs          # Apply/deploy/rollout
├── secrets/
│   ├── mod.rs                 # Secret management
│   ├── injector.rs            # Secret injection into containers
│   └── provider.rs            # Secret provider trait
└── network/
    ├── mod.rs                 # (existing)
    └── setup.rs               # Network initialization

```

## Refactoring Phases

### Phase 1: Foundation (Week 1)
**Goal**: Establish the new structure without breaking existing functionality

#### 1.1 Create Event System
- [ ] Create `events/` module with event types
- [ ] Implement event bus for decoupled communication
- [ ] Define events: BuildStarted, BuildCompleted, ContainerStarted, ContainerStopped, FileChanged, etc.

#### 1.2 Extract Configuration
- [ ] Move `ContainerReactorConfig` to `reactor/config.rs`
- [ ] Create `ReactorState` in `reactor/state.rs` for mutable state
- [ ] Define clear state transitions

#### 1.3 Setup Test Infrastructure
- [ ] Create test utilities module
- [ ] Mock implementations for Docker, filesystem, kubectl
- [ ] Integration test harness

**Deliverables**:
- Event system with 100% test coverage
- Separated configuration and state
- Test infrastructure ready

### Phase 2: Lifecycle Extraction (Week 2)
**Goal**: Extract container lifecycle management

#### 2.1 Create Lifecycle Manager
- [ ] Extract container start/stop/monitor logic to `lifecycle/manager.rs`
- [ ] Move health checking to `lifecycle/monitor.rs`
- [ ] Extract shutdown logic to `lifecycle/shutdown.rs`

#### 2.2 Refactor Container Operations
- [ ] Methods to extract:
  - `launch_containers()` → `LifecycleManager::start_services()`
  - `cleanup_containers()` → `LifecycleManager::stop_services()`
  - `monitor_and_handle_events()` → `LifecycleMonitor::monitor()`
  - `kill_and_remove_container_with_retry()` → `ShutdownManager::force_stop()`

#### 2.3 Add Lifecycle Tests
- [ ] Unit tests for each lifecycle component
- [ ] Integration tests for full lifecycle scenarios
- [ ] Test error handling and retry logic

**Deliverables**:
- Lifecycle module with 80%+ test coverage
- Clear separation of start/stop/monitor concerns
- Improved error handling

### Phase 3: Build System Refactoring (Week 3)
**Goal**: Improve build orchestration and caching

#### 3.1 Build Orchestrator
- [ ] Create `build/orchestrator.rs` for build coordination
- [ ] Extract methods:
  - `build_all()` → `BuildOrchestrator::build_components()`
  - `build_image()` → `BuildOrchestrator::build_single()`
  - `render_artifacts_for_component()` → `BuildOrchestrator::prepare_artifacts()`

#### 3.2 Implement Build Cache
- [ ] Create `build/cache.rs` for build caching logic
- [ ] Implement cache invalidation based on file changes
- [ ] Add cache hit/miss metrics

#### 3.3 Address Build TODOs
- [ ] TODO: Store built images properly (line 1060)
- [ ] TODO: Configurable target platform (line 1101)
- [ ] TODO: Proper environment template computation (line 2058)

**Deliverables**:
- Improved build system with caching
- Resolved build-related TODOs
- Build performance metrics

### Phase 4: Kubernetes Operations (Week 4)
**Goal**: Implement missing Kubernetes functionality

#### 4.1 Kubernetes Client
- [ ] Create `kubernetes/client.rs` wrapping kubectl
- [ ] Implement context selection
- [ ] Add proper error handling for kubectl operations

#### 4.2 Manifest Management
- [ ] Create `kubernetes/manifest.rs` for manifest generation
- [ ] Template-based manifest generation
- [ ] Environment-specific customization

#### 4.3 Implement K8s Operations
- [ ] TODO: Implement Docker push (line 535)
- [ ] TODO: Implement kubectl context selection (line 547)
- [ ] TODO: Generate and apply K8s manifests (line 561)
- [ ] TODO: Delete K8s resources (line 571)
- [ ] TODO: Install/uninstall K8s manifests (lines 581, 591)
- [ ] TODO: Generate K8s manifests from templates (line 601)

**Deliverables**:
- Complete Kubernetes integration
- All K8s TODOs resolved
- Kubernetes operations with proper testing

### Phase 5: Secret Management (Week 5)
**Goal**: Improve secret handling and injection

#### 5.1 Secret Injector
- [ ] Create `secrets/injector.rs` for container secret injection
- [ ] Support multiple secret providers
- [ ] Implement secret rotation support

#### 5.2 Secret Provider Interface
- [ ] Define trait for secret providers
- [ ] Implement for existing vault types
- [ ] Add secret validation

**Deliverables**:
- Modular secret management
- Support for multiple secret sources
- Secret rotation capability

### Phase 6: Final Integration (Week 6)
**Goal**: Complete the refactoring and ensure backward compatibility

#### 6.1 Reactor Core Refactoring
- [ ] Slim down `ContainerReactor` to orchestration only
- [ ] Use dependency injection for all components
- [ ] Implement via event-driven architecture

#### 6.2 Migration and Compatibility
- [ ] Ensure all existing APIs work
- [ ] Create migration guide for API changes
- [ ] Performance benchmarking

#### 6.3 Documentation and Testing
- [ ] Complete documentation for all modules
- [ ] Achieve 80%+ test coverage overall
- [ ] Integration test suite covering all scenarios

**Deliverables**:
- Fully refactored reactor system
- Complete test coverage
- Performance benchmarks

## Implementation Guidelines

### Testing Strategy

#### Unit Testing
- Each module should have accompanying tests
- Use mockall for trait mocking
- Test error conditions explicitly
- Aim for 80%+ coverage per module

#### Integration Testing
- Test complete workflows end-to-end
- Use testcontainers for Docker testing
- Test failure scenarios and recovery
- Performance regression tests

#### Example Test Structure
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;

    #[tokio::test]
    async fn test_lifecycle_start_stop() {
        // Given
        let mock_docker = MockDockerClient::new();
        let manager = LifecycleManager::new(mock_docker);
        
        // When
        let result = manager.start_service("test").await;
        
        // Then
        assert!(result.is_ok());
    }
}
```

### Code Quality Standards

#### Method Size
- No method longer than 50 lines
- Complex methods split into smaller helpers
- Clear single responsibility per method

#### Error Handling
- Use Result<T, Error> consistently
- Specific error types per module
- Proper error context and chaining

#### Documentation
- Module-level documentation
- Public API documentation
- Example usage in docs

### Dependency Management

#### Trait Definitions
```rust
#[async_trait]
pub trait ContainerManager: Send + Sync {
    async fn start(&self, config: ContainerConfig) -> Result<ContainerId>;
    async fn stop(&self, id: &ContainerId) -> Result<()>;
    async fn status(&self, id: &ContainerId) -> Result<ContainerStatus>;
}
```

#### Dependency Injection
```rust
pub struct LifecycleManager {
    container_manager: Arc<dyn ContainerManager>,
    event_bus: Arc<dyn EventBus>,
    monitor: Arc<dyn HealthMonitor>,
}
```

## Success Metrics

### Quantitative
- **Code Reduction**: Reactor.rs reduced from 2,503 to <500 lines
- **Test Coverage**: Overall 80%+, critical paths 95%+
- **Method Complexity**: No method >50 lines
- **TODO Resolution**: All 11 TODOs addressed

### Qualitative
- **Modularity**: Clear separation of concerns
- **Testability**: Easy to test individual components
- **Maintainability**: New features easy to add
- **Performance**: No regression in container startup time

## Risk Mitigation

### Backward Compatibility
- Maintain existing public APIs
- Deprecate rather than remove
- Provide migration guide

### Testing Strategy
- Each phase fully tested before next
- Integration tests maintained throughout
- Performance benchmarks at each phase

### Rollback Plan
- Git branches for each phase
- Feature flags for new implementations
- Ability to revert per-module

## Timeline Summary

| Phase | Duration | Focus | Deliverable |
|-------|----------|-------|-------------|
| 1 | Week 1 | Foundation | Event system, test infrastructure |
| 2 | Week 2 | Lifecycle | Container lifecycle management |
| 3 | Week 3 | Build | Build orchestration and caching |
| 4 | Week 4 | Kubernetes | K8s operations implementation |
| 5 | Week 5 | Secrets | Secret management improvements |
| 6 | Week 6 | Integration | Final refactoring and testing |

## Appendix: TODO Locations and Resolutions

| Line | TODO | Resolution | Phase |
|------|------|------------|-------|
| 427 | ID placeholder | Generate proper container IDs | 2 |
| 535 | Docker push | Implement registry push | 4 |
| 547 | Kubectl context | Context selection logic | 4 |
| 561 | Apply manifests | Manifest application | 4 |
| 571 | Delete resources | Resource cleanup | 4 |
| 581 | Install manifests | Installation logic | 4 |
| 591 | Uninstall manifests | Uninstallation logic | 4 |
| 601 | Generate manifests | Template generation | 4 |
| 1060 | Store built images | Image registry management | 3 |
| 1101 | Target platform | Configuration system | 3 |
| 2058 | Environment templates | Template resolution | 3 |

## Next Steps

1. **Review and Approve Plan**: Team review and feedback
2. **Create Tracking Issues**: GitHub issues for each phase
3. **Assign Ownership**: Designate leads per module
4. **Begin Phase 1**: Start with event system foundation

---

*This plan provides a systematic approach to refactoring the reactor.rs file into a maintainable, testable architecture while ensuring all existing functionality is preserved and enhanced.*