# Rush CLI Refactoring Plan

## 1. Current Issues Identified

1. **Monolithic Files**: Several files are extremely large (e.g., container_reactor.rs at 1346 lines, main.rs at 936 lines)
2. **High Coupling**: Components have many dependencies, making testing difficult
3. **Mixed Responsibilities**: Many modules handle multiple concerns
4. **Limited Test Coverage**: The test directory is relatively small compared to the codebase
5. **Excessive Function Sizes**: Many functions are very large and cover multiple responsibilities

## 2. Refactoring Goals

1. **Improved Testability**: Create smaller, pure functions that are easier to unit test
2. **Reduced Coupling**: Use more interfaces and dependency injection
3. **Clear Separation of Concerns**: Each module should have a single responsibility
4. **Consistent Error Handling**: Standardize error handling throughout the codebase
5. **Better Documentation**: Improve code documentation for better maintainability

## 3. Proposed Architecture Restructuring

### 3.1. Project Structure Refactoring

```
rush/
└── src/
    ├── cli/               # Command-line interface logic
    │   ├── args.rs        # Argument parsing
    │   ├── commands/      # Individual command implementations
    │   └── mod.rs
    ├── core/              # Core domain models and business logic
    │   ├── config/        # Configuration management
    │   ├── environment/   # Environment management
    │   ├── product/       # Product management
    │   └── mod.rs
    ├── build/             # Building capabilities
    │   ├── context.rs     # Build context
    │   ├── script.rs      # Build script processing
    │   ├── artefact.rs    # Build artifacts
    │   ├── variables.rs   # Build variables
    │   └── mod.rs
    ├── container/         # Container management
    │   ├── docker.rs      # Docker API interactions
    │   ├── network.rs     # Network management
    │   ├── lifecycle/     # Container lifecycle management
    │   │   ├── launch.rs
    │   │   ├── monitor.rs
    │   │   └── shutdown.rs
    │   ├── service.rs     # Service definition
    │   └── mod.rs
    ├── k8s/               # Kubernetes functionality
    │   ├── context.rs     # Kubernetes context
    │   ├── manifests.rs   # Manifest generation
    │   ├── deployment.rs  # Deployment logic
    │   ├── validation.rs  # Manifest validation
    │   └── mod.rs
    ├── security/          # Security related functionality
    │   ├── vault/         # Secret vault implementation
    │   ├── secrets.rs     # Secrets management
    │   └── mod.rs
    ├── utils/             # Utility functions
    │   ├── fs.rs          # File system utilities
    │   ├── git.rs         # Git utilities
    │   ├── path.rs        # Path utilities
    │   ├── template.rs    # Template utilities
    │   └── mod.rs
    ├── error.rs           # Centralized error handling
    ├── lib.rs             # Library exports
    └── main.rs            # Simplified entry point
```

### 3.2. Module-Specific Refactoring

#### 3.2.1. Container Reactor Refactoring

Break down `container_reactor.rs` into smaller modules:

1. **Network Management**:
   - `container::network::create_network`
   - `container::network::delete_network`

2. **Lifecycle Management**:
   - `container::lifecycle::launch::launch_images`
   - `container::lifecycle::monitor::monitor_and_handle_events`
   - `container::lifecycle::shutdown::handle_shutdown`

3. **Build Process**:
   - `container::build::build_and_handle_errors`
   - `container::build::handle_build_error`

4. **File Watching**:
   - `container::watcher::setup_file_watcher`
   - `container::watcher::handle_file_changes`

#### 3.2.2. Main.rs Refactoring

Break down `main.rs` into a cleaner CLI structure:

1. **CLI Commands**:
   - `cli::commands::describe::execute`
   - `cli::commands::dev::execute`
   - `cli::commands::build::execute`
   - `cli::commands::deploy::execute`

2. **Argument Parsing**:
   - `cli::args::parse_args`

3. **Environment Setup**:
   - `core::environment::setup`

#### 3.2.3. Builder Refactoring

Create more focused builder components:

1. **Build Context**:
   - `build::context::create_context`
   - `build::context::validate_context`

2. **Build Script**:
   - `build::script::parse_script`
   - `build::script::execute_script`

## 4. Detailed Implementation Plan

### Phase 1: Setup and Foundation

1. **Create New Directory Structure**:
   - Set up the new folder structure without changing functionality
   - Move existing files to appropriate locations, update imports

2. **Implement Error Handling Framework**:
   - Create centralized error types in `error.rs`
   - Replace string errors with proper error types

3. **Implement Small Utility Modules**:
   - Extract and test pure utility functions
   - Create reusable modules for common functionality

### Phase 2: Core Refactoring

1. **Container Reactor Decomposition**:
   - Break down `container_reactor.rs` into the proposed modules
   - Create interfaces for dependencies
   - Implement dependency injection

