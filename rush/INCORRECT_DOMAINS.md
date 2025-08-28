# Incorrect Domain Calculation for Component Services in Artifact Generation

## Status: ✅ FIXED

## Executive Summary

The Rush build system had a critical bug in domain calculation during artifact generation for ingress components. **This issue has been fixed.** When rendering artifacts (like `nginx.conf`), the system incorrectly uses the ingress component's domain for ALL referenced service components, instead of calculating each component's individual domain based on its own subdomain configuration. This causes all services to be incorrectly configured with the same domain, breaking multi-subdomain deployments.

## Problem Description

### Current Behavior (INCORRECT)
When an ingress component references other components and renders artifacts:
1. The ingress component calculates its own domain correctly using its subdomain
2. When creating `ServiceSpec` objects for referenced components, it assigns its own domain to ALL services
3. All services end up with the ingress component's domain instead of their individual domains

### Expected Behavior
Each component should have its domain calculated based on its own subdomain configuration:
- Component A with `subdomain: "api"` → `api.product.com`
- Component B with `subdomain: "app"` → `app.product.com`  
- Component C with no subdomain → `product.com`

### Impact
This bug prevents proper multi-domain/subdomain routing in production environments where different services need to be accessible at different subdomains (e.g., `api.example.com`, `app.example.com`, `admin.example.com`).

## Root Cause Analysis

### Location of Bug
**File:** `/rush/crates/rush-container/src/build/orchestrator.rs`
**Method:** `render_artifacts_for_component`
**Lines:** 703 and 727

### Problematic Code

```rust
// Line 697-709: Creating ServiceSpec for each component
if let Some(component_spec) = component_spec {
    let service_spec = rush_build::ServiceSpec {
        name: component_name.clone(),
        host: format!("{}-{}", spec.product_name, component_name),
        port: component_spec.port.unwrap_or(8080),
        target_port: component_spec.target_port.unwrap_or(80),
        mount_point: component_spec.mount_point.clone(),
        domain: spec.domain.clone(),  // ❌ BUG: Uses ingress domain for ALL services
        docker_host: format!("{}-{}", spec.product_name, component_name),
    };
    services_map.entry(spec.domain.clone())  // ❌ BUG: Groups all under same domain
        .or_insert_with(Vec::new)
        .push(service_spec);
}

// Line 727: BuildContext also uses wrong domain
let context = BuildContext {
    // ...
    domain: spec.domain.clone(),  // ❌ Uses ingress domain instead of component domains
    // ...
};
```

### Why This Happens

1. **Domain Calculation Flow:**
   - Each `ComponentBuildSpec` has its domain correctly calculated during creation in `spec.rs:417`
   - The domain is calculated using `config.domain(subdomain.clone())` where subdomain comes from the component's YAML
   - This correctly computed domain is stored in `spec.domain`

2. **The Bug:**
   - When the ingress component renders artifacts, it needs to create `ServiceSpec` objects for referenced components
   - Instead of using each component's `component_spec.domain`, it uses `spec.domain` (the ingress component's domain)
   - This causes all services to have the same domain in the rendered template

3. **Template Impact:**
   - The nginx.conf template iterates over services grouped by domain
   - Since all services have the same (incorrect) domain, they all get grouped under one server block
   - This breaks subdomain-based routing

## Code Flow Analysis

### 1. Component Spec Creation (WORKS CORRECTLY)
```rust
// rush-build/src/spec.rs:413-417
let subdomain = yaml_section
    .get("subdomain")
    .map(|v| Self::process_template_string(v.as_str().unwrap(), &variables));
let domain = config.domain(subdomain.clone());  // ✅ Correctly calculates domain per component
```

### 2. Domain Calculation Method (WORKS CORRECTLY)
```rust
// rush-config/src/types.rs:122-133
pub fn domain(&self, subdomain: Option<String>) -> String {
    let ctx = DomainContext {
        product_name: self.product_name.clone(),
        product_uri: self.product_uri.clone(),
        subdomain,  // ✅ Uses component-specific subdomain
    };
    // Renders template like "{{subdomain}}.{{product_uri}}"
    // ...
}
```

### 3. Artifact Rendering (BUG LOCATION)
```rust
// rush-container/src/build/orchestrator.rs:688-709
// When processing ingress component references:
for component_name in components {
    let component_spec = all_specs.iter()
        .find(|s| &s.component_name == component_name);
    
    if let Some(component_spec) = component_spec {
        // component_spec HAS the correct domain but we don't use it!
        let service_spec = rush_build::ServiceSpec {
            // ...
            domain: spec.domain.clone(),  // ❌ Should be component_spec.domain
            // ...
        };
    }
}
```

## Proposed Solution

