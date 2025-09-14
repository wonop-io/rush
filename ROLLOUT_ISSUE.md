# Rollout Docker Push Issue Analysis

## Executive Summary

The rollout command fails when attempting to push Docker images for components that don't actually build container images, specifically `LocalService` type components. The error occurs because these components return empty strings as their "built image" names, which creates invalid Docker registry tags.

## Error Details

```
Docker error: Failed to tag image: Error parsing reference: "" is not a valid repository/tag: invalid reference format
```

This error occurs when trying to push the `database` component, which is defined as a `LocalService` type.

## Root Cause Analysis

### 1. Build Orchestrator Behavior

**Location**: `rush/crates/rush-container/src/build/orchestrator.rs:371-374`

```rust
BuildType::LocalService { .. } => {
    // Local services don't need container images
    debug!("Skipping build for local service {}", spec.component_name);
    Ok(String::new())  // ← Returns empty string
}
```

The build orchestrator correctly identifies that `LocalService` components don't need container images and returns an empty string. Similar behavior exists for:
- `PureKubernetes` - Returns empty string (line 378-379)
- `KubernetesInstallation` - Returns empty string (line 381-384)

### 2. Built Images Collection

**Location**: `rush/crates/rush-container/src/reactor/modular_core.rs:944`

```rust
self.built_images = built_images;
```

The reactor stores ALL build results, including empty strings for components that don't produce images.

### 3. Push Logic Flaw

**Location**: `rush/crates/rush-container/src/reactor/modular_core.rs:1115-1143`

```rust
// Push images to registry
for (component_name, image_name) in &self.built_images {
    // Get the full registry tag
    let registry_tag = self.get_registry_tag(image_name);  // ← Processes empty strings
    info!("Pushing image: {} -> {}", component_name, registry_tag);

    // Attempts to tag and push EVERY entry, including empty strings
    ...
}
```

The `build_and_push` method iterates through ALL entries in `built_images` without filtering out empty strings or checking if the component actually produces a pushable image.

### 4. Registry Tag Generation

**Location**: `rush/crates/rush-container/src/reactor/modular_core.rs:1090-1102`

When `image_name` is an empty string:
- With registry URL and namespace: `registry.wonop.dev/wonop/` (invalid)
- With just registry URL: `registry.wonop.dev/` (invalid)
- With just namespace: `wonop/` (invalid)
- With nothing: `` (empty, invalid)

## Affected Component Types

The following `BuildType` variants return empty strings and will cause push failures:

1. **LocalService** - Database services like PostgreSQL, Redis, etc.
2. **PureKubernetes** - Kubernetes-only resources without custom images
3. **KubernetesInstallation** - External Kubernetes installations

## Solution

The core issue is that `build_and_push` attempts to push ALL components, including those that don't produce Docker images. The solution should filter components based on their `BuildType`, not on empty strings or image names.

### Correct Architectural Approach

**Components should be filtered by their build type BEFORE attempting to build or push them.**

### Option 1: Filter by BuildType in build_and_push (Recommended)

**Location to fix**: `rush/crates/rush-container/src/reactor/modular_core.rs:1105-1147`

```rust
pub async fn build_and_push(&mut self) -> Result<()> {
    info!("Building and pushing Docker images...");

    // Filter components that actually produce pushable images
    let pushable_components: Vec<ComponentBuildSpec> = self.component_specs
        .iter()
        .filter(|spec| Self::produces_pushable_image(&spec.build_type))
        .cloned()
        .collect();

    if pushable_components.is_empty() {
        info!("No components with pushable images found");
        return Ok(());
    }

    // Build only pushable components
    let built_images = self.build_orchestrator.build_components(
        pushable_components.clone(),
        false, // force_rebuild
    ).await?;

    self.built_images = built_images;

    // Login to registry if needed
    self.docker_login().await?;

    // Push images to registry
    for (component_name, image_name) in &self.built_images {
        let registry_tag = self.get_registry_tag(image_name);
        info!("Pushing image: {} -> {}", component_name, registry_tag);

        // Tag and push logic...
    }

    info!("Build and push completed successfully");
    Ok(())
}

/// Determines if a build type produces a pushable Docker image
fn produces_pushable_image(build_type: &BuildType) -> bool {
    matches!(
        build_type,
        BuildType::RustBinary { .. } |
        BuildType::TrunkWasm { .. } |
        BuildType::Image { .. } |
        BuildType::Ingress { .. } |
        BuildType::PureDockerImage { .. }
    )
}
```

### Option 2: Separate build() and build_for_push()

Create separate methods for different build contexts:

```rust
/// Build all components (for local development)
pub async fn build(&mut self) -> Result<()> {
    // Builds everything including LocalServices
}

/// Build only components that produce pushable images (for deployment)
pub async fn build_for_deployment(&mut self) -> Result<()> {
    let deployable_components = self.component_specs
        .iter()
        .filter(|spec| Self::produces_pushable_image(&spec.build_type))
        .cloned()
        .collect();

    self.built_images = self.build_orchestrator.build_components(
        deployable_components,
        false,
    ).await?;

    Ok(())
}

pub async fn build_and_push(&mut self) -> Result<()> {
    info!("Building and pushing Docker images...");

    // Use deployment-specific build
    self.build_for_deployment().await?;

    // Login and push...
}
```

### Option 3: Add BuildType awareness to BuildOrchestrator

Modify the build orchestrator to accept a filter predicate:

```rust
pub async fn build_components_filtered<F>(
    &self,
    specs: Vec<ComponentBuildSpec>,
    force_rebuild: bool,
    filter: F,
) -> Result<HashMap<String, String>>
where
    F: Fn(&BuildType) -> bool,
{
    let filtered_specs: Vec<_> = specs
        .into_iter()
        .filter(|spec| filter(&spec.build_type))
        .collect();

    // Build only filtered components
    self.build_components(filtered_specs, force_rebuild).await
}
```

## Implementation Recommendation

**Use Option 1** - Filter by BuildType in `build_and_push`. This maintains clean separation of concerns:

1. The method explicitly filters for pushable components
2. It's self-documenting about what types of components are handled
3. It prevents LocalServices from being built unnecessarily for deployment
4. It maintains backward compatibility for local development

## Complete Fix

```rust
impl Reactor {
    pub async fn build_and_push(&mut self) -> Result<()> {
        info!("Building and pushing Docker images for deployment...");

        // Filter components that produce pushable images
        let pushable_components: Vec<ComponentBuildSpec> = self.component_specs
            .iter()
            .filter(|spec| Self::produces_pushable_image(&spec.build_type))
            .cloned()
            .collect();

        if pushable_components.is_empty() {
            info!("No components with pushable images found");
            return Ok(());
        }

        info!("Found {} components with pushable images", pushable_components.len());

        // Build only pushable components
        let built_images = self.build_orchestrator.build_components(
            pushable_components,
            false,
        ).await?;

        self.built_images = built_images;

        // Login to registry if needed
        self.docker_login().await?;

        // Push images to registry
        for (component_name, image_name) in &self.built_images {
            let registry_tag = self.get_registry_tag(image_name);
            info!("Pushing image: {} -> {}", component_name, registry_tag);

            // Tag the image for the registry if needed
            if registry_tag != *image_name {
                let tag_output = tokio::process::Command::new("docker")
                    .args(&["tag", image_name, &registry_tag])
                    .output()
                    .await
                    .map_err(|e| Error::Docker(format!("Failed to tag image: {}", e)))?;

                if !tag_output.status.success() {
                    let stderr = String::from_utf8_lossy(&tag_output.stderr);
                    return Err(Error::Docker(format!("Failed to tag image: {}", stderr)));
                }
            }

            // Use the Docker client to push the image
            if let Err(e) = self.docker_integration.client().push_image(&registry_tag).await {
                error!("Failed to push image {} for component {}: {}",
                       registry_tag, component_name, e);
                return Err(e);
            }

            info!("Successfully pushed image: {}", registry_tag);
        }

        info!("Build and push completed successfully");
        Ok(())
    }

    /// Determines if a build type produces a pushable Docker image
    fn produces_pushable_image(build_type: &BuildType) -> bool {
        matches!(
            build_type,
            BuildType::RustBinary { .. } |
            BuildType::TrunkWasm { .. } |
            BuildType::Image { .. } |
            BuildType::Ingress { .. } |
            BuildType::PureDockerImage { .. }
        )
    }
}

## Testing

After implementing the fix, test with:

```bash
# Should complete without errors
./rush/target/release/rush --env staging io.wonop.helloworld rollout

# Verify only actual images are pushed:
# - frontend (TrunkWasm)
# - backend (RustBinary)
# - ingress (Ingress)
#
# Should skip:
# - database (LocalService)
# - stripe (LocalService)
```

## Prevention

To prevent similar issues in the future:

1. **Add unit tests** for `build_and_push` with mixed component types
2. **Add integration tests** that include LocalService components
3. **Consider adding a `has_image()` method to ComponentBuildSpec**
4. **Document** which BuildTypes produce pushable images

## Impact

- **Current**: Rollout fails completely when any LocalService is present
- **After Fix**: Rollout will correctly skip non-image components and only push actual Docker images

## Related Code Paths

1. Build orchestrator: `rush/crates/rush-container/src/build/orchestrator.rs:366-385`
2. Build and push: `rush/crates/rush-container/src/reactor/modular_core.rs:1105-1147`
3. Registry tagging: `rush/crates/rush-container/src/reactor/modular_core.rs:1089-1102`
4. Component specs: `products/io.wonop.helloworld/stack.spec.yaml`