2. **CLI Restructuring**:
   - Refactor command parsing and execution in `main.rs`
   - Create individual command modules with consistent interfaces

3. **Configuration Management**:
   - Improve configuration loading and validation
   - Create dedicated configuration types

### Phase 3: Testing and Documentation

1. **Unit Tests**:
   - Create unit tests for newly extracted functions
   - Implement mocks for interfaces to enable isolated testing

2. **Integration Tests**:
   - Update existing integration tests
   - Add new integration tests for key functionality

3. **Documentation**:
   - Add module-level documentation
   - Document public interfaces
   - Create example usage documentation

## 5. Key Architectural Improvements

### 5.1. Dependency Inversion

Replace direct dependencies with interfaces:

```rust
// Before
struct ContainerReactor {
    vault: Arc<Mutex<dyn Vault + Send>>,
    // ... other fields
}

// After
trait VaultProvider {
    fn get_vault(&self) -> Result<Box<dyn Vault>, Error>;
}

struct ContainerReactor<V: VaultProvider> {
    vault_provider: V,
    // ... other fields
}
```

### 5.2. Use of Trait Objects for Testing

```rust
trait DockerClient {
    async fn create_network(&self, name: &str) -> Result<(), Error>;
    async fn delete_network(&self, name: &str) -> Result<(), Error>;
    // Other Docker operations
}

// Production implementation
struct RealDockerClient;
impl DockerClient for RealDockerClient {
    // Implementations
}

// Mock for testing
struct MockDockerClient;
impl DockerClient for MockDockerClient {
    // Test implementations
}
```

### 5.3. Pure Function Extraction

```rust
// Before: Function with side effects and multiple responsibilities
async fn build_and_handle_errors(&self, component: &str) -> Result<(), String> {
    // Complex logic with side effects
}

// After: Pure functions that are easy to test
fn generate_build_plan(component: &str, config: &Config) -> Result<BuildPlan, Error> {
    // Pure logic, no side effects
}

async fn execute_build_plan(
    plan: &BuildPlan, 
    docker_client: &dyn DockerClient
) -> Result<(), Error> {
    // Focused execution
}
```

## 6. Testing Strategy

### 6.1. Unit Testing

Focus on testing pure functions and traits with mock implementations:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_build_plan() {
        let config = Config::default();
        let plan = generate_build_plan("test-component", &config).unwrap();
        assert_eq!(plan.component_name, "test-component");
        // Other assertions
    }
    
    #[tokio::test]
    async fn test_execute_build_plan() {
        let mock_docker = MockDockerClient::new();
        let plan = BuildPlan::default();
        let result = execute_build_plan(&plan, &mock_docker).await;
        assert!(result.is_ok());
        // Verify mock was called correctly
    }
}
```

### 6.2. Integration Testing

Create integration tests that verify interactions between components:

```rust
#[tokio::test]
async fn test_container_lifecycle() {
    // Setup test environment
    let temp_dir = tempfile::tempdir().unwrap();
    // Create test fixtures
    
    // Execute operations
    let result = launch_and_monitor(temp_dir.path()).await;
    
    // Verify results
    assert!(result.is_ok());
    // Other assertions
}
```

## 7. Implementation Timeline

1. **Weeks 1-2**: Setup new directory structure, error handling, and utility modules
2. **Weeks 3-4**: Break down container_reactor.rs and implement tests
3. **Weeks 5-6**: Refactor CLI and main.rs
4. **Weeks 7-8**: Implement core domain models and interfaces
5. **Weeks 9-10**: Complete testing and documentation

## 8. Metrics for Success

1. **Code Metrics**:
   - No file should exceed 300 lines
   - No function should exceed 50 lines
   - Cyclomatic complexity should be reduced
   - Test coverage should exceed 70%

2. **Quality Metrics**:
   - Reduced coupling between modules
   - Improved cohesion within modules
   - Better error handling
   - Consistent naming conventions

## 9. Potential Challenges and Mitigations

1. **Challenge**: Maintaining backward compatibility during refactoring
   **Mitigation**: Create comprehensive integration tests before refactoring and run them after each step

2. **Challenge**: Resistance to changing established patterns
   **Mitigation**: Document benefits of new approach and implement changes incrementally

3. **Challenge**: Increased initial development time
   **Mitigation**: Focus on high-value areas first, demonstrate improved maintainability and testability

4. **Challenge**: Learning curve for new architecture
   **Mitigation**: Provide comprehensive documentation, examples, and conduct knowledge sharing sessions

## 10. Conclusion

This refactoring plan aims to transform the Rush CLI codebase into a more maintainable, testable, and extendable system. By breaking down large, monolithic components into smaller, focused modules with clear responsibilities, the codebase will become easier to understand, test, and evolve. The phased approach allows for incremental improvements while maintaining functionality throughout the process.