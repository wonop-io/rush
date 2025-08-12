# Rush CLI Modular Architecture

## Overview

This document outlines the modular restructuring of the Rush CLI codebase into separate, focused crates under the `crates/` directory. This separation will improve maintainability, enable better testing, reduce compilation times, and allow for clearer dependency management.

## Proposed Crate Structure

### 1. `rush-core` (Foundation)
**Purpose**: Shared types, traits, constants, and utilities used across all other crates.

**Contents**:
- Constants (Docker tags, default values, etc.)
- Error types and Result type aliases
- Common traits and interfaces
- Basic types (Environment, Platform, etc.)
- Shutdown/signal handling mechanisms

**Dependencies**: Minimal - only essential third-party crates

**Current modules to include**:
- `src/constants.rs`
- `src/error.rs`
- `src/shutdown.rs`
- Parts of `src/core/types.rs`

### 2. `rush-config`
**Purpose**: Configuration loading, validation, and management.

**Contents**:
- Config file parsing (YAML, TOML, JSON)
- Config validation
- Product configuration
- Environment configuration
- Template variable resolution

**Dependencies**: 
- `rush-core`
- `serde`, `serde_yaml`, `toml`

**Current modules to include**:
- `src/core/config/`
- `src/core/product/`
- `src/core/environment/`
- `src/core/dotenv.rs`

### 3. `rush-security`
**Purpose**: Security, secrets management, and vault integration.

**Contents**:
- Vault implementations (File, Dotenv, K8s)
- Secret encoders/decoders
- Environment variable security
- Certificate management

**Dependencies**:
- `rush-core`
- `rush-config`
- Crypto libraries

**Current modules to include**:
- `src/security/`

### 4. `rush-build`
**Purpose**: Build system, artifact generation, and build scripts.

**Contents**:
- Build specifications
- Build scripts generation
- Artifact rendering
- Template processing
- Cross-compilation support

**Dependencies**:
- `rush-core`
- `rush-config`
- `rush-toolchain`

**Current modules to include**:
- `src/build/`

### 5. `rush-toolchain`
**Purpose**: Toolchain detection and management.

**Contents**:
- Git operations
- Cargo/Rust toolchain management
- Node.js/npm detection
- Docker detection
- Platform detection

**Dependencies**:
- `rush-core`
- System command utilities

**Current modules to include**:
- `src/toolchain/`

### 6. `rush-container`
**Purpose**: Container orchestration and Docker operations.

**Contents**:
- Docker client implementations
- Container lifecycle management
- Image building and caching
- Container networking
- File watching for hot-reload

**Dependencies**:
- `rush-core`
- `rush-config`
- `rush-build`
- `rush-toolchain`
- `bollard` or Docker CLI

**Current modules to include**:
- `src/container/`

### 7. `rush-k8s`
**Purpose**: Kubernetes deployment and manifest generation.

**Contents**:
- Manifest generation
- Kubernetes context management
- Deployment operations
- Service mesh integration
- K8s validation

**Dependencies**:
- `rush-core`
- `rush-config`
- `rush-container`
- `k8s-openapi`, `kube`

**Current modules to include**:
- `src/k8s/`

### 8. `rush-output`
**Purpose**: Output formatting, logging, and terminal UI.

**Contents**:
- Output directors (Plain, JSON, Interactive)
- Color management
- Progress indicators
- Log aggregation
- Terminal UI components

**Dependencies**:
- `rush-core`
- Terminal UI libraries

**Current modules to include**:
- `src/output/`

### 9. `rush-utils`
**Purpose**: General utilities and helpers.

**Contents**:
- Path utilities
- Directory guards
- File operations
- Process utilities
- Network utilities

**Dependencies**:
- `rush-core`
- Standard library extensions

**Current modules to include**:
- `src/utils/`

### 10. `rush-cli`
**Purpose**: Main CLI application and command orchestration.

**Contents**:
- CLI argument parsing
- Command implementations
- Context building
- Main application entry point

**Dependencies**:
- All other crates
- `clap` for CLI parsing

**Current modules to include**:
- `src/cli/`
- `src/main.rs`
- `src/lib.rs`

## Migration Strategy

### Phase 1: Core Extraction
1. Create `crates/rush-core/`
2. Move constants, errors, and basic types
3. Update all imports in existing code

### Phase 2: Bottom-Up Migration
1. Migrate leaf crates with no internal dependencies:
   - `rush-utils`
   - `rush-output`
   - `rush-toolchain`

2. Migrate middle-layer crates:
   - `rush-config`
   - `rush-security`
   - `rush-build`

3. Migrate high-level crates:
   - `rush-container`
   - `rush-k8s`

4. Finally, restructure `rush-cli` as the orchestration layer

### Phase 3: Optimization
1. Review and optimize inter-crate dependencies
2. Ensure no circular dependencies
3. Consider creating additional feature flags for optional functionality

## Workspace Configuration

Create a workspace `Cargo.toml` in the root:

```toml
[workspace]
members = [
    "crates/rush-core",
    "crates/rush-config",
    "crates/rush-security",
    "crates/rush-build",
    "crates/rush-toolchain",
    "crates/rush-container",
    "crates/rush-k8s",
    "crates/rush-output",
    "crates/rush-utils",
    "crates/rush-cli",
]

[workspace.package]
version = "0.0.21"
authors = ["Rush Contributors"]
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/your-org/rush"

[workspace.dependencies]
# Shared dependencies with unified versions
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.35", features = ["full"] }
anyhow = "1.0"
thiserror = "1.0"
log = "0.4"
# ... other common dependencies
```

