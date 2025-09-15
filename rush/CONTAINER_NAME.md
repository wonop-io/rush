# Container Name Formatting Analysis

## Problem Summary

There is an inconsistency in how container names are formatted across the Rush codebase. The error in the logs shows:

```
[rush_container::docker::client_wrapper] get_container_by_name operation failed after 4 attempts:
Some(Docker("Container 'compoundcoders.com_frontend' not found"))
```

The container name uses an underscore (`_`) separator, but the actual containers are created with hyphens (`-`), resulting in names like `compoundcoders.com-frontend`.

## Current State Analysis

### Container Name Generation Locations

After thorough analysis of the codebase, I found the following container name generation patterns:

#### 1. **Hyphen Format** (`{product}-{component}`) - Used in most places:

- **rush-build/src/spec.rs:130** - `ComponentBuildSpec::docker_local_name()`
  ```rust
  format!("{}-{}", self.product_name, self.component_name)
  ```

- **rush-container/src/lifecycle/manager.rs:401** - When creating Docker containers
  ```rust
  name: format!("{}-{}", self.config.product_name, service.name)
  ```

- **rush-container/src/lifecycle/shutdown.rs:294** - When stopping containers
  ```rust
  let container_name = format!("{}-{}", product_name, spec.component_name)
  ```

- **rush-container/src/image_builder.rs:193** - Image naming
  ```rust
  format!("{}-{}", self.product_name, self.component_name)
  ```

- **rush-container/src/image_builder.rs:507** - Service configuration
  ```rust
  image: format!("{}-{}", spec_guard.product_name, spec_guard.component_name)
  ```

- **rush-container/src/build/orchestrator.rs:662, 758, 763, 791** - Various build contexts
  ```rust
  image_name: format!("{}-{}", spec.product_name, spec.component_name)
  docker_host: format!("{}-{}", spec.product_name, component_name)
  ```

#### 2. **Underscore Format** (`{product}_{component}`) - Used in one critical place:

- **rush-container/src/reactor/modular_core.rs:1061** - In `get_deployed_tag()` method
  ```rust
  let container_name = format!("{}_{}", self.config.base.product_name, component_name);
  ```

## Root Cause

The inconsistency occurs because:
1. Containers are **created** with hyphen-separated names (e.g., `compoundcoders.com-frontend`)
2. But when **querying** for existing containers in `get_deployed_tag()`, the code uses underscore separation (e.g., `compoundcoders.com_frontend`)
3. This mismatch causes the container lookup to fail

## Impact

This bug affects:
- Tag-based rebuild detection (Phase 3 of watch functionality)
- The `get_deployed_tag()` method cannot find running containers
- May cause unnecessary rebuilds as the system cannot detect already-deployed containers

## Proposed Solution

### Option 1: Centralized Container Naming (Recommended)

Create a centralized utility for container name generation to ensure consistency:

1. **Add a utility module** in `rush-core` or `rush-utils`:

```rust
// rush-core/src/naming.rs
pub struct NamingConvention;

impl NamingConvention {
    /// Generate a container name from product and component names
    pub fn container_name(product_name: &str, component_name: &str) -> String {
        format!("{}-{}", product_name, component_name)
    }

    /// Generate an image name from product and component names
    pub fn image_name(product_name: &str, component_name: &str) -> String {
        format!("{}-{}", product_name, component_name)
    }
}
```

2. **Update all locations** to use the centralized naming:
   - Replace all direct `format!` calls with `NamingConvention::container_name()`
   - Ensure consistency across the codebase

### Option 2: Quick Fix (Immediate)

Simply fix the inconsistent line in `modular_core.rs:1061`:

```rust
// Change from:
let container_name = format!("{}_{}", self.config.base.product_name, component_name);

// To:
let container_name = format!("{}-{}", self.config.base.product_name, component_name);
```

## Implementation Plan

### Immediate Fix (5 minutes)
1. Apply Option 2 to fix the immediate issue
2. Test that container lookups work correctly

### Long-term Solution (1-2 hours)
1. Implement the centralized naming convention utility
2. Update all 10+ locations that generate container names
3. Add tests to ensure naming consistency
4. Consider adding validation for product/component names (e.g., no special characters)

## Testing Requirements

After implementation:
1. Verify that `get_deployed_tag()` correctly finds running containers
2. Test that rebuilds are skipped when containers with matching tags exist
3. Ensure container start/stop operations work correctly
4. Test with product names containing dots (like `compoundcoders.com`)

## Additional Recommendations

1. **Naming Validation**: Add validation to ensure product and component names don't contain characters that could cause issues in Docker container names
2. **Documentation**: Document the naming convention in the codebase
3. **Tests**: Add unit tests specifically for name generation to prevent future regressions
4. **Consider Docker Naming Rules**: Docker container names must match `[a-zA-Z0-9][a-zA-Z0-9_.-]+`

## Conclusion

The container naming inconsistency is localized to a single location (`modular_core.rs:1061`) where underscore is used instead of hyphen. While a quick fix is trivial, implementing a centralized naming convention would prevent similar issues in the future and make the codebase more maintainable.