# Ingress nginx.conf Service Rendering Investigation Report

## Resolution Status: ✅ RESOLVED
- **Issue**: nginx.conf artifacts appeared not to be rendering
- **Root Cause**: Docker image caching was preventing re-rendering of artifacts
- **Solution**: Implemented `--force-rebuild` flag for the build command
- **Status**: Working correctly with force rebuild option

## Summary
After thorough investigation, the ingress nginx.conf **IS rendering services correctly**. The rendered nginx.conf at `/Users/tfr/Documents/Projects/rush/products/io.wonop.helloworld/dist/nginx.conf` properly includes both backend and frontend services with their correct mount points and container names.

### Important Discovery: Product Name Format
Rush accepts product names in both formats:
- **Domain format**: `helloworld.wonop.io`
- **Reversed format**: `io.wonop.helloworld`

Both resolve to the same product directory (`products/io.wonop.helloworld/`), BUT they create different Docker image names:
- `helloworld.wonop.io` → Images: `helloworld.wonop.io/ingress:tag`
- `io.wonop.helloworld` → Images: `io.wonop.helloworld/ingress:tag`

This means the build cache treats them as separate images, so switching between the two formats will trigger a rebuild (and artifact re-rendering).

## Current Behavior (Working Correctly)
The rendered nginx.conf shows:
- Backend service at `/api` routing to `http://io.wonop.helloworld-backend:8000`
- Frontend service at `/` routing to `http://io.wonop.helloworld-frontend:80`

## Investigation Findings

### 1. Template Processing
The nginx.conf template uses Tera templating with the following structure:
```nginx
{% for domain, service_list in services %}
server {
    server_name {{ domain }};
    {% for service in service_list %}
    location {{ service.mount_point }} {
        proxy_pass http://{{service.host}}:{{ service.target_port }};
    }
    {% endfor %}
}
{% endfor %}
```

### 2. Service Population Logic
In `build/orchestrator.rs:render_artifacts_for_component()`:
- For Ingress build types, it iterates through the `components` list
- For each component name, it searches `all_specs` to find the corresponding `ComponentBuildSpec`
- It extracts port information and creates `ServiceSpec` entries
- Services are grouped by domain

### 3. Potential Issues with LocalServices

While the current rendering works, there's a **potential issue** that could arise:

#### The Problem
When we filter out LocalServices from component_specs before building (as we do in the dependency graph), those specs might not be available when the ingress tries to render artifacts that reference LocalServices.

#### Current State
- LocalServices (database, stripe) are correctly excluded from Docker builds
- The ingress only references Docker-managed services (backend, frontend)
- This works correctly

#### Future Risk
If someone tries to add a LocalService to the ingress components list:
```yaml
ingress:
  components:
    - backend
    - frontend
    - database  # This would fail to render!
```

The `database` LocalService wouldn't be found in the filtered specs, causing the nginx.conf to miss that service entry.

### 4. Code Flow
1. `ModularReactor::manual_rebuild()` calls `build_orchestrator.build_components(component_specs)`
2. `BuildOrchestrator::build_components()` passes all specs to each component build
3. `render_artifacts_for_component()` uses these specs to look up component information
4. For ingress, it builds a services map from the referenced components

## Why It Currently Works
1. The ingress only references actual Docker containers (backend, frontend)
2. LocalServices are not typically routed through the ingress
3. The component specs passed to build still include all Docker-managed components

## Recommendations

### 1. Document the Limitation
LocalServices should not be added to ingress component lists as they won't be resolved correctly.

### 2. Add Validation
Add a check in the ingress build to warn if a referenced component is a LocalService:
```rust
if matches!(component_spec.build_type, BuildType::LocalService { .. }) {
    warn!("Component {} is a LocalService and cannot be proxied through ingress", component_name);
    continue;
}
```

### 3. Consider Alternative Handling
If LocalServices need to be accessible through ingress, they should be referenced by their container names directly in a custom nginx.conf rather than through the component system.

## How to Debug Artifact Rendering

### Viewing Rendered Artifacts
When Rush builds a component with artifacts (like the ingress nginx.conf), the rendered files are stored in multiple locations:

#### 1. Component Distribution Directory
**Location:** `products/<product-name>/dist/<artifact-name>`
- Example: `/Users/tfr/Documents/Projects/rush/products/io.wonop.helloworld/dist/nginx.conf`
- This is the primary rendered output used during Docker builds

