# Ingress Component Not Starting - Analysis Report

## Executive Summary

The ingress component is not starting in dev mode because it is explicitly excluded at multiple points in the Rush container lifecycle. While other components get Docker containers created and started, the ingress component is skipped entirely despite having a valid Dockerfile and configuration.

## Root Causes Identified

### 1. Build Orchestrator Skips Ingress (Line 328-331)
**File**: `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/build/orchestrator.rs`

```rust
BuildType::Ingress { .. } => {
    // Ingress doesn't need a container image
    debug!("Skipping build for ingress {}", spec.component_name);
    Ok(String::new())
}
```

**Issue**: The build orchestrator returns an empty string for ingress components, meaning no Docker image is built.

### 2. Service Creation Skips Ingress (Line 804-805)
**File**: `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/reactor/modular_core.rs`

```rust
// Skip ingress and other special components that don't have images
if matches!(spec.component_name.as_str(), "ingress" | "database" | "stripe") {
    continue;
}
```

**Issue**: Even if an ingress image were built, the reactor explicitly skips creating a container service for components named "ingress".

### 3. No Special Ingress Handling
Unlike LocalService components (database, stripe) which have special handling logic, ingress components have no alternative launch mechanism despite being configured with:
- A valid Dockerfile at `./ingress/Dockerfile`
- Port configuration (9000:80)
- Component dependencies (frontend, backend)
- Artifacts to copy (`nginx.conf`)

## Configuration Analysis

The ingress component in `stack.spec.yaml` is properly configured:
```yaml
ingress:
  build_type: "Ingress"
  port: 9000
  target_port: 80
  location: "./ingress"
  context_dir: ../target
  dockerfile: "./ingress/Dockerfile"
  components:
    - "backend"
    - "frontend"
  artefacts:
    "./ingress/nginx.conf": nginx.conf
  depends_on:
    - frontend
    - backend
```

This configuration suggests the ingress should:
1. Build a Docker image from `./ingress/Dockerfile`
2. Run as a container on port 9000 (mapping to internal port 80)
3. Include nginx.conf as an artifact
4. Start after frontend and backend components

## Why This Breaks Dev Mode

In dev mode, the ingress component serves as the entry point for the application, routing requests between frontend and backend. Without it:
- No unified entry point exists for the application
- Frontend and backend run in isolation
- Routing between components doesn't work
- The application is not accessible as intended

## Code Flow Analysis

1. **Component Loading** (`from_product_dir`):
   - Ingress component IS loaded from stack.spec.yaml
   - ComponentBuildSpec is created correctly
   - Component is added to component_specs list

2. **Build Phase** (`build_orchestrator.build_components`):
   - ❌ Ingress is skipped - returns empty string instead of building image
   - No Docker image is created for ingress

3. **Service Creation** (`create_services_from_specs`):
   - ❌ Ingress is explicitly excluded from service creation
   - No container service is registered for ingress

4. **Container Launch**:
   - Since no service exists for ingress, no container is started

## Historical Context

The code comments suggest ingress was treated as a "special component" similar to database and stripe (LocalService types). However:
- LocalService components have alternative handling (native processes)
- Ingress has no such alternative - it needs a Docker container

This appears to be a design oversight where ingress was categorized with LocalService components but doesn't have the same alternative launch mechanism.

## Recommended Fix

### Option 1: Remove Ingress from Skip Lists (Simplest)
1. Remove "ingress" from the skip check in `modular_core.rs` line 804
2. Remove the skip logic for BuildType::Ingress in `orchestrator.rs` line 328-331
3. Let ingress be built and launched like any other container component

### Option 2: Add Special Ingress Handling (More Complex)
1. Create dedicated ingress handling logic similar to LocalService
2. Build the ingress Docker image in the build orchestrator
3. Add special container launch logic for ingress with proper port mapping

### Option 3: Treat Ingress as Regular Component
1. Change BuildType::Ingress to BuildType::Image in the configuration
2. Let existing image building and container launch logic handle it

## Testing Strategy

After implementing the fix:
```bash
# Build and run in dev mode
./target/release/rush helloworld.wonop.io dev

# Verify ingress container is running
docker ps | grep ingress

# Test routing through ingress
curl http://localhost:9000  # Should route to frontend
curl http://localhost:9000/api  # Should route to backend
```

## Impact Assessment

- **Severity**: High - Core functionality broken in dev mode
- **Affected Commands**: `rush dev`
- **Workaround**: None - ingress is essential for proper dev environment
- **Risk of Fix**: Low - Changes are localized to specific skip conditions

## Conclusion

The ingress component failure is due to explicit exclusion logic that treats it as a special component without providing alternative launch mechanisms. The simplest fix is to remove these exclusions and let ingress be handled like any other Docker-based component, since it has a valid Dockerfile and container configuration.