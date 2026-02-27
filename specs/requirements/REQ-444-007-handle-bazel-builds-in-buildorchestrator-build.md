---
id: REQ-444-007
type: requirement
progress: backlog
parents:
- UC-444-001
priority: high
created: 2026-02-27T11:04:43.024593Z
updated: 2026-02-27T11:04:43.024593Z
author: agent
---

# Handle Bazel builds in BuildOrchestrator::build_single

## Description
Add a case for `BuildType::Bazel` in the `BuildOrchestrator::build_single` method to handle Bazel component builds and produce Docker images that can be started like other components.

## Current Gap
Looking at `rush-container/src/build/orchestrator.rs:545-715`, the `build_single` method handles these build types:
- `RustBinary`, `TrunkWasm`, `DixiousWasm`, `Script`, `Zola`, `Book`, `Ingress` → Dockerfile-based builds
- `PureDockerImage` → Uses pre-built images
- `LocalService` → Skips (managed separately)
- `PureKubernetes` → Skips (no container needed)

**Missing:** `BuildType::Bazel` is not handled, so Bazel components are silently ignored.

## Specification

Add new match arm in `build_single`:

```rust
BuildType::Bazel {
    location,
    output_dir,
    targets,
    additional_args,
    base_image,
    ..
} => {
    info!("Building Bazel component: {}", spec.component_name);
    
    // 1. Resolve paths
    let workspace_path = self.config.product_dir.join(location);
    let output_path = resolve_output_dir(output_dir, &workspace_path);
    
    // 2. Execute Bazel build
    self.run_bazel_build(&workspace_path, targets.as_deref(), additional_args.as_deref()).await?;
    
    // 3. Generate Dockerfile from build outputs
    let dockerfile_path = self.generate_bazel_dockerfile(&output_path, base_image.as_deref()).await?;
    
    // 4. Build Docker image
    self.docker_client
        .build_image(
            &full_image_name,
            &dockerfile_path.to_string_lossy(),
            &output_path.to_string_lossy(),
        )
        .await?;
    
    info!(
        "Built Bazel component {} in {:?}",
        spec.component_name,
        start_time.elapsed()
    );
    
    Ok(full_image_name)
}
```

### Helper Methods to Add

```rust
/// Execute Bazel build command
async fn run_bazel_build(
    &self,
    workspace_path: &Path,
    targets: Option<&[String]>,
    additional_args: Option<&[String]>,
) -> Result<()>;

/// Generate a Dockerfile for Bazel build outputs
async fn generate_bazel_dockerfile(
    &self,
    output_path: &Path,
    base_image: Option<&str>,
) -> Result<PathBuf>;
```

## Integration Points

The built image will be:
1. Added to `built_images` HashMap
2. Used by `SimpleLifecycleManager::start_service` to run the container
3. Tagged and pushed like other images

## Acceptance Criteria
- [ ] `BuildType::Bazel` case added to `build_single` match
- [ ] Bazel build executes with correct targets and arguments
- [ ] Dockerfile is generated dynamically from build outputs
- [ ] Docker image is built and tagged correctly
- [ ] Image appears in logs: `[SYSTEM] rush_container | Starting container helloworld.wonop.io-demo-bazel`
- [ ] Container starts and runs successfully
