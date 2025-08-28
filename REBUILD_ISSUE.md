# Ingress Component Not Rebuilding on Artifact Changes

## Executive Summary

The ingress component is not rebuilding when its artifacts (e.g., nginx.conf) change. The Docker image shows as 11 hours old despite recent artifact modifications. This is due to multiple gaps in the build cache invalidation logic.

## Root Causes

### 1. **Ingress BuildType Not Handled in Cache Invalidation**

**Location**: `crates/rush-container/src/build/cache.rs:196-204`

The cache invalidation logic only checks for file changes in components with a `location` field:

```rust
let location = match &spec.build_type {
    rush_build::BuildType::RustBinary { location, .. } => Some(location.as_str()),
    rush_build::BuildType::TrunkWasm { location, .. } => Some(location.as_str()),
    rush_build::BuildType::DixiousWasm { location, .. } => Some(location.as_str()),
    rush_build::BuildType::Script { location, .. } => Some(location.as_str()),
    rush_build::BuildType::Zola { location, .. } => Some(location.as_str()),
    rush_build::BuildType::Book { location, .. } => Some(location.as_str()),
    _ => None,  // ← Ingress falls through here
};
```

**Impact**: BuildType::Ingress doesn't have a `location` field in this match, so it returns `None`. This means the ingress component is never considered for cache invalidation when files change.

### 2. **Artifacts Not Included in Cache Hash**

**Location**: `crates/rush-container/src/build/cache.rs:42-52`

The cache hash only includes the component name and build type:

```rust
fn hash_spec(spec: &ComponentBuildSpec) -> String {
    let mut hasher = DefaultHasher::new();
    spec.component_name.hash(&mut hasher);
    format!("{:?}", spec.build_type).hash(&mut hasher);  // Only build type, not artifacts
    format!("{:x}", hasher.finish())
}
```

**Impact**: Changes to artifact templates or their content don't invalidate the cache because:
- The `artefacts` field is not included in the hash
- The actual rendered artifact content is not tracked
- Template variables that affect artifact rendering are not considered

### 3. **No Tracking of Artifact Dependencies**

**Issues**:
- Artifact templates (e.g., `./ingress/nginx.conf`) are not monitored for changes
- The rendered artifact content is not checksummed
- Dynamic values used in artifact rendering (component ports, domains) are not tracked
- Changes to dependent components' ports don't trigger ingress rebuild

## Current Behavior

1. User modifies nginx.conf template or component ports change
2. Artifacts are re-rendered to `target/rushd/nginx.conf` with new content
3. Build orchestrator checks cache for ingress component
4. Cache returns the old image because:
   - File change detection doesn't cover ingress location
   - Hash doesn't include artifact content
   - No invalidation occurs
5. Old Docker image is reused despite artifact changes

## Evidence

From the Docker images list:
```
helloworld.wonop.io/frontend   20250828-075652   34 seconds ago   (rebuilt)
helloworld.wonop.io/ingress    20250828-075649   11 hours ago     (NOT rebuilt)
helloworld.wonop.io/backend    20250828-075647   3 days ago       (cached)
```

## Proposed Solutions

### Short-term Fix
1. Add BuildType::Ingress to the cache invalidation match statement
2. Include artifact hashes in the cache validation

### Long-term Improvements
1. **Track Artifact Dependencies**:
   - Monitor artifact template files for changes
   - Include artifact content hash in cache entries
   - Track component dependencies (ingress depends on backend/frontend ports)

2. **Enhanced Cache Validation**:
   ```rust
   fn hash_spec(spec: &ComponentBuildSpec) -> String {
       let mut hasher = DefaultHasher::new();
       spec.component_name.hash(&mut hasher);
       format!("{:?}", spec.build_type).hash(&mut hasher);
       
       // Hash artifacts
       if let Some(artifacts) = &spec.artefacts {
           for (source, _) in artifacts {
               source.hash(&mut hasher);
               // Also hash the actual template content if available
           }
       }
       
       // Hash dependent component ports for ingress
       if matches!(spec.build_type, BuildType::Ingress { .. }) {
           // Include port mappings that affect nginx.conf
       }
       
       format!("{:x}", hasher.finish())
   }
   ```

3. **File Watch Integration**:
   - Watch artifact template files
   - Trigger cache invalidation when templates change
   - Invalidate ingress when dependent component ports change

## Workaround

Until fixed, users can:
1. Use `--force-rebuild` flag to bypass cache
2. Manually delete the cached image
3. Modify the Dockerfile to force a rebuild

## Impact

This issue affects any component that:
- Uses BuildType::Ingress
- Has dynamic artifacts that change independently of source code
- Depends on other components' configuration (ports, domains)

The issue is particularly problematic for ingress components because their primary function is routing, which depends entirely on artifact configuration rather than source code.