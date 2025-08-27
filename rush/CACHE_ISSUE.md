# Cache Invalidation Issue Analysis

## Problem Summary

The file watcher correctly detects file changes and triggers rebuilds, but the build cache is not being invalidated when files change. This results in cached images being used even when the underlying source code has been modified, causing the frontend to reload without reflecting the expected changes.

## Root Cause Analysis

### 1. Cache Invalidation Never Called

The build cache has a well-designed `invalidate_changed(&mut self, changed_files: &[PathBuf])` method in `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/build/cache.rs:180`, but this method is **never called** in the rebuild flow.

**Evidence:**
- `grep -r "invalidate_changed" crates/` shows the method is only defined, never invoked
- The method is not called in `BuildOrchestrator::build_components()` 
- The method is not called in `Reactor::handle_rebuild()`

### 2. Rebuild Process Uses Cache Incorrectly

In `modular_core.rs`, when file changes trigger a rebuild via `handle_rebuild()`:

```rust
let build_result = self.build_orchestrator.build_components(component_specs, false).await;
```

The `force_rebuild: false` parameter means the build orchestrator will:
1. Check if caching is enabled (`self.config.enable_cache && !force_rebuild`)
2. Look for cached images (`cache_guard.get(&spec.component_name)`)
3. Check if cache is expired (`!cache_guard.is_expired(&spec.component_name)`)
4. **Use cached images if they're not time-expired**

### 3. Cache Expiration Logic Issue

The cache uses **time-based expiration only** in `cache.rs:164-177`:

```rust
pub async fn is_expired(&self, component: &str) -> bool {
    if let Some(entry) = self.entries.get(component) {
        let age = chrono::Utc::now() - entry.built_at;
        let expired = age > chrono::Duration::from_std(self.expiry).unwrap();
        // ...
    }
}
```

**Default expiry: 1 hour** (`Duration::from_secs(3600)` in `cache.rs:96`)

This means that within 1 hour of a build, the cache will always return cached images regardless of source file changes.

### 4. File Change Information Available But Unused

The `ChangeBatch` struct contains all the information needed for proper cache invalidation:

```rust
pub struct ChangeBatch {
    pub modified: Vec<PathBuf>,    // ✅ Available
    pub created: Vec<PathBuf>,     // ✅ Available  
    pub deleted: Vec<PathBuf>,     // ✅ Available
    pub affected_components: HashSet<String>, // ✅ Available
}
```

However, the `handle_rebuild()` method discards this file path information and only passes component names to the build orchestrator.

## Event Flow Analysis

```
File Change → Watcher Detects → ChangeBatch Created (with file paths)
     ↓
Reactor::handle_rebuild(components: HashSet<String>) [FILE PATHS LOST]
     ↓  
BuildOrchestrator::build_components(specs, force_rebuild: false)
     ↓
Cache Check: is_expired() [TIME-BASED ONLY]
     ↓
Cache Hit → Uses Stale Cached Image ❌
```

## Proposed Solution

### 1. Invalidate Cache Before Rebuild

Modify `Reactor::handle_rebuild()` to accept the full `ChangeBatch` instead of just component names:

```rust
// Current signature
async fn handle_rebuild(&mut self, components: std::collections::HashSet<String>) -> Result<()>

// Proposed signature  
async fn handle_rebuild(&mut self, batch: ChangeBatch) -> Result<()>
```

Then invalidate cache using the file change information:

```rust
async fn handle_rebuild(&mut self, batch: ChangeBatch) -> Result<()> {
    debug!("Handling rebuild for components: {:?}", batch.affected_components);
    
    // Invalidate cache based on changed files
    let all_changed_files: Vec<PathBuf> = batch.modified.iter()
        .chain(batch.created.iter())
        .chain(batch.deleted.iter())
        .cloned()
        .collect();
    
    self.build_orchestrator.invalidate_cache_for_files(&all_changed_files).await?;
    
    // ... rest of rebuild logic
}
```

### 2. Add Cache Invalidation to BuildOrchestrator

Add a public method to `BuildOrchestrator` to expose cache invalidation:

```rust
impl BuildOrchestrator {
    pub async fn invalidate_cache_for_files(&self, changed_files: &[PathBuf]) -> Result<()> {
        let mut cache = self.cache.lock().await;
        cache.invalidate_changed(changed_files).await;
        Ok(())
    }
}
```

### 3. Update Event Flow

```
File Change → Watcher Detects → ChangeBatch Created (with file paths)
     ↓
Reactor::handle_rebuild(batch: ChangeBatch) [FILE PATHS PRESERVED]
     ↓
BuildOrchestrator::invalidate_cache_for_files(changed_files)
     ↓  
BuildOrchestrator::build_components(specs, force_rebuild: false)
     ↓
Cache Check: Component not in cache due to invalidation
     ↓
Fresh Build → New Image ✅
```

### 4. Alternative: Force Rebuild for File Changes

If cache invalidation proves complex, a simpler approach would be to use `force_rebuild: true` when file changes are detected:

```rust
async fn handle_rebuild(&mut self, batch: ChangeBatch) -> Result<()> {
    // ... existing logic ...
    
    // Force rebuild when file changes detected (bypasses cache entirely)
    let build_result = self.build_orchestrator.build_components(component_specs, true).await;
    
    // ... rest of logic ...
}
```

This bypasses the cache completely for file-change-triggered rebuilds.

## Impact Analysis

**Current State:** 
- ❌ File changes don't invalidate cache
- ❌ Cached images used for up to 1 hour regardless of source changes
- ❌ Developers see stale builds after code changes

**After Fix:**
- ✅ File changes properly invalidate relevant cache entries
- ✅ Fresh builds triggered when source files change
- ✅ Developers see updated builds immediately
- ✅ Cache still provides benefits for unchanged components

## Recommendation

Implement **Solution 1 + 2** (cache invalidation) as it provides the best balance:
- Proper cache invalidation based on file changes
- Maintains cache benefits for unchanged components  
- More sophisticated and correct solution
- Aligns with the existing cache invalidation infrastructure

The force rebuild approach (Solution 4) could be used as a temporary workaround if needed.