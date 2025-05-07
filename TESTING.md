# Testing Rush CLI

This document provides instructions for running and maintaining tests for Rush CLI.

## Quick Start

We've set up a Makefile to simplify testing. The following commands are available:

```sh
# Run only the simple tests (recommended)
make test-simple

# Run all tests (may fail due to unresolved issues)
make test

# Fix code warnings automatically
make fix-warnings

# Clean and rebuild the tests
make rebuild-tests

# For more options
make help
```

## Test Structure

The test structure is organized as follows:

- `tests/simple_test.rs` - Basic tests that verify the crate loads and project structure
- `tests/unit/` - Unit tests for individual components
- `tests/integration/` - Integration tests for components working together
- `tests/test_utils/` - Shared testing utilities

## Known Issues

Currently, there are issues with the full test suite that need to be addressed:

1. Some dependency and import issues in integration tests
2. Path references in the test verification code 
3. Circular dependencies in some test modules

If you're debugging issues, the `simple_test.rs` shows how to correctly reference the library in tests.

## Adding New Tests

When adding new tests:

1. Follow the pattern in `simple_test.rs` for importing the library
2. Use `extern crate rush_cli;` in your test files
3. Run `make test-simple` to verify your changes work with the simple test infrastructure 

## Fixing Code Warnings

The codebase has numerous style and lint warnings. To automatically fix many of them:

```sh
make fix-warnings
```

This will run `cargo fix` on both the library and binary targets.