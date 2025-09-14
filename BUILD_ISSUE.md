# Build and Rollout Issues Analysis

## Executive Summary

Two critical issues were identified in the Rush CLI implementation:
1. **State Transition Error**: Invalid state transition from `Building` to `Idle` causing build failures
2. **K8s Manifest Generation**: Missing integration between component specifications and Kubernetes manifest templates

## Issue 1: Invalid State Transition Error

### Problem
```
Internal error: Invalid state transition from Building to Idle
```

### Root Cause
**Location**: `rush/crates/rush-container/src/reactor/modular_core.rs:948`

The `build()` method attempts to transition from `Building` state to `Idle` state after build completion:
```rust
// Line 945-949
// Transition back to idle after building
{
    let mut state = self.state.write().await;
    state.transition_to(ReactorPhase::Idle)?;  // ❌ INVALID TRANSITION
}
```

However, the state machine definition in `rush/crates/rush-container/src/reactor/state.rs:188-224` does not allow this transition:
```rust
// Valid transitions from Building state:
(ReactorPhase::Building, ReactorPhase::Starting) => true,    // ✓ Valid
(ReactorPhase::Building, ReactorPhase::Error) => true,       // ✓ Valid
(ReactorPhase::Building, ReactorPhase::ShuttingDown) => true, // ✓ Valid
// Building → Idle is NOT listed, therefore invalid
```

### Solution
The build method should transition to `Starting` instead of `Idle`, or the state machine should be updated to allow `Building → Idle` transition if that's the intended behavior.

**Option 1: Fix the transition (Recommended)**
```rust
// Line 948 should be:
state.transition_to(ReactorPhase::Starting)?;
// Or simply remove the transition if staying in Building is acceptable
```

**Option 2: Update state machine**
```rust
// Add to state.rs:195
(ReactorPhase::Building, ReactorPhase::Idle) => true,
```

## Issue 2: K8s Manifest Generation Problems

### Problem
Kubernetes manifests are not being generated with the correct structure and naming convention.

### Root Cause Analysis

#### Missing Component-to-Manifest Mapping

**Reference Implementation** (`ref/rush/src/container/container_reactor.rs`):
- Uses `K8ClusterManifests` structure that maintains component-to-manifest mappings
- Components are added with priority prefixes: `{priority}_{component_name}`
- Each component has a `k8s` field pointing to its manifest directory
- Manifests are rendered per component with proper context

**Current Implementation** (`rush/crates/rush-container/src/reactor/modular_core.rs`):
- Uses generic `ManifestGenerator` without component-specific context
- Missing the priority-based naming convention
- Not reading manifest templates from component directories (`frontend/infrastructure/`, `backend/infrastructure/`)
- Attempts to generate all manifests at once without component separation

#### Specific Issues in build_manifests()

1. **Missing Component K8s Path Integration**:
   - Components in `stack.spec.yaml` define `k8s: frontend/infrastructure`
   - Current implementation doesn't use these paths
   - Should read templates from `products/{product}/{component}/infrastructure/`

2. **Incorrect Output Structure**:
   - Expected: `.rush/k8s/{priority}_{component}/` (e.g., `50_backend/`)
   - Current: Flat structure in `.rush/k8s/`

3. **Missing Component Context**:
   - Each component needs its own build context for template rendering
   - Current implementation tries to generate all manifests with a single context

### Required Changes

#### 1. Update ComponentBuildSpec Structure
Add the `k8s` field to track manifest locations:
```rust
pub struct ComponentBuildSpec {
    // ... existing fields ...
    pub k8s: Option<String>,  // Path to K8s manifests (e.g., "backend/infrastructure")
    pub priority: u32,        // Component priority for ordering
}
```

#### 2. Implement Proper Manifest Generation
```rust
pub async fn build_manifests(&mut self) -> Result<()> {
    let output_dir = std::path::PathBuf::from(".rush/k8s");

    // Clear existing manifests
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir)?;
    }

    for spec in &self.component_specs {
        // Skip components without K8s manifests
        let k8s_path = match &spec.k8s {
            Some(path) => path,
            None => continue,
        };

        // Create component-specific output directory with priority
        let component_dir = format!("{}_{}", spec.priority, spec.component_name);
        let component_output = output_dir.join(&component_dir);
        std::fs::create_dir_all(&component_output)?;

        // Read templates from component's infrastructure directory
        let template_dir = PathBuf::from(&spec.product_dir)
            .join(k8s_path);

        // Get component-specific secrets
        let secrets = // ... fetch from vault for this component

        // Create component-specific build context
        let context = spec.generate_build_context(toolchain, secrets);

        // Render each template in the component's directory
        for template_file in std::fs::read_dir(&template_dir)? {
            // Render template with component context
            // Write to component_output directory
        }
    }
}
```

#### 3. Parse K8s Field from stack.spec.yaml
Update the stack.spec.yaml parser to extract the `k8s` field for each component.

## Impact Assessment

### Current State
- Build command fails with state transition error
- Rollout command fails at the build stage
- K8s manifests are not generated correctly even if build succeeds

### After Fixes
- Build will complete successfully
- K8s manifests will be generated with correct structure
- Rollout will be able to copy properly structured manifests to infrastructure repo

## Recommended Fix Priority

1. **Immediate**: Fix state transition error (5 minutes)
   - Change line 948 in modular_core.rs

2. **High Priority**: Fix K8s manifest generation (2-4 hours)
   - Add k8s field parsing from stack.spec.yaml
   - Implement component-based manifest generation
   - Update directory structure to match expected format

3. **Follow-up**: Add integration tests
   - Test state transitions
   - Test manifest generation with sample components
   - Test full rollout workflow

## Testing Recommendations

### Test State Transition Fix
```bash
./rush/target/release/rush io.wonop.helloworld build
# Should complete without "Invalid state transition" error
```

### Test Manifest Generation
```bash
./rush/target/release/rush io.wonop.helloworld rollout --env staging
# Check .rush/k8s/ structure:
ls -la .rush/k8s/
# Should show:
# 50_backend/
# 100_frontend/
# etc.
```

## Code References

- State machine definition: `rush/crates/rush-container/src/reactor/state.rs:186-224`
- Build method with error: `rush/crates/rush-container/src/reactor/modular_core.rs:925-953`
- Manifest generation: `rush/crates/rush-container/src/reactor/modular_core.rs:1424-1516`
- Reference implementation: `ref/rush/src/container/container_reactor.rs:680-710`
- Component specs: `products/io.wonop.helloworld/stack.spec.yaml`