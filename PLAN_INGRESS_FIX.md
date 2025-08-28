# Ingress Build Process Fix Plan

## Current Problems

### 1. Incorrect Docker Context Path
- **Issue**: The ingress has `context_dir: ../target` in stack.spec.yaml (line 51)
- **Result**: Docker context becomes `/Users/tfr/Documents/Projects/rush/products/io.wonop.helloworld/../target`
- **Problem**: This points to `/Users/tfr/Documents/Projects/rush/products/target/` which is outside the product directory
- **Log Evidence**: `Rendered artifact to Docker context: /Users/tfr/Documents/Projects/rush/products/io.wonop.helloworld/../target/nginx.conf`

### 2. Missing Docker Build Output
- **Issue**: The `DockerExecutor::build_image` method doesn't capture/display build output
- **Problem**: Can't debug what's happening during Docker builds
- **Location**: `/rush/crates/rush-docker/src/client.rs:144-158`

### 3. Artifact Rendering Location Issues
- **Current Flow**:
  1. `prepare_artifacts()` creates `.rush/artifacts/{component}` directory (line 348-351)
  2. Docker context is determined (lines 278-297)
  3. Artifacts are rendered to docker_context (line 718: `docker_context.join(output_path)`)
- **Problem**: When context_dir is `../target`, artifacts get written outside product directory

## Root Cause Analysis

The fundamental issue is that the ingress component is configured with an incorrect `context_dir: ../target` which causes:
1. Docker build context to be outside the product directory
2. Rendered artifacts (nginx.conf) to be placed in the wrong location
3. The Dockerfile expects `./rushd/nginx.conf` relative to its context

## Proposed Solution

### Step 1: Fix the stack.spec.yaml Configuration
Remove the problematic `context_dir: ../target` from ingress configuration. The context should default to the directory containing the Dockerfile (ingress/).

```yaml
ingress:
  build_type: "Ingress"
  port: 9000
  target_port: 80
  location: "./ingress"
  # REMOVE: context_dir: ../target  
  dockerfile: "./ingress/Dockerfile"
  artefacts:
    "./ingress/nginx.conf": "rushd/nginx.conf"  # Note: target path should include rushd/
```

### Step 2: Fix Artifact Rendering Path
The artifact map should specify the full target path including the `rushd/` directory:
- Source: `./ingress/nginx.conf` (template)
- Target: `rushd/nginx.conf` (where Dockerfile expects it)

### Step 3: Add Docker Build Output
Modify `DockerExecutor::build_image` to capture and display build output for debugging:

```rust
async fn build_image(&self, tag: &str, dockerfile: &str, context: &str) -> Result<()> {
    let mut args = vec!["build".to_string()];
    
    args.push("--tag".to_string());
    args.push(tag.to_string());
    
    args.push("--file".to_string());
    args.push(dockerfile.to_string());
    
    args.push(context.to_string());
    
    info!("Docker build command: docker {}", args.join(" "));
    info!("Building from context: {}", context);
    
    let output = self.execute_with_output(args).await?;
    
    // Log the build output for debugging
    if !output.stdout.is_empty() {
        debug!("Docker build output:\n{}", output.stdout);
    }
    if !output.stderr.is_empty() && !output.status.success() {
        error!("Docker build errors:\n{}", output.stderr);
    }
    
    info!("Built Docker image: {}", tag);
    Ok(())
}
```

### Step 4: Ensure Correct Artifact Flow
The correct flow should be:
1. Template: `products/io.wonop.helloworld/ingress/nginx.conf`
2. Render to: `products/io.wonop.helloworld/ingress/rushd/nginx.conf`
3. Docker context: `products/io.wonop.helloworld/ingress/`
4. Dockerfile COPY: `./rushd/nginx.conf` → `/etc/nginx/nginx.conf`

### Step 5: Clean Up Artifact Rendering
The `render_artifacts_for_component` method should:
1. Write to `.rush/artifacts/{component}/` for tracking (keep as-is)
2. Write to `docker_context.join(output_path)` for Docker build
3. Remove the legacy `target/rushd/` path (lines 723-731)

## Implementation Order

1. **Fix stack.spec.yaml** - Remove `context_dir: ../target` and fix artifact paths
2. **Update Docker client** - Add build output logging
3. **Test the fix** - Verify nginx.conf is correctly rendered and included in image
4. **Clean up** - Remove any hardcoded paths or legacy code

## Expected Outcome

After these fixes:
- Docker context will be `products/io.wonop.helloworld/ingress/`
- Artifacts will be rendered to `products/io.wonop.helloworld/ingress/rushd/nginx.conf`
- Docker build output will be visible for debugging
- The ingress image will contain the correctly rendered nginx.conf

## Testing Plan

1. Remove old ingress images: `docker rmi $(docker images helloworld.wonop.io/ingress -q)`
2. Clean build artifacts: `rm -rf products/io.wonop.helloworld/ingress/rushd`
3. Run build: `./target/release/rush helloworld.wonop.io build`
4. Verify rendered artifact: `cat products/io.wonop.helloworld/ingress/rushd/nginx.conf`
5. Check Docker image: `docker run --rm helloworld.wonop.io/ingress:latest cat /etc/nginx/nginx.conf`