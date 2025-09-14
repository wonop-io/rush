# Rollout Manifest Generation Panic Analysis

## Executive Summary

The rollout command panics during Kubernetes manifest generation with "No services found for docker image" because the `generate_build_context` method expects `services` and `domains` fields to be set, but these are never initialized when building manifests. This is a fundamental design issue where the same method is being used for two different contexts (Docker builds vs. K8s manifest generation) with different requirements.

## Error Details

```
thread 'main' panicked at crates/rush-build/src/spec.rs:700:14:
No services found for docker image
```

The panic occurs when generating Kubernetes manifests for the frontend component after successfully pushing Docker images.

## Root Cause Analysis

### 1. The Panic Location

**Location**: `rush/crates/rush-build/src/spec.rs:697-705`

```rust
pub fn generate_build_context(
    &self,
    toolchain: Option<Arc<ToolchainContext>>,
    secrets: HashMap<String, String>,
) -> BuildContext {
    let services = self
        .services
        .clone()
        .expect("No services found for docker image");  // ← PANICS HERE

    let domains = (*self
        .domains
        .clone()
        .expect("No domains found for docker image"))  // ← WOULD ALSO PANIC
    .clone();
```

### 2. How ComponentBuildSpec is Created

**Location**: `rush/crates/rush-build/src/spec.rs:543-550`

```rust
ComponentBuildSpec {
    // ... other fields ...
    services: None,  // ← Initialized to None
    tagged_image_name: None,
    dotenv,
    domain: yaml_section
        .get("domain")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()),
    domains: None,  // ← Also initialized to None
}
```

### 3. Where services Should Be Set (But Isn't)

The `ComponentBuildSpec` has a method `set_services()` at line 134-136:
```rust
pub fn set_services(&mut self, services: Arc<HashMap<String, Vec<ServiceSpec>>>) {
    self.services = Some(services);
}
```

However, **this method is never called** anywhere in the codebase during manifest generation.

### 4. The Manifest Generation Flow

**Location**: `rush/crates/rush-container/src/reactor/modular_core.rs:1530-1532`

```rust
// Create build context for this component
let toolchain = Arc::new(rush_toolchain::ToolchainContext::default());
let build_context = spec.generate_build_context(Some(toolchain), component_secrets);
//                        ↑ This calls the method that expects services to be set
```

The manifest generation directly calls `generate_build_context` without:
1. Setting up services
2. Setting up domains
3. Checking if these fields are needed for manifest generation

### 5. Context Mismatch

The `generate_build_context` method is designed for **Docker container builds** where:
- Services need to be configured for ingress routing
- Domains are required for networking setup
- The method filters services based on component type (especially for Ingress)

However, it's being reused for **Kubernetes manifest generation** where:
- Services are not needed (K8s defines its own Service resources)
- Domains are handled differently in K8s
- The context is primarily for template variable substitution

## Why This Wasn't Caught Earlier

1. **Local Development Works**: The `dev` and `build` commands don't trigger manifest generation
2. **Different Code Path**: Local container running uses different methods that properly set services
3. **Missing Integration Tests**: No tests cover the full rollout → manifest generation flow

## Solution Options

### Option 1: Make services and domains Optional (Quick Fix)

**Location to fix**: `rush/crates/rush-build/src/spec.rs:697-706`

```rust
pub fn generate_build_context(
    &self,
    toolchain: Option<Arc<ToolchainContext>>,
    secrets: HashMap<String, String>,
) -> BuildContext {
    // Make services optional with default empty map
    let services = self
        .services
        .clone()
        .unwrap_or_else(|| Arc::new(HashMap::new()));

    // Make domains optional with default empty vector
    let domains = self
        .domains
        .clone()
        .map(|d| (*d).clone())
        .unwrap_or_else(Vec::new);

    // Rest of the method...
}
```

### Option 2: Create Separate Methods (Recommended)

Create two distinct methods for different contexts:

```rust
impl ComponentBuildSpec {
    /// For Docker builds - requires services and domains
    pub fn generate_build_context(
        &self,
        toolchain: Option<Arc<ToolchainContext>>,
        secrets: HashMap<String, String>,
    ) -> BuildContext {
        // Current implementation with expects
    }

    /// For K8s manifest generation - doesn't require services/domains
    pub fn generate_manifest_context(
        &self,
        toolchain: Option<Arc<ToolchainContext>>,
        secrets: HashMap<String, String>,
    ) -> BuildContext {
        // Simplified implementation without services/domains requirements
        let services = self.services.clone()
            .unwrap_or_else(|| Arc::new(HashMap::new()));
        let domains = self.domains.clone()
            .map(|d| (*d).clone())
            .unwrap_or_else(Vec::new);

        // Continue with context generation...
    }
}
```

Then update `modular_core.rs:1532`:
```rust
let build_context = spec.generate_manifest_context(Some(toolchain), component_secrets);
```

### Option 3: Initialize Services/Domains for Manifests

Before calling `generate_build_context`, initialize the required fields:

```rust
// In build_manifests() before line 1532
let mut spec_with_services = spec.clone();
spec_with_services.set_services(Arc::new(HashMap::new()));
spec_with_services.set_domains(Arc::new(Vec::new()));
let build_context = spec_with_services.generate_build_context(Some(toolchain), component_secrets);
```

## Implementation Recommendation

**Use Option 1 for immediate fix**, then refactor to Option 2 for long-term maintainability.

### Immediate Fix Implementation

```rust
// rush/crates/rush-build/src/spec.rs:697-706
pub fn generate_build_context(
    &self,
    toolchain: Option<Arc<ToolchainContext>>,
    secrets: HashMap<String, String>,
) -> BuildContext {
    // Make services optional with default empty map
    let services = self
        .services
        .clone()
        .unwrap_or_else(|| Arc::new(HashMap::new()));

    // Make domains optional with default empty vector
    let domains = self
        .domains
        .clone()
        .map(|d| (*d).clone())
        .unwrap_or_else(Vec::new);

    let (location, filtered_services) = match &self.build_type {
        // ... rest of the implementation
```

This fix:
1. Prevents the panic
2. Provides sensible defaults for manifest generation
3. Maintains backward compatibility with Docker builds
4. Allows rollout to complete successfully

## Testing

After implementing the fix:

```bash
# Should complete without panics
./rush/target/release/rush --env staging io.wonop.helloworld rollout

# Check generated manifests
ls -la .rush/k8s/
# Should show properly generated manifest files
```

## Long-term Recommendations

1. **Separate Concerns**: Create distinct methods for Docker builds vs. K8s manifests
2. **Add Integration Tests**: Test the full rollout pipeline including manifest generation
3. **Document Requirements**: Clearly document which fields are required for which operations
4. **Consider Builder Pattern**: Use a builder pattern for ComponentBuildSpec to ensure required fields are set for specific contexts

## Impact

- **Current**: Rollout fails completely when generating manifests
- **After Fix**: Manifest generation will complete successfully with appropriate defaults
- **No Side Effects**: Docker builds will continue to work as before since they properly set services/domains

## Related Code References

1. ComponentBuildSpec creation: `rush/crates/rush-build/src/spec.rs:470-550`
2. generate_build_context: `rush/crates/rush-build/src/spec.rs:692-770`
3. Manifest generation: `rush/crates/rush-container/src/reactor/modular_core.rs:1420-1577`
4. set_services method (unused): `rush/crates/rush-build/src/spec.rs:134-136`