#### 2. Rush Artifacts Cache
**Location:** `products/<product-name>/.rush/artifacts/<component-name>/<artifact-name>`
- Example: `/Users/tfr/Documents/Projects/rush/products/io.wonop.helloworld/.rush/artifacts/ingress/nginx.conf`
- Used for caching and change detection

#### 3. Inside the Docker Context (during build)
**Location:** `<docker-context>/<artifact-name>`
- Temporary location during the actual Docker build process

### Methods to Force Artifact Re-rendering

#### Using the Force Rebuild Flag (Recommended)
Rush now supports a `--force-rebuild` flag for the build command that bypasses Docker image cache and forces re-rendering of all artifacts:

```bash
# Force rebuild all components and re-render all artifacts
cargo run -- io.wonop.helloworld build --force-rebuild

# Or with the compiled binary
./target/release/rush io.wonop.helloworld build --force-rebuild
```

This flag:
- Bypasses Docker image cache checking
- Forces re-rendering of all component artifacts
- Rebuilds all Docker images even if they exist
- Useful for debugging artifact generation issues

#### Alternative Methods

##### Method 1: Regular Build Command (Uses Cache)
rush <product-name> build

# With debug logging to see rendering details
RUST_LOG=debug rush <product-name> build 2>&1 | grep -E "render|artifact"
```

**Important Note:** Rush uses caching to skip unnecessary builds. If the Docker image already exists with the current hash, the build (and artifact rendering) will be skipped. To force artifact rendering:

1. **Delete the existing image:**
   ```bash
   # Find the image name from build logs
   docker images | grep <component-name>
   docker rmi <image-name:tag>

   # Example:
   docker rmi io.wonop.helloworld/ingress:7d584529-wip-83f1a9eb
   ```

2. **Modify a source file** in the component directory to change the hash

3. **Run build again:**
   ```bash
   rush <product-name> build
   ```

When the build runs, you'll see:
```
Rendering 1 artifacts for ingress
Rendered artifact to component dist: /path/to/product/dist/nginx.conf
```

#### Method 2: Dev Command (Renders and Starts)
```bash
# Start development environment - renders artifacts during startup
rush <product-name> dev

# Check the dist directory after startup
ls -la products/<product-name>/dist/
```

#### Method 3: Manual Inspection After Any Build
```bash
# After any build/dev command, inspect rendered artifacts
cat products/<product-name>/dist/nginx.conf

# Compare with template
diff products/<product-name>/ingress/nginx.conf products/<product-name>/dist/nginx.conf
```

### Understanding the Rendering Process

The artifact rendering happens in `BuildOrchestrator::render_artifacts_for_component()`:

1. **Template Loading**: Reads the template file (e.g., `ingress/nginx.conf`)
2. **Context Building**: Creates a BuildContext with:
   - Services map (grouped by domain)
   - Environment variables
   - Component metadata
3. **Tera Rendering**: Processes the template with the context
4. **Output Writing**: Saves to both:
   - `.rush/artifacts/` (cache)
   - `dist/` (distribution)

### Debugging Tips

#### 1. Enable Debug Logging
```bash
RUST_LOG=debug rush <product-name> build 2>&1 | tee build.log
grep -E "Render|artifact|services_map" build.log
```

#### 2. Check What Services Are Available
Look for lines like:
```
[BUILD DECISION] Component 'ingress': Rendering 1 artifacts
Component backend referenced by ingress found with ports 8130:8000
Component frontend referenced by ingress found with ports 8129:80
```

#### 3. Verify Template Variables
The key variables available in nginx.conf templates:
- `services`: HashMap of domain → Vec<ServiceSpec>
- `domain`: Component domain
- `product_name`: Product identifier
- `component`: Component name
- `environment`: Current environment (local, dev, prod)

### Feature Request: Render-Only Command

A useful enhancement would be adding a dedicated command:
```bash
# Proposed command (not yet implemented)
rush <product-name> render <component-name>
# or
rush <product-name> render --all
```

This would:
1. Load component specifications
2. Build the rendering context
3. Process all artifact templates
4. Save to dist/ directory
5. Display rendered file locations

For now, the build command with debug logging provides the best visibility into the rendering process.

## Conclusion
The nginx.conf rendering is working correctly for the current use case. The perceived issue may have been due to:
1. Looking at the template file instead of the rendered output
2. Confusion about how LocalServices are handled
3. The port conflict issues that prevented the ingress from starting (unrelated to rendering)

The system correctly:
- ✅ Renders services that are Docker containers
- ✅ Excludes LocalServices from the build process
- ✅ Generates proper proxy configurations
- ✅ Uses correct container names and ports

No immediate fix is required, but the recommendations above would improve robustness for edge cases.