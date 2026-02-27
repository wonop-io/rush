---
id: REQ-444-003
type: requirement
progress: backlog
parents:
- UC-444-001
priority: high
created: 2026-02-27T10:51:25.495671Z
updated: 2026-02-27T10:51:25.495671Z
author: agent
---

# OCI Image generation from Bazel outputs

## Description
Implement OCI image generation functionality that creates Docker-compatible images from Bazel build outputs without requiring a traditional Dockerfile.

## Specification

### OCI Image Generator
Create a module `rush-build/src/oci_generator.rs` or integrate into the Bazel builder.

### Functionality
1. **Manifest Creation**: Generate OCI image manifest (JSON)
2. **Layer Creation**: Package Bazel build outputs as image layers
3. **Config Generation**: Create image configuration with:
   - Working directory
   - Entry point (from Bazel build outputs)
   - Environment variables
4. **Tarball Generation**: Create OCI image tarball that can be loaded into Docker

### Implementation Options

#### Option A: Use `docker buildx` with generated Dockerfile
```rust
fn generate_oci_image(&self, output_dir: &Path, context: &BuildContext) -> Result<()> {
    // 1. Create a minimal Dockerfile in the output directory
    // 2. Copy Bazel outputs to a staging area
    // 3. Use docker buildx to create the image
}
```

#### Option B: Use `crane` or `oras` CLI tools
```rust
fn generate_oci_image(&self, output_dir: &Path, context: &BuildContext) -> Result<()> {
    // 1. Use crane to create an image from directory
    // 2. Push to local Docker daemon
}
```

#### Option C: Native Rust OCI implementation (using `oci-spec` crate)
```rust
fn generate_oci_image(&self, output_dir: &Path, context: &BuildContext) -> Result<()> {
    // 1. Create OCI image layout
    // 2. Generate layers and manifests
    // 3. Load into Docker using docker load
}
```

### Recommended Approach
Start with **Option A** (generate Dockerfile) for simplicity and compatibility with existing Rush infrastructure. This integrates well with the existing `ImageBuilder` and Docker client infrastructure.

## Acceptance Criteria
- [ ] OCI images are generated from Bazel build outputs
- [ ] Images can be loaded into Docker (`docker load`)
- [ ] Images are compatible with Kubernetes deployment
- [ ] Image naming follows Rush conventions (`product-component:tag`)
- [ ] Base image is configurable (default: `scratch`)
- [ ] Entry point is configurable
