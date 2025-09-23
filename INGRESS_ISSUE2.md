# Ingress nginx.conf Service Rendering Issue - Investigation Report #2

## Resolution Status: ✅ FIXED
- **Issue**: nginx.conf artifacts failed to render services during partial rebuilds
- **Root Cause**: Only affected component specs were passed to build orchestrator during rebuilds
- **Solution Implemented**: Modified rebuild functions to pass ALL component specs to build orchestrator
- **Status**: Fixed and tested successfully

## Executive Summary
The nginx.conf artifact for the ingress component intermittently failed to render services correctly during rebuilds. The root cause was that when ingress was rebuilt individually (due to file changes or tag changes), only the ingress component spec was passed to the build orchestrator, preventing it from finding the backend and frontend component specifications needed to properly render the nginx.conf template.

## Problem Statement
When running `rush` in development mode, the ingress component's nginx.conf sometimes renders without any service configurations, resulting in 404 errors when trying to access the application. The logs show:
```
Component backend referenced by ingress not found in specs
Component frontend referenced by ingress not found in specs
```

## Root Cause Analysis

### The Issue
The problem occurs in two rebuild scenarios in `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/reactor/modular_core.rs`:

1. **Tag-based rebuilds** (lines 480-486 in `handle_rebuild`):
```rust
// Get component specs for affected components
let component_specs = {
    let state = self.state.read().await;
    batch.affected_components.iter()
        .filter_map(|name| state.get_component(name))
        .filter_map(|comp| comp.build_spec.as_ref().cloned())
        .collect::<Vec<_>>()
};
```

2. **Manual rebuilds** (lines 620-625 in `handle_manual_rebuild`):
```rust
// Get component specs for affected components
let component_specs = {
    let state = self.state.read().await;
    components.iter()
        .filter_map(|name| state.get_component(name))
        .filter_map(|comp| comp.build_spec.as_ref().cloned())
        .collect::<Vec<_>>()
};
```

Both functions filter `component_specs` to only include the components being rebuilt. When only the ingress component is rebuilt, the filtered specs don't include backend or frontend components.

### The Artifact Rendering Process
In `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/build/orchestrator.rs` (line 738-783):

1. The `render_artifacts_for_component` function receives `all_specs` parameter
2. For ingress components, it looks up dependent services from `all_specs`
3. If a referenced component isn't in `all_specs`, it logs a warning and skips it
4. The resulting nginx.conf only includes services found in `all_specs`

### Why It's Intermittent
- **Initial build**: Works correctly because ALL component specs are passed
- **Full rebuild with --force-rebuild**: Works correctly because ALL components are rebuilt
- **Partial rebuild (file/tag change)**: FAILS because only changed component specs are passed

## Reproduction Steps
1. Start rush in dev mode: `rush compoundcoders.com dev`
2. Make a change to the ingress configuration or wait for tag-based rebuild
3. Observe the rebuild logs showing "Component X referenced by ingress not found in specs"
4. The rendered nginx.conf will be missing service configurations
5. Accessing the application results in 404 errors

## Impact
- Users get 404 errors when trying to access the application after ingress rebuilds
- The ingress proxy doesn't route to backend/frontend services
- Requires manual restart of rush to fix the issue temporarily

## Solution Implemented

### Changes Made
Modified both `handle_rebuild` and `handle_manual_rebuild` in `modular_core.rs` to pass all component specs to the build orchestrator:

1. **handle_rebuild** (lines 479-500):
   - Still validates that affected components exist
   - Passes `self.component_specs.clone()` instead of filtered specs
   - Build orchestrator receives full context for artifact rendering

2. **handle_manual_rebuild** (lines 621-637):
   - Similar changes as handle_rebuild
   - Also fixed to only start rebuilt containers (lines 651-666)

### How It Works
- The build orchestrator receives ALL component specs
- It still only builds components that need rebuilding (based on cache/image checks)
- Artifact rendering now has full context to find all referenced services
- Only the actually rebuilt containers are restarted

## Workarounds (Current)
1. Use `--force-rebuild` flag to rebuild all components
2. Restart rush completely when nginx.conf is incorrectly rendered
3. Manually trigger rebuild of all components

## Code Locations
- **Problem Location 1**: `rush/crates/rush-container/src/reactor/modular_core.rs:480-486` (handle_rebuild)
- **Problem Location 2**: `rush/crates/rush-container/src/reactor/modular_core.rs:620-625` (handle_manual_rebuild)
- **Artifact Rendering**: `rush/crates/rush-container/src/build/orchestrator.rs:738-783` (render_artifacts_for_component)
- **Warning Log**: `rush/crates/rush-container/src/build/orchestrator.rs:782`

## Testing the Fix
After implementing the fix:
1. Start rush in dev mode
2. Modify a file that triggers ingress rebuild
3. Verify logs show all services being found
4. Check rendered nginx.conf includes all service configurations
5. Verify application is accessible without 404 errors

## Prevention
1. Add integration tests for partial rebuilds with artifact rendering
2. Add validation that ingress artifacts contain expected services
3. Consider adding a pre-build check that all required specs are available