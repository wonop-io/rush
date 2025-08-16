# Add Integration Tests for Container Management

## Overview
Add comprehensive integration tests to validate the container management functionality, particularly focusing on the recent fixes for platform architecture, graceful shutdown, container crash detection, and log capture.

## Test Areas

### 1. Platform Architecture Tests
- **Test cross-platform image building**: Verify that images are always built for `linux/amd64` regardless of host architecture
- **Test image cache validation**: Ensure `check_image_exists()` correctly validates architecture
- **Test platform mismatch detection**: Verify that ARM64 images are rejected on systems expecting AMD64

### 2. Graceful Shutdown Tests
- **Test clean shutdown on user request**: Verify Ctrl+C triggers graceful shutdown without error messages
- **Test shutdown cancellation token propagation**: Ensure all async operations respect the cancellation token
- **Test process termination handling**: Verify exit codes 255, 1, 125 are handled gracefully during shutdown
- **Test resource cleanup**: Ensure all containers and processes are properly cleaned up

### 3. Container Crash Detection Tests
- **Test single container failure**: Verify that when one container crashes, all containers are shut down
- **Test shutdown reason propagation**: Ensure `ContainerExit` shutdown reason is properly set
- **Test container monitoring**: Verify continuous monitoring detects container exits promptly
- **Test cleanup after crash**: Ensure proper cleanup of all resources after container crash

### 4. Build Failure Handling Tests
- **Test build failure recovery**: Verify system waits for file changes after build failure
- **Test infinite wait on failure**: Ensure no automatic retry loops occur
- **Test file change detection after failure**: Verify rebuild triggers correctly after file modification
- **Test timeout behavior**: Confirm 1-hour timeout doesn't trigger premature rebuilds

### 5. Log Capture Tests
- **Test startup log capture**: Verify all logs from container start are captured
- **Test log streaming**: Ensure continuous log streaming works correctly
- **Test multi-container logging**: Verify logs from multiple containers are properly separated
- **Test log format preservation**: Ensure colors and formatting are preserved in split output mode

## Implementation Approach

### Unit Tests
Location: `rush/crates/rush-container/src/tests/`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_image_architecture_validation() {
        // Test that check_image_exists validates architecture
    }

    #[tokio::test]
    #[serial]
    async fn test_graceful_shutdown_handling() {
        // Test shutdown without errors
    }

    #[tokio::test]
    #[serial]
    async fn test_container_crash_detection() {
        // Test that container exit triggers full shutdown
    }
}
```

### Integration Tests
Location: `rush/crates/rush-cli/tests/`

```rust
#[tokio::test]
#[serial]
async fn test_full_container_lifecycle() {
    // Test complete flow: build -> run -> monitor -> shutdown
}

#[tokio::test]
#[serial]
async fn test_build_failure_recovery() {
    // Test that build failures are handled correctly
}

#[tokio::test]
#[serial]
async fn test_log_capture_completeness() {
    // Test that no startup logs are missed
}
```

## Test Utilities Needed

### Mock Docker Client
- Create a mock implementation of `DockerClient` trait for testing
- Support simulating various Docker command responses
- Allow injection of failures for error testing

### Test Containers
- Create simple test Docker images with predictable output
- Include containers that exit immediately for crash testing
- Include containers with verbose startup logs for capture testing

### Test Helpers
```rust
// Helper to create test reactor with mock dependencies
async fn create_test_reactor() -> ContainerReactor { ... }

// Helper to simulate file changes
async fn trigger_file_change(path: &str) { ... }

// Helper to verify shutdown behavior
async fn assert_graceful_shutdown(reactor: &ContainerReactor) { ... }
```

## Coverage Goals
- Minimum 80% code coverage for critical paths
- 100% coverage for error handling paths
- Integration tests for all user-facing commands

## Testing Strategy

### Phase 1: Unit Tests
- Test individual components in isolation
- Focus on business logic and error handling
- Use mocks for external dependencies

### Phase 2: Integration Tests
- Test component interactions
- Use real Docker where possible, mocks where necessary
- Test full command execution paths

### Phase 3: End-to-End Tests
- Test complete workflows with real containers
- Verify output formatting and user experience
- Test cross-platform compatibility

## Success Criteria
- All tests pass consistently on CI
- No flaky tests
- Tests run in under 2 minutes
- Clear test names that document behavior
- Tests serve as usage examples

## Dependencies
- `serial_test` - Prevent test conflicts with Docker
- `tempfile` - Create temporary test directories
- `mockall` - Generate mock implementations
- `assert_cmd` - Test CLI command execution
- `predicates` - Assert complex conditions

## Priority Order
1. Container crash detection tests (critical bug fix)
2. Log capture tests (user-visible issue)
3. Graceful shutdown tests (user experience)
4. Platform architecture tests (compatibility)
5. Build failure handling tests (developer experience)