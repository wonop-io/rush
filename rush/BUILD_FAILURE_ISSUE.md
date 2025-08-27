# Build Failure Handling Issue Analysis

## Problem Statement

When source files contain syntax errors that cause build failures, containers appear to restart rather than properly failing. This gives the impression that the build system is not working correctly.

## Current Behavior Analysis

From the logs, the system **is** correctly detecting build failures:

```
12:05:44 [SCRIPT] frontend | error: could not compile `webui` (bin "webui") due to 1 previous error
12:05:44 [SYSTEM] rush_container | Failed to build frontend: Docker error: Process /bin/sh failed with status: exit status: 1
12:05:44 [SYSTEM] rush_container | Build failed for 1 components
12:05:44 [SYSTEM] rush_container | Rebuild failed: Build error: Failed to build 1 components
```

Cache invalidation is also working correctly:
```
12:05:49 [SYSTEM] rush_container | Invalidating cache for 7 changed files
```

## Root Cause Analysis

### Issue 1: Existing Containers Continue Running

When file-triggered rebuilds fail, existing containers from previous successful builds continue running:

```bash
$ docker ps -a | grep helloworld
d70c1381d23d   helloworld.wonop.io/frontend:20250827-100225   # Still running from previous build
```

**This is by design** for development workflow - developers don't want their working containers to stop every time they make a syntax error.

### Issue 2: Lifecycle Manager Behavior on Build Failures

Looking at the `handle_rebuild` method in `modular_core.rs:316-320`:

```rust
if let Err(e) = self.handle_rebuild(batch).await {
    error!("Rebuild failed: {}", e);
    // Don't break the loop, continue processing  <-- Key issue
}
```

When builds fail:
1. ✅ Build errors are properly detected and logged
2. ✅ Cache invalidation occurs correctly  
3. ❌ **No container lifecycle changes happen** - old containers keep running
4. ✅ Reactor continues processing (doesn't crash)

### Issue 3: Container Stop Logic

In `handle_rebuild()`, containers are stopped **before** the build:

```rust
// Stop affected containers before rebuilding
for component_name in &batch.affected_components {
    if let Err(e) = self.lifecycle_manager.stop_component(component_name).await {
        warn!("Failed to stop component {}: {}", component_name, e);
    }
}
```

**But** if the build fails, no new containers are started, leaving **no containers running** for the failed components.

The user is probably seeing containers from a different timeframe or there's some container restart logic we haven't identified yet.

## Expected vs Actual Behavior

### For Initial Builds (startup)
- **Expected**: If build fails during startup, no containers should start
- **Actual**: ✅ This works correctly - failed builds prevent container startup

### For File-Change Triggered Rebuilds  
- **Current**: Old containers keep running when new builds fail
- **User Expected**: Containers should stop/fail when builds fail
- **Better Behavior**: Old containers should keep running (so developers have working environment) but clear indication that build failed

## Potential Issues

1. **Container Restart Policies**: Docker containers might have restart policies causing them to restart automatically
2. **Lifecycle Manager Logic**: There might be logic restarting containers independent of build results
3. **State Management**: Reactor state might not properly reflect build failures
4. **Multiple Build Attempts**: Continuous file watching might be triggering multiple rebuild attempts

## Recommended Investigation

1. Check Docker container restart policies
2. Examine lifecycle manager for automatic restart logic
3. Verify that containers are actually stopping during failed rebuilds
4. Check if there are multiple build processes running simultaneously

## Proposed Solutions

### Option 1: Stop Containers on Build Failure (Breaking Change)
```rust
// In handle_rebuild, after build failure:
if build_result.is_err() {
    // Stop containers for failed builds
    for component_name in &batch.affected_components {
        self.lifecycle_manager.stop_component(component_name).await?;
    }
}
```

### Option 2: Better Status Indication (Recommended)
- Keep current behavior (old containers run)
- Add clear UI/logging to show build status
- Maybe add a "stale" indicator for running containers with failed builds

### Option 3: Configurable Behavior
Allow developers to choose:
- `--stop-on-failure`: Stop containers when builds fail
- `--keep-running`: Keep old containers running (current behavior)

## Need More Information

To fully diagnose, we need to determine:
1. Are containers actually restarting, or just continuing to run from previous successful builds?
2. Is there automatic restart logic in the lifecycle manager?
3. What is the user's expected behavior in development mode?