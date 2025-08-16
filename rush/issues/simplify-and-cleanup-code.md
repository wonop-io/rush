# Simplify and Clean Up Codebase

## Overview
Remove unnecessary code, consolidate redundant functionality, and simplify the architecture following the recent fixes. Focus on improving maintainability and reducing complexity.

## Areas for Cleanup

### 1. Remove Deprecated Functions

#### `simple_output.rs`
- **Remove `attach_to_container()`**: Now just redirects to `follow_container_logs_from_start()`, should be removed entirely
- **Remove `follow_container_logs_simple()`**: Legacy function that redirects to attach_to_container
- **Consolidate output capture functions**: Merge similar functionality between `capture_process_output()` and `capture_process_output_with_shutdown()`

#### `reactor.rs`
- **Remove unused color functions**: `get_color_for_component()` is never used
- **Remove commented-out code**: Clean up any old implementations left as comments
- **Simplify container monitoring**: Consolidate duplicate monitoring logic

### 2. Eliminate Dead Code

#### Identified by Compiler Warnings
```rust
// rush-container/src/image_builder.rs
- Remove unused import: DockerImage
- Remove unused methods: check_specific_image_exists(), retag_image(), get_context_dir()

// rush-container/src/build/processor.rs
- Remove unused field: verbose
- Remove unused methods: get_build_script(), build_docker_image()

// rush-container/src/lifecycle/
- Remove unused fields in LifecycleManager, LifecycleMonitor
- Remove unused shutdown_task(), perform_shutdown() methods
```

#### Other Dead Code
- Remove unused structs in `rush-security` (SecretStore, etc.)
- Clean up unused fields in various vault implementations
- Remove unused imports throughout the codebase

### 3. Consolidate Duplicate Logic

#### Docker Command Execution
- Create a single, unified Docker command executor
- Consolidate platform-specific logic in one place
- Standardize error handling for Docker operations

#### Output Handling
- Merge the two capture functions into one with optional shutdown handling
- Simplify the sink system if possible
- Remove the distinction between build and runtime output where unnecessary

#### Container Lifecycle
- Consolidate container state management
- Merge similar lifecycle operations
- Simplify the state machine logic

### 4. Simplify Architecture

#### ContainerReactor Simplification
- **Reduce state complexity**: Minimize the number of state variables
- **Simplify the main loop**: Make the reactor loop more readable
- **Extract complex logic**: Move large code blocks to dedicated methods

#### Remove Unnecessary Abstractions
- **Evaluate trait usage**: Remove traits with single implementations
- **Simplify builder patterns**: Use direct construction where builders add no value
- **Reduce Arc<Mutex<>> usage**: Only use where truly needed for shared state

#### Consolidate Error Types
- Reduce the number of error variants
- Standardize error messages
- Improve error context propagation

### 5. Code Organization

#### Module Structure
```
rush-container/
├── src/
│   ├── lib.rs           # Public API only
│   ├── docker.rs        # All Docker interactions
│   ├── reactor.rs       # Simplified reactor
│   ├── output.rs        # Consolidated output handling
│   ├── monitor.rs       # Container monitoring
│   └── builder.rs       # Image building
```

#### Remove Unnecessary Modules
- Consolidate lifecycle modules into reactor
- Merge small modules with single functions
- Remove the separation between simple and complex output

### 6. Specific Refactoring Tasks

#### Task 1: Unify Docker Operations
```rust
// Before: Multiple places calling docker
Command::new("docker").args(["logs", "--follow"])...
Command::new("docker").args(["run", "-d"])...

// After: Single Docker client
docker_client.logs(container_id, follow=true).await?
docker_client.run(config).await?
```

#### Task 2: Simplify Output Capture
```rust
// Before: Two similar functions
capture_process_output(...)
capture_process_output_with_shutdown(...)

// After: Single function with options
capture_output(CaptureOptions {
    command,
    args,
    component_name,
    sink,
    is_build,
    respect_shutdown: true,
})
```

#### Task 3: Clean Up Container State
```rust
// Before: Complex state tracking
if container.state == Running && !container.stopping && ...

// After: Clear state machine
match container.state {
    State::Running => ...,
    State::Stopping => ...,
    State::Stopped => ...,
}
```

### 7. Performance Improvements

#### Reduce Allocations
- Use `&str` instead of `String` where possible
- Avoid unnecessary clones
- Use `Cow<str>` for conditional ownership

#### Optimize Async Operations
- Reduce unnecessary awaits
- Batch Docker operations where possible
- Use tokio::select! more efficiently

### 8. Documentation Cleanup

#### Remove Outdated Comments
- Delete TODO comments that are done
- Remove commented-out code blocks
- Update incorrect documentation

#### Add Missing Documentation
- Document public APIs
- Add examples for complex functions
- Explain the "why" not just the "what"

## Implementation Plan

### Phase 1: Remove Dead Code (Quick Wins)
1. Run `cargo fix` to remove unused imports
2. Delete methods/functions marked as never used
3. Remove deprecated functions that just redirect

### Phase 2: Consolidate Duplicate Logic
1. Unify Docker command execution
2. Merge output capture functions
3. Consolidate error handling

### Phase 3: Architectural Simplification
1. Simplify ContainerReactor state machine
2. Reduce unnecessary abstractions
3. Reorganize module structure

### Phase 4: Testing and Validation
1. Ensure all tests still pass
2. Add tests for refactored components
3. Verify no functionality is lost

## Success Metrics
- **Code reduction**: At least 20% fewer lines of code
- **Complexity reduction**: Lower cyclomatic complexity scores
- **Warning elimination**: Zero compiler warnings
- **Build time**: Faster compilation due to less code
- **Maintainability**: Easier to understand and modify

## Risks and Mitigation
- **Risk**: Breaking existing functionality
  - **Mitigation**: Comprehensive test suite before refactoring
- **Risk**: Removing code that seems unused but isn't
  - **Mitigation**: Careful analysis of call graphs, grep for usage
- **Risk**: Over-simplification losing flexibility
  - **Mitigation**: Keep extension points for future features

## Priority Order
1. Remove dead code (low risk, immediate benefit)
2. Fix compiler warnings (improves code quality)
3. Consolidate duplicate Docker operations (reduces bugs)
4. Simplify output handling (improves maintainability)
5. Refactor reactor architecture (highest risk, biggest benefit)