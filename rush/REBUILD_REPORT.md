# Rebuild Not Triggering - Detailed Analysis Report

## Executive Summary

When source files are modified, the system correctly detects changes and invalidates the cache, but then immediately uses a cached Docker image instead of rebuilding. This is due to a path comparison bug in the cache invalidation logic.

## Problem Timeline

From the logs provided:
```
12:55:06 File changes detected, triggering rebuild for 1 components
12:55:06 Invalidating cache for 2 changed files
12:55:06 Invalidated cache entries for 2 changed files  
12:55:06 Building 1 components
12:55:06 Using cached image for frontend: helloworld.wonop.io/frontend:20250827-105445  ← BUG
```

## Root Cause Analysis

### The Bug Location

**File**: `rush/crates/rush-container/src/build/cache.rs`
**Method**: `invalidate_changed()` at line 180-214
**Issue**: Path comparison mismatch between absolute and relative paths

### The Faulty Code

```rust
// Line 198-202 in cache.rs
if let Some(loc) = location {
    for file in changed_files {
        if file.starts_with(loc) {  // ← BUG: Comparing absolute path with relative path
            invalidated.push(component.clone());
            break;
        }
    }
}
```

### Why It Fails

1. **changed_files contains absolute paths**:
   - Example: `/Users/tfr/Documents/Projects/rush/products/io.wonop.helloworld/frontend/webui/src/main.rs`

2. **location is a relative path from the build spec**:
   - Example: `frontend/webui`

3. **The comparison always fails**:
   - `/Users/tfr/.../main.rs`.starts_with(`frontend/webui`) = **false**
   - Therefore, the cache entry is never actually removed

4. **Result**: 
   - `invalidate_changed()` runs but doesn't remove any entries
   - `build_components()` finds the "invalidated" entry still in cache
   - Uses cached image instead of rebuilding

## Why It Appeared to Work Earlier

The file watcher correctly detects changes and triggers rebuilds. The cache invalidation SAYS it invalidated entries, but it actually didn't. If cache was disabled or if this was the first build, rebuilds would work correctly.

## The Fix Required

The `invalidate_changed` method needs to properly compare paths. Options:

### Option 1: Convert relative locations to absolute paths (Recommended)
```rust
pub async fn invalidate_changed(&mut self, changed_files: &[PathBuf]) {
    let mut invalidated = Vec::new();
    
    for (component, entry) in &self.entries {
        if let Some(spec) = &entry.spec {
            let location = // ... get location from build_type
            
            if let Some(loc) = location {
                // Convert relative location to absolute for comparison
                let abs_location = if Path::new(loc).is_absolute() {
                    PathBuf::from(loc)
                } else {
                    // Need access to base_dir/product_dir here
                    self.base_dir.join(loc)  
                };
                
                for file in changed_files {
                    if file.starts_with(&abs_location) {
                        invalidated.push(component.clone());
                        break;
                    }
                }
            }
        }
    }
    // ... rest of method
}
```

### Option 2: Store absolute paths in cache entries
When creating cache entries, store the absolute path of the component location so comparison is straightforward.

### Option 3: Use canonical paths
Canonicalize both paths before comparison to handle any path inconsistencies.

## Additional Issues Found

1. **Missing base_dir in BuildCache**: The cache doesn't have access to the product directory to resolve relative paths to absolute paths.

2. **Logging misleads**: The log says "Invalidated cache entries for 2 changed files" but no entries were actually invalidated.

## Verification Steps

To confirm this analysis:
1. Add debug logging in `invalidate_changed()` to print the paths being compared
2. You'll see absolute paths never match relative paths
3. The `invalidated` vector remains empty
4. Cache entries are never removed

## Impact

- File changes are detected ✓
- Watcher triggers rebuild ✓  
- Cache invalidation is called ✓
- But cache entries remain ✗
- Cached images are used instead of rebuilding ✗
- **Result**: Changes don't trigger actual rebuilds

## Recommended Solution

1. Add `base_dir: PathBuf` field to `BuildCache` struct
2. Pass the product directory when creating the cache
3. In `invalidate_changed()`, convert relative locations to absolute paths before comparison
4. Add debug logging to verify paths are being compared correctly

This is a critical bug that completely breaks the hot-reload development workflow.