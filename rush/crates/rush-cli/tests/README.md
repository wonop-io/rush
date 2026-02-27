# Testing Strategy for Rush CLI

This directory contains unit and integration tests for the Rush CLI tool. The tests are designed to verify the functionality of the core components of the Rush system.

## Test Organization

Tests are organized as follows:

1. **Unit Tests** - Focused on testing individual components in isolation
2. **Integration Tests** - Testing how components work together
3. **Test Helpers** - Common utilities for test setup

## Key Test Files

- `minimal_test.rs` - Basic functionality tests for the Config system
- `docker_test.rs` - Tests for the Docker image handling functionality

## Test Coverage Goals

The test suite aims to cover the following critical areas:

1. **Docker Image Management** - Testing image creation, tagging, dependencies
2. **Build System** - Testing build context generation and build type handling
3. **Configuration Management** - Testing config loading and validation
4. **Kubernetes Integration** - Testing manifest generation and deployment

## Running Tests

Run all tests with:
```
cargo test
```

Run a specific test file with:
```
cargo test --test <test_file_name>
```

Run a specific test with:
```
cargo test <test_name>
```

## Adding New Tests

When adding new tests:

1. Start with unit tests for isolated functionality
2. Use test helpers to create common test objects
3. Use proper mocking for external dependencies
4. Focus on testing core business logic rather than implementation details

## Testing Priorities

1. **First Priority**: Test the core Docker image functionality (the most critical component)
2. **Second Priority**: Test the container reactor functionality for managing containers
3. **Third Priority**: Test build and deployment pipelines