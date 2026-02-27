---
id: REQ-444-005
type: requirement
progress: backlog
parents:
- UC-444-001
priority: high
created: 2026-02-27T10:51:53.573090Z
updated: 2026-02-27T10:51:53.573090Z
author: agent
---

# Bazel build type parsing in stack.spec.yaml

## Description
Extend the YAML parsing in `ComponentBuildSpec::from_yaml()` to support the new Bazel build type with all its configuration options.

## Specification

### stack.spec.yaml Format
```yaml
demo-bazel:
  build_type: "Bazel"
  location: "demo-bazel"
  color: "cyan"
  
  # Optional: Specific targets to build (default: all)
  targets:
    - "//src:app"
    - "//src:lib"
  
  # Optional: Output directory (overrides rushd.yaml setting)
  output_dir: "./bazel-artifacts"
  
  # Optional: Additional Bazel arguments
  additional_args:
    - "--jobs=8"
    - "--disk_cache=/tmp/bazel-cache"
  
  # Optional: Base image for OCI generation
  base_image: "gcr.io/distroless/static"
  
  # Optional: Entry point for the container
  entrypoint: "/app/main"
  
  # Optional: Working directory in container
  workdir: "/app"
  
  # Standard Rush component options
  k8s: demo-bazel/infrastructure
  port: 8080
  target_port: 8080
  depends_on:
    - database
```

### Implementation in spec.rs
Add parsing logic in `ComponentBuildSpec::from_yaml()`:

```rust
"Bazel" => BuildType::Bazel {
    location: yaml_section
        .get("location")
        .expect("location is required for Bazel")
        .as_str()
        .unwrap()
        .to_string(),
    output_dir: yaml_section
        .get("output_dir")
        .map(|v| v.as_str().unwrap().to_string())
        .unwrap_or_else(|| "target/bazel-out".to_string()),
    context_dir: yaml_section
        .get("context_dir")
        .map(|v| v.as_str().unwrap().to_string()),
    targets: yaml_section.get("targets").map(|v| {
        v.as_sequence()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap().to_string())
            .collect()
    }),
    additional_args: yaml_section.get("additional_args").map(|v| {
        v.as_sequence()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap().to_string())
            .collect()
    }),
    base_image: yaml_section
        .get("base_image")
        .map(|v| v.as_str().unwrap().to_string()),
},
```

## Acceptance Criteria
- [ ] `"Bazel"` build type is recognized in stack.spec.yaml
- [ ] All Bazel-specific fields are correctly parsed
- [ ] Default values are applied for optional fields
- [ ] Error messages are clear when required fields are missing
- [ ] Integration test verifies parsing of a complete Bazel component
