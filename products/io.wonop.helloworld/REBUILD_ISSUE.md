# Rush Automatic Rebuild Issue Analysis

## Problem Summary

Components are not rebuilding automatically when files change. The logs show:
```
13:37:26 [SYSTEM] File changes detected, triggering rebuild for 0 components
13:37:26 [SYSTEM] [CACHE] Invalidating 1 components: ["frontend"]
13:37:26 [SYSTEM] No component specs found for rebuild, skipping build
```

The file watcher detects changes and the cache correctly identifies affected components, but the rebuild is skipped because `batch.affected_components` is empty.

## Root Cause

The issue is in the `identify_affected_components` function in `watcher/handler.rs`. When it checks if a component is affected:

1. **Components WITHOUT watch patterns**: Falls back to location-based checking
2. **Components WITH watch patterns**: Only checks pattern matching

The problem is that most components don't have watch patterns defined (only `ingress` has them in stack.spec.yaml), so they rely on location-based checking. However, the location-based checking might fail because:

### Issue 1: Path Resolution Mismatch
The location-based checking (lines 347-383 in handler.rs) compares absolute paths:
```rust
let abs_location = if Path::new(loc).is_absolute() {
    PathBuf::from(loc)
} else {
    self.base_dir.join(loc)  // base_dir + location
};
```

But the actual file paths might not match due to:
- The base_dir being set to the product directory
- The location being a relative path like "frontend/webui"
- The actual changed file being at a different path structure

### Issue 2: Component Specs Not Matching Runtime State

The handler's `component_specs` are set during initialization (line 202 in modular_core.rs):
```rust
coordinator.init(component_specs.clone()).await;
```

But when `handle_rebuild` tries to get specs from state (lines 416-422):
```rust
let component_specs = {
    let state = self.state.read().await;
    batch.affected_components.iter()
        .filter_map(|name| state.get_component(name))
        .filter_map(|comp| comp.build_spec.as_ref().cloned())
        .collect::<Vec<_>>()
};
```

Even if `affected_components` were populated correctly, the state might not have the component specs properly stored.

## Detailed Flow Analysis

1. **File change detected** → notify event sent to handler
2. **Handler processes event** → adds to pending_changes
3. **process_pending()** called → `identify_affected_components()` runs
4. **identify_affected_components()** checks each component:
   - For `frontend`: No watch patterns → uses location-based check
   - Location check fails due to path mismatch
   - Component NOT added to affected_components
5. **ChangeBatch sent** with empty affected_components
6. **handle_rebuild()** receives batch with 0 components
7. **Build skipped**

Meanwhile, the cache invalidation works separately and correctly identifies the affected component, but this doesn't help because the rebuild logic depends on `affected_components`.

## Proposed Solutions

### Solution 1: Fix Path Comparison (Immediate)

Fix the location-based checking in `is_component_affected()`:

```rust
fn is_component_affected(&self, spec: &ComponentBuildSpec, batch: &ChangeBatch) -> bool {
    // ... existing watch pattern logic ...

    if let Some(loc) = location {
        // Normalize both paths for comparison
        let abs_location = self.base_dir.join(loc).canonicalize().unwrap_or_else(|_| self.base_dir.join(loc));

        for path in &batch.modified {
            // Also canonicalize the changed path
            let abs_path = path.canonicalize().unwrap_or_else(|_| path.clone());

            // Check if the file is under the component's location
            if abs_path.starts_with(&abs_location) {
                info!("Component {} affected by change to: {}", spec.component_name, path.display());
                return true;
            }

            // Also check if the paths share a common ancestor
            // This handles cases where the structure might be different
            if path.components().any(|c| c.as_os_str() == loc.as_os_str()) {
                info!("Component {} affected by related change: {}", spec.component_name, path.display());
                return true;
            }
        }
    }
}
```

### Solution 2: Add Debug Logging

Add comprehensive logging to understand path mismatches:

```rust
fn is_component_affected(&self, spec: &ComponentBuildSpec, batch: &ChangeBatch) -> bool {
    debug!("Checking if component {} is affected by {} changes",
        spec.component_name, batch.len());

    // Log the paths being compared
    if let Some(loc) = location {
        let abs_location = self.base_dir.join(loc);
        debug!("  Component location: {} (base: {}, relative: {})",
            abs_location.display(), self.base_dir.display(), loc);

        for path in &batch.modified {
            debug!("  Checking against modified file: {}", path.display());
            // ... comparison logic ...
        }
    }
}
```

### Solution 3: Use Cache Invalidation Results

Since cache invalidation correctly identifies affected components, use those results:

```rust
async fn handle_rebuild(&mut self, mut batch: ChangeBatch) -> Result<()> {
    // ... existing code ...

    // If no affected components identified by watcher,
    // use cache invalidation results as fallback
    if batch.affected_components.is_empty() && !all_changed_files.is_empty() {
        let invalidated = self.build_orchestrator.get_invalidated_components(&all_changed_files).await?;
        batch.affected_components = invalidated.into_iter().collect();
        info!("Using cache invalidation results: {} components affected",
            batch.affected_components.len());
    }

    // ... continue with rebuild ...
}
```

### Solution 4: Default Watch Patterns

Add default watch patterns for all components based on their build type:

```yaml
frontend:
  build_type: "TrunkWasm"
  location: "frontend/webui"
  watch:
    - "frontend/**/*.rs"
    - "frontend/**/*.toml"
    - "frontend/**/*.html"
    - "frontend/**/*.css"

backend:
  build_type: "RustBinary"
  location: "backend/server"
  watch:
    - "backend/**/*.rs"
    - "backend/**/*.toml"
```

## Immediate Workaround

Until the fix is implemented:

1. **Add explicit watch patterns** to all components in stack.spec.yaml
2. **Force rebuild manually** when changes are made: `rush --force`
3. **Restart Rush** to pick up changes

## Testing the Fix

1. Make a change to `frontend/src/lib.rs`
2. Check logs for:
   - "Component frontend affected by change"
   - "triggering rebuild for 1 components" (not 0)
   - Successful rebuild and container restart
3. Verify the application reflects the changes

## Impact

- **Severity**: Critical - Core development feature broken
- **User Impact**: No automatic rebuilds, must manually restart
- **Root Cause**: Path comparison logic failing in file watcher
- **Fix Complexity**: Low - Path normalization should resolve it