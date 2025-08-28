# CONTEXT_DIR Issue Analysis Report

## Executive Summary
The `context_dir` field in Rush components is currently being incorrectly calculated relative to the product directory instead of the component directory in certain parts of the codebase. This causes issues when building Docker images as the build context is not properly scoped to the component.

## Current Implementation Analysis

### 1. Where context_dir is Defined
The `context_dir` field is defined in the `BuildType` enum variants in `/rush/crates/rush-build/src/build_type.rs`:
- `TrunkWasm { context_dir: Option<String>, ... }`
- `DixiousWasm { context_dir: Option<String>, ... }`
- `RustBinary { context_dir: Option<String>, ... }`
- `Book { context_dir: Option<String>, ... }`
- `Zola { context_dir: Option<String>, ... }`
- `Script { context_dir: Option<String>, ... }`
- `Ingress { context_dir: Option<String>, ... }`

### 2. Where context_dir is Set
In `/rush/crates/rush-build/src/spec.rs:216-220` and similar locations:
```rust
BuildType::RustBinary {
    context_dir: Some(
        yaml_section
            .get("context_dir")
            .map_or(".".to_string(), |v| v.as_str().unwrap().to_string()),
    ),
    ...
}
```

The issue: When `context_dir` is not specified in YAML, it defaults to `"."` which means "current directory". However, this is interpreted differently depending on where it's used.

### 3. Where context_dir is Used

#### CORRECT Usage (relative to component):
In `/rush/crates/rush-container/src/build/orchestrator.rs:388-396`:
```rust
let source_dir = if let BuildType::RustBinary { context_dir, .. } = &spec.build_type {
    if let Some(ctx) = context_dir {
        self.config.product_dir.join(ctx)  // BUG: This assumes ctx is relative to product_dir
    } else {
        self.config.product_dir.join(&spec.component_name)  // This is correct fallback
    }
} else {
    self.config.product_dir.join(&spec.component_name)
};
```

#### PROBLEMATIC Usage (relative to product):
In `/rush/crates/rush-container/src/build/orchestrator.rs:276-297`:
```rust
let docker_context = match &spec.build_type {
    BuildType::TrunkWasm { context_dir, .. }
    | BuildType::DixiousWasm { context_dir, .. }
    | BuildType::RustBinary { context_dir, .. }
    | BuildType::Book { context_dir, .. }
    | BuildType::Zola { context_dir, .. }
    | BuildType::Script { context_dir, .. }
    | BuildType::Ingress { context_dir, .. } => {
        if let Some(ctx) = context_dir {
            // Explicit context directory specified
            self.config.product_dir.join(ctx)  // ISSUE: Always relative to product_dir
        } else {
            // Use the directory containing the Dockerfile as context
            dockerfile_path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| self.config.product_dir.clone())
        }
    }
    _ => self.config.product_dir.clone(),
};
```

## Root Cause
The fundamental issue is that `context_dir` is being treated as a path relative to the product directory, when it should be relative to the component directory. The confusion arises because:

1. **Implicit assumption**: The code assumes `context_dir` is always relative to `product_dir`
2. **Missing component path resolution**: The component's location is not taken into account when resolving `context_dir`
3. **Inconsistent defaults**: When `context_dir` is not specified, different parts of the code make different assumptions

## Recommended Solution

### Option 1: Make context_dir Relative to Component Directory (Recommended)
This maintains backward compatibility while fixing the semantic issue.

**Changes needed in `/rush/crates/rush-container/src/build/orchestrator.rs:276-297`:**

```rust
let docker_context = match &spec.build_type {
    BuildType::TrunkWasm { context_dir, location, .. }
    | BuildType::DixiousWasm { context_dir, location, .. }
    | BuildType::RustBinary { context_dir, location, .. }
    | BuildType::Book { context_dir, location, .. }
    | BuildType::Zola { context_dir, location, .. }
    | BuildType::Script { context_dir, location, .. } => {
        // First determine the component's base directory
        let component_base = self.config.product_dir.join(location);
        
        if let Some(ctx) = context_dir {
            // context_dir is relative to the component's location
            component_base.join(ctx)
        } else {
            // Default to the component's directory itself
            component_base
        }
    }
    BuildType::Ingress { context_dir, .. } => {
        // Ingress doesn't have a location, so context_dir is relative to product
        if let Some(ctx) = context_dir {
            self.config.product_dir.join(ctx)
        } else {
            // Use dockerfile parent as fallback
            dockerfile_path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| self.config.product_dir.clone())
        }
    }
    _ => self.config.product_dir.clone(),
};
```

### Option 2: Explicit Path Resolution in spec.rs
Resolve the context_dir path when creating the ComponentBuildSpec:

**Changes needed in `/rush/crates/rush-build/src/spec.rs`:**

Add context resolution during parsing:
```rust
// When parsing context_dir, make it absolute or properly relative
let resolved_context_dir = yaml_section
    .get("context_dir")
    .map(|v| {
        let ctx = v.as_str().unwrap();
        if ctx == "." {
            // "." means relative to component location
            location.clone()  // Use the component's location
        } else if ctx.starts_with("/") {
            // Absolute path
            ctx.to_string()
        } else {
            // Relative to component location
            format!("{}/{}", location, ctx)
        }
    });
```

### Option 3: Add a `component_relative_context_dir` Field
Introduce a new field that explicitly stores the context directory relative to the component:

```rust
pub struct ComponentBuildSpec {
    // ... existing fields ...
    
    /// Context directory relative to component location
    pub component_relative_context_dir: Option<String>,
    
    /// Absolute context directory path (computed)
    pub absolute_context_dir: PathBuf,
}
```

## Impact Assessment

### Files that need modification:
1. `/rush/crates/rush-container/src/build/orchestrator.rs` - Primary fix location
2. `/rush/crates/rush-build/src/spec.rs` - Optional: path resolution during parsing
3. `/rush/crates/rush-container/src/build/orchestrator.rs:388-396` - Ensure consistency
4. `/rush/crates/rush-cli/src/commands/build.rs` - Verify context_dir usage

### Testing Required:
1. Test builds with explicit `context_dir` in YAML
2. Test builds without `context_dir` (using defaults)
3. Test builds with nested component structures
4. Test Ingress builds (special case)
5. Test cross-compilation scenarios

## Recommended Immediate Fix

The most straightforward fix with minimal disruption is **Option 1**. It:
- Maintains backward compatibility for most cases
- Makes the behavior intuitive (context_dir relative to component)
- Requires changes in only one location
- Special-cases Ingress which doesn't have a component location

## Example YAML Impact

### Before (Incorrect behavior):
```yaml
backend:
  build_type: "RustBinary"
  location: "backend/server"  # Component at products/myapp/backend/server
  context_dir: "."            # Currently resolves to products/myapp/
  dockerfile: "backend/server/Dockerfile"
```

### After (Correct behavior):
```yaml
backend:
  build_type: "RustBinary"
  location: "backend/server"  # Component at products/myapp/backend/server
  context_dir: "."            # Will resolve to products/myapp/backend/server/
  dockerfile: "backend/server/Dockerfile"
```

## Migration Path

1. **Phase 1**: Implement the fix in orchestrator.rs
2. **Phase 2**: Add deprecation warnings if context_dir contains "../" patterns
3. **Phase 3**: Update documentation to clarify context_dir behavior
4. **Phase 4**: Add validation to ensure context_dir doesn't escape component boundaries

## Conclusion

The context_dir issue stems from an inconsistent interpretation of relative paths across the codebase. The recommended fix ensures that context_dir is always relative to the component's location, which aligns with user expectations and Docker best practices where the build context should be scoped to the component being built.