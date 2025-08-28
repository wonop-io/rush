# Docker Build Context Specification and Analysis

## Executive Summary

This document analyzes how Rush calculates Docker build context paths and working directories for container builds. As of the latest changes, **the implementation has been updated** to make Docker build context always relative to the component directory, providing consistent and intuitive behavior.

## Current Implementation Status

**Status: ✅ UPDATED** - The context path calculation now consistently uses component-relative paths for all build types.

## How Context Path Calculation Works

### 1. Configuration Structure

Components define their build context in `stack.spec.yaml` using these fields:
- `location`: Relative path from product root to component directory  
- `context_dir`: Optional path relative to the component's location (NOT product root)
- `dockerfile`: Path to Dockerfile relative to product root

**Example from `io.wonop.helloworld`:**
```yaml
frontend:
  build_type: "TrunkWasm"
  location: "frontend/webui"          # Component at products/io.wonop.helloworld/frontend/webui/
  context_dir: ".."                   # Context = products/io.wonop.helloworld/frontend/webui/../ = frontend/
  dockerfile: "frontend/Dockerfile"   # Dockerfile at products/io.wonop.helloworld/frontend/Dockerfile

backend:
  build_type: "RustBinary" 
  location: "backend/server"          # Component at products/io.wonop.helloworld/backend/server/
  # context_dir omitted                 Context = products/io.wonop.helloworld/ (default)
  dockerfile: "backend/Dockerfile"    # Dockerfile at products/io.wonop.helloworld/backend/Dockerfile
```

### 2. Path Resolution Logic

The context path calculation is implemented in `/rush/crates/rush-container/src/build/orchestrator.rs:279-340`:

```rust
let docker_context = match &spec.build_type {
    BuildType::TrunkWasm { context_dir, location, .. } => {
        let component_base = self.config.product_dir.join(location);
        if let Some(ctx) = context_dir {
            // context_dir is relative to the component's location
            component_base.join(ctx)
        } else {
            // Default to the component's directory
            component_base
        }
    }
    // ... similar for other build types
}
```

**Key Points:**
1. **When `context_dir` is specified:** `context_dir` is resolved relative to the component's location directory
2. **When `context_dir` is omitted:** **NEW BEHAVIOR** - Defaults to the component's location directory (not product root)
3. **Special case for Ingress:** Since Ingress has no `location`, `context_dir` is relative to product root

### 3. Working Directory Resolution

For artifact preparation (copying source files before Docker build), the working directory is calculated in `/rush/crates/rush-container/src/build/orchestrator.rs:500+`:

```rust
let source_dir = if let BuildType::TrunkWasm { context_dir, location, .. } = &spec.build_type {
    let component_base = self.config.product_dir.join(location);
    if let Some(ctx) = context_dir {
        // context_dir is relative to the component's location
        component_base.join(ctx)
    } else {
        // Default to component location
        component_base
    }
} else {
    component_base
};
```

**Behavior Consistency:**
- **Docker context:** Now defaults to component location when `context_dir` is omitted (UPDATED)
- **Source directory:** Defaults to component location when `context_dir` is omitted (same as before)

Both Docker context and source directory now have consistent default behavior, making the system more predictable and intuitive.

## Concrete Path Examples

Given the `io.wonop.helloworld` product structure:

```
products/io.wonop.helloworld/
├── frontend/
│   ├── Dockerfile                    # Expects webui/dist/ and nginx.conf in context
│   ├── nginx.conf
│   └── webui/                        # Component location
│       ├── Cargo.toml
│       ├── src/
│       └── dist/                     # Build output
└── backend/
    ├── Dockerfile                    # Expects ./backend/server/target/.../server binary
    └── server/                       # Component location
        ├── Cargo.toml
        ├── src/
        └── target/
```

### Frontend Component Context Resolution:
- **Component location:** `products/io.wonop.helloworld/frontend/webui/`
- **context_dir:** `".."`  
- **Resolved Docker context:** `products/io.wonop.helloworld/frontend/webui/../` = `products/io.wonop.helloworld/frontend/`
- **Source directory:** `products/io.wonop.helloworld/frontend/webui/../` = `products/io.wonop.helloworld/frontend/`

This is **correct** because:
- Dockerfile is at `frontend/Dockerfile`
- Dockerfile expects `./webui/dist` and `./nginx.conf` to be available
- Both paths exist in the `frontend/` directory when used as Docker build context

### Backend Component Context Resolution (UPDATED):
- **Component location:** `products/io.wonop.helloworld/backend/server/`
- **context_dir:** `"."` (explicitly set)
- **Resolved Docker context:** `products/io.wonop.helloworld/backend/server/` (component location)
- **Source directory:** `products/io.wonop.helloworld/backend/server/`

This is **correct** because:
- Dockerfile has been updated to expect `./target/x86_64-unknown-linux-gnu/release/server` (relative to component)
- Docker context is now consistent with source directory behavior
- The `context_dir: "."` in the stack spec makes the new behavior explicit

## Docker Interface

The final Docker build command uses the resolved context:

```rust
self.docker_client.build_image(
    &full_image_name,
    &dockerfile_path.to_string_lossy(),
    &docker_context.to_string_lossy(),    // Resolved context path
).await?;
```

This translates to a Docker command like:
```bash
docker build -t image_name -f /path/to/dockerfile /path/to/context
```

## Recent Changes and Improvements

The implementation has been updated to address inconsistencies in context path calculation:

### ✅ Updated: Consistent Default Behavior
- **Previous:** Docker context defaulted to product root when `context_dir` was omitted
- **Current:** Docker context now defaults to component location (same as source directory)
- **Implementation:** Changed `self.config.product_dir.clone()` to `component_base` in the `else` branches

### ✅ Updated: Path Handling Consistency  
- **Docker context:** Now consistently uses component-relative resolution for both explicit and default contexts
- **Source directory:** Behavior unchanged - already used component-relative resolution
- **Result:** Both systems now work identically

### ✅ Updated: Configuration Requirements
- **Breaking Change:** Components that relied on the old product-root default may need `context_dir` explicitly set
- **Solution:** Add `context_dir: ".."` to move up from component to parent directory if needed
- **Example:** Backend component updated with `context_dir: "."` and Dockerfile paths adjusted

## Validation Through Real Examples

The current implementation correctly handles the real-world example:

**Frontend component with `context_dir: ".."`:**
- Input: `location: "frontend/webui"`, `context_dir: ".."`
- Expected result: Context should be `frontend/` directory
- Actual result: `product_dir.join("frontend/webui").join("..")` = `frontend/` ✅

**Backend component with explicit `context_dir: "."`:**
- Input: `location: "backend/server"`, `context_dir: "."`  
- Expected result: Context should be `backend/server/` directory (component location)
- Actual result: `product_dir.join("backend/server").join(".")` = `backend/server/` ✅
- **Note:** Dockerfile updated to expect `./target/...` instead of `./backend/server/target/...`

## Architecture Consistency

The context path calculation is consistent across the codebase:

1. **Build orchestration** (`orchestrator.rs:279-340`): Docker context calculation
2. **Artifact preparation** (`orchestrator.rs:500+`): Source directory calculation  
3. **Docker client interface** (`traits.rs`): Standard `build_image(tag, dockerfile, context)` signature

## Recommendations

### ✅ Implementation Status: Complete
The context path calculation is correctly implemented and handles all identified use cases.

### 📋 Documentation Recommendations
1. **Add inline documentation** explaining the context_dir resolution rules
2. **Update user documentation** to clarify that context_dir is relative to component location
3. **Add validation warnings** for context_dir paths that escape component boundaries (e.g., `../../..`)

### 🧪 Testing Recommendations
1. **Add integration tests** for context path resolution with various `context_dir` configurations
2. **Test edge cases** like nested directory structures and symlinks
3. **Validate Docker build behavior** with different context configurations

## Migration Guide

### For Existing Projects

If you have existing Rush projects, you may need to update them after this change:

1. **Check your Dockerfiles:** If they expect the build context to be the product root, you have two options:
   - **Option A (Recommended):** Update the Dockerfile paths to be relative to the component directory
   - **Option B:** Add explicit `context_dir: ".."` (or appropriate relative path) to maintain old behavior

2. **Example Migration:**
   ```yaml
   # Before (relied on product root default)
   backend:
     location: "backend/server"
     dockerfile: "backend/Dockerfile"  # Dockerfile expected ./backend/server/target/...
   
   # After (explicitly use component-relative context)  
   backend:
     location: "backend/server"
     context_dir: "."                 # Dockerfile now expects ./target/...
     dockerfile: "backend/Dockerfile" # Dockerfile updated
   ```

## Conclusion

The Docker build context calculation in Rush now provides **consistent, predictable behavior** across all build types. The updated system offers:

- **Intuitive behavior:** Both `context_dir` and default context are relative to component location
- **Consistency:** Docker context and source directory use identical logic
- **Flexibility:** Explicit `context_dir` allows customization when needed
- **Type safety:** Different build types handled appropriately

This change makes Rush's behavior more predictable and eliminates the previous inconsistency between Docker context defaults and source directory defaults. While it's a breaking change, it simplifies the mental model and provides better long-term maintainability.