### Fix 1: Use Component's Own Domain (Primary Fix)
```rust
// rush-container/src/build/orchestrator.rs:703
let service_spec = rush_build::ServiceSpec {
    name: component_name.clone(),
    host: format!("{}-{}", spec.product_name, component_name),
    port: component_spec.port.unwrap_or(8080),
    target_port: component_spec.target_port.unwrap_or(80),
    mount_point: component_spec.mount_point.clone(),
    domain: component_spec.domain.clone(),  // ✅ FIX: Use component's own domain
    docker_host: format!("{}-{}", spec.product_name, component_name),
};
```

### Fix 2: Group Services by Their Domain
```rust
// rush-container/src/build/orchestrator.rs:706-708
services_map.entry(component_spec.domain.clone())  // ✅ FIX: Group by component's domain
    .or_insert_with(Vec::new)
    .push(service_spec);
```

### Fix 3: Build Context Domain Handling
The BuildContext at line 727 should represent the ingress component's context, but the services map should contain the correct domains. This might be okay as-is since the services map has the correct domains now.

### Complete Fixed Code
```rust
if let Some(component_spec) = component_spec {
    let service_spec = rush_build::ServiceSpec {
        name: component_name.clone(),
        host: format!("{}-{}", spec.product_name, component_name),
        port: component_spec.port.unwrap_or(8080),
        target_port: component_spec.target_port.unwrap_or(80),
        mount_point: component_spec.mount_point.clone(),
        domain: component_spec.domain.clone(),  // ✅ Use component's domain
        docker_host: format!("{}-{}", spec.product_name, component_name),
    };
    services_map.entry(component_spec.domain.clone())  // ✅ Group by component's domain
        .or_insert_with(Vec::new)
        .push(service_spec);
} else {
    warn!("Component {} referenced by ingress not found in specs", component_name);
}
```

## Testing Requirements

### 1. Unit Tests
- Test that each component's domain is calculated correctly based on its subdomain
- Test that ServiceSpec objects have the correct domain for their component
- Test that services are grouped correctly by domain in the services map

### 2. Integration Tests
- Create a test with multiple components having different subdomains
- Verify the rendered nginx.conf has separate server blocks for each domain
- Test that routing works correctly for each subdomain

### 3. Example Test Configuration
```yaml
# Component configurations
frontend:
  subdomain: "app"  # Should result in app.example.com
  # ...

backend:
  subdomain: "api"  # Should result in api.example.com
  # ...

admin:
  subdomain: "admin"  # Should result in admin.example.com
  # ...

ingress:
  build_type: "Ingress"
  components:
    - frontend
    - backend
    - admin
  # ...
```

Expected nginx.conf output should have three server blocks:
- `server_name app.example.com` → routes to frontend
- `server_name api.example.com` → routes to backend
- `server_name admin.example.com` → routes to admin

## Impact Assessment

### Affected Components
1. **Ingress components** - Primary impact, will now correctly route based on subdomains
2. **Multi-domain deployments** - Will now work correctly
3. **Existing single-domain deployments** - Should continue to work (backward compatible)

### Risk Assessment
- **Low Risk:** The fix is localized to one method
- **High Impact:** Fixes a critical routing bug
- **Backward Compatible:** Components without subdomains will continue to use the default domain

## Implementation Steps

1. **Update `render_artifacts_for_component` method** in `orchestrator.rs`
   - Change line 703 to use `component_spec.domain`
   - Change line 706 to group by `component_spec.domain`

2. **Add unit tests** to verify domain calculation
   - Test in `rush-container/src/build/orchestrator_tests.rs`
   - Verify each component gets its correct domain

3. **Add integration test** with multi-subdomain configuration
   - Create test configuration with multiple subdomains
   - Verify rendered artifacts have correct domain routing

4. **Update documentation** to explain subdomain configuration
   - Add examples of multi-subdomain setups
   - Document the domain calculation behavior

## Fix Applied

The fix has been successfully applied to the codebase:

### Changes Made
**File:** `/rush/crates/rush-container/src/build/orchestrator.rs`
**Lines:** 703 and 706

1. **Line 703:** Changed from `spec.domain.clone()` to `component_spec.domain.clone()`
2. **Line 706:** Changed from `spec.domain.clone()` to `component_spec.domain.clone()` for grouping services

These changes ensure that each component's services are now correctly configured with their own subdomain-based domain.

## Conclusion

This bug has been successfully fixed. The system now correctly uses each component's calculated domain when creating service specifications for artifact rendering. Each component with a subdomain will now be properly routed to its own domain (e.g., `api.example.com`, `app.example.com`, etc.) instead of all being incorrectly grouped under the ingress component's domain.

The fix is minimal, localized, and backward compatible. Components without subdomains will continue to work as before, while multi-subdomain deployments will now function correctly.