## Testing Strategy for 80% Coverage

### Current State
- **Unit tests**: ~70 tests in `src/`
- **Integration tests**: ~13 test files
- **Estimated coverage**: ~40-50%

### Target Coverage by Crate

#### High Priority (90%+ coverage target)
1. **`rush-core`**: 95% coverage
   - All error handling paths
   - Type conversions
   - Utility functions

2. **`rush-config`**: 90% coverage
   - Config parsing edge cases
   - Validation logic
   - Template resolution

3. **`rush-utils`**: 95% coverage
   - Path manipulation
   - Directory operations
   - All utility functions

#### Medium Priority (80%+ coverage target)
4. **`rush-build`**: 85% coverage
   - Build script generation
   - Artifact rendering
   - Template processing

5. **`rush-security`**: 85% coverage
   - Secret encoding/decoding
   - Vault operations
   - Environment variable handling

6. **`rush-toolchain`**: 80% coverage
   - Tool detection
   - Version parsing
   - Git operations

#### Lower Priority (70%+ coverage target)
7. **`rush-container`**: 75% coverage
   - Focus on image building logic
   - Container lifecycle states
   - Mock Docker operations

8. **`rush-k8s`**: 70% coverage
   - Manifest generation
   - Validation logic
   - Mock K8s API calls

9. **`rush-output`**: 70% coverage
   - Output formatting
   - Color handling
   - Progress indicators

10. **`rush-cli`**: 70% coverage
    - Command parsing
    - Integration tests for workflows

### Testing Improvements Needed

#### 1. Unit Test Additions
**Immediate needs** (Add ~150 unit tests):
- [ ] Error handling paths in all modules
- [ ] Edge cases in config parsing
- [ ] Build script generation for all build types
- [ ] Secret encoding/decoding roundtrips
- [ ] Path utilities with special characters
- [ ] Container state transitions
- [ ] Git operations with various states
- [ ] Manifest generation for different specs

#### 2. Integration Test Additions
**New integration tests needed** (~20 tests):
- [ ] End-to-end build workflows
- [ ] Container launch and teardown
- [ ] File watching and rebuild triggers
- [ ] Multi-component dependency resolution
- [ ] Cross-compilation scenarios
- [ ] Vault integration scenarios
- [ ] K8s deployment workflows

#### 3. Property-Based Testing
Consider using `proptest` for:
- Config parsing with random inputs
- Path manipulation edge cases
- Template rendering with various inputs
- Secret encoding/decoding

#### 4. Mocking Strategy
Implement mocks for:
- Docker operations (`mockall` or custom traits)
- File system operations (use `tempfile` for tests)
- Network operations
- External command execution
- Time-based operations

#### 5. Test Infrastructure
Create test utilities:
- Test fixture generators
- Common test configurations
- Docker container test helpers
- Mock vault implementations
- Test data builders

### Coverage Measurement

1. **Setup Coverage Tools**:
```bash
# Install tarpaulin for coverage
cargo install cargo-tarpaulin

# Run with coverage
cargo tarpaulin --out Html --output-dir coverage
```

2. **CI Integration**:
- Add coverage reporting to CI pipeline
- Set minimum coverage thresholds per crate
- Block PRs that reduce coverage
- Generate coverage badges

3. **Coverage Goals Timeline**:
- **Month 1**: Achieve 60% overall coverage
- **Month 2**: Achieve 70% overall coverage
- **Month 3**: Achieve 80% overall coverage
- **Ongoing**: Maintain 80%+ for new code

### Testing Best Practices

1. **Test Organization**:
   - Unit tests in `src/` next to code
   - Integration tests in `tests/` per crate
   - Common test utilities in `test-utils` crate

2. **Test Naming**:
   - Descriptive test names: `test_<module>_<scenario>_<expected>`
   - Group related tests in modules
   - Use `#[should_panic]` for error cases

3. **Test Data**:
   - Use builders for complex test objects
   - Keep test data minimal and focused
   - Use fixtures for large test data

4. **Async Testing**:
   - Use `tokio::test` for async tests
   - Test timeout handling
   - Test concurrent operations

5. **Documentation**:
   - Document why tests exist
   - Document complex test setups
   - Include examples in doc comments

## Benefits of Modularization

1. **Faster Compilation**: Changes to one crate don't require recompiling all code
2. **Better Testing**: Each crate can be tested in isolation
3. **Clearer Dependencies**: Explicit dependencies between modules
4. **Reusability**: Crates can be used independently in other projects
5. **Parallel Development**: Teams can work on different crates simultaneously
6. **Easier Onboarding**: New developers can understand focused crates more easily
7. **Better Documentation**: Each crate has its own focused documentation
8. **Semantic Versioning**: Crates can evolve independently with proper versioning

## Implementation Timeline

- **Week 1-2**: Setup workspace, create `rush-core`, migrate basic types
- **Week 3-4**: Migrate utility crates (`rush-utils`, `rush-output`, `rush-toolchain`)
- **Week 5-6**: Migrate config and security crates
- **Week 7-8**: Migrate build and container crates
- **Week 9-10**: Migrate K8s and finalize CLI crate
- **Week 11-12**: Testing improvements and documentation
- **Ongoing**: Increase test coverage to 80%

## Success Metrics

1. **Build Performance**: 50% reduction in incremental build times
2. **Test Coverage**: Achieve and maintain 80% code coverage
3. **Module Coupling**: No circular dependencies, clear hierarchy
4. **Documentation**: 100% public API documentation
5. **CI Performance**: 30% faster CI pipeline
6. **Developer Velocity**: Easier parallel development on features