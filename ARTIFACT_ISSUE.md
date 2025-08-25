# Nginx Configuration Domain Generation Issue

## Problem Description
When rendering the nginx.conf for the ingress component, the domains are being generated incorrectly as `{product_name}.local` (e.g., `helloworld.wonop.io.local`) instead of using the proper domain templates defined in `rushd.yaml` based on the environment.

## Root Cause Analysis

### Location of Issue
File: `/Users/tfr/Documents/Projects/rush/rush/crates/rush-container/src/reactor/core.rs`
Function: `render_artifacts_for_component`
Line: 2166

### Code Analysis
```rust
// TODO: This is outright wrong - it needs to be properly computed from the rushd.yaml template (depends on environment)
let domain = format!("{}.local", spec.product_name);
```

The domain is being hardcoded in the artifact rendering phase, completely ignoring:
1. The environment-specific domain templates from rushd.yaml
2. The correctly computed domain already stored in the ComponentBuildSpec

## Domain Template Configuration

The rushd.yaml defines environment-specific domain templates:
```yaml
LOCAL_DOMAIN: "{%-if subdomain-%}{{ subdomain }}.{%-endif-%}localhost"
DEV_DOMAIN: "{%-if subdomain-%}{{ subdomain }}-{%-endif-%}{{ product_uri }}-dev.wonop.dev"
STAGING_DOMAIN: "{%-if subdomain-%}{{ subdomain }}-{%-endif-%}{{ product_uri }}-staging.wonop.dev"
PROD_DOMAIN: "{%-if subdomain-%}{{ subdomain }}.{%-endif-%}{{ product_name }}"
```

## Correct Domain Computation Flow

1. **ComponentBuildSpec Creation** (`rush-build/src/spec.rs:from_yaml`)
   - Correctly computes domain using: `config.domain(subdomain)`
   - Stores it in `ComponentBuildSpec.domain` field

2. **Config Domain Method** (`rush-config/src/types.rs`)
   - Uses the domain_template from rushd.yaml
   - Renders it with Tera template engine
   - Properly handles subdomain and product variables

3. **Problem in Artifact Rendering** (`rush-container/src/reactor/core.rs`)
   - Ignores the pre-computed domain from ComponentBuildSpec
   - Hardcodes a `.local` suffix regardless of environment

## Impact

This issue causes:
- Incorrect server_name directives in nginx.conf
- Routing failures in non-local environments
- Mismatch between expected and actual domain names

## Solution

The fix is straightforward - use the already computed domain from the ComponentBuildSpec:

```rust
// Instead of:
let domain = format!("{}.local", spec.product_name);

// Use:
let domain = spec.domain.clone();
```

Additionally, for Ingress components that aggregate multiple services, each component's domain should be retrieved from its respective ComponentBuildSpec rather than creating a single hardcoded domain for all services.

## Verification

After fixing, the rendered nginx.conf should show:
- **Local**: `localhost` or `{subdomain}.localhost`
- **Dev**: `{product_uri}-dev.wonop.dev` or `{subdomain}-{product_uri}-dev.wonop.dev`
- **Staging**: `{product_uri}-staging.wonop.dev` or `{subdomain}-{product_uri}-staging.wonop.dev`
- **Prod**: `{product_name}` or `{subdomain}.{product_name}`

Instead of the current incorrect format: `{product_name}.local`