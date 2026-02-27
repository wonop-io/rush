---
id: REQ-444-001
type: requirement
progress: backlog
parents:
- UC-444-001
priority: high
created: 2026-02-27T10:51:00.252876Z
updated: 2026-02-27T10:51:00.252876Z
author: agent
---

# Bazel BuildType variant in rush-build crate

## Description
Add a new `Bazel` variant to the `BuildType` enum in `rush-build/src/build_type.rs` that supports Bazel-based builds with OCI image generation.

## Specification

### BuildType::Bazel variant
```rust
Bazel {
    /// Path to the Bazel workspace directory
    location: String,
    /// Output directory for build artifacts (relative or absolute)
    output_dir: String,
    /// Optional context directory for resolving relative paths
    context_dir: Option<String>,
    /// Optional list of Bazel targets to build (defaults to "//...")
    targets: Option<Vec<String>>,
    /// Optional additional Bazel arguments
    additional_args: Option<Vec<String>>,
    /// Optional OCI base image (defaults to "scratch")
    base_image: Option<String>,
}
```

### Implementation Requirements
1. The `location()` method must return `Some(location)` for the Bazel variant
2. The `requires_docker_build()` method must return `true` for Bazel builds
3. The `dockerfile_path()` method must return `None` (OCI images are generated programmatically)

## Acceptance Criteria
- [ ] `BuildType::Bazel` variant is defined with all required fields
- [ ] Serde serialization/deserialization works correctly for the new variant
- [ ] The `location()` method returns the Bazel workspace path
- [ ] Unit tests verify the new variant behaves correctly
