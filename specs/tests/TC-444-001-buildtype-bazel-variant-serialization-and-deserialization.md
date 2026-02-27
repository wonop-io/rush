---
id: TC-444-001
type: test-case
progress: backlog
parents:
- REQ-444-001
priority: high
created: 2026-02-27T11:09:35.654157Z
updated: 2026-02-27T11:09:35.654157Z
author: agent
---

# BuildType::Bazel variant serialization and deserialization

## Test Objective
Verify that the `BuildType::Bazel` variant correctly serializes and deserializes with all fields.

## Test Cases

### TC-1: Serialize Bazel variant with all fields
**Given:** A `BuildType::Bazel` with all fields populated
**When:** Serialized to YAML/JSON
**Then:** All fields are correctly represented in the output

```rust
#[test]
fn test_bazel_build_type_serialization() {
    let build_type = BuildType::Bazel {
        location: "demo-bazel".to_string(),
        output_dir: "./bazel-out".to_string(),
        context_dir: Some(".".to_string()),
        targets: Some(vec!["//src:app".to_string()]),
        additional_args: Some(vec!["--jobs=8".to_string()]),
        base_image: Some("gcr.io/distroless/static".to_string()),
    };
    
    let yaml = serde_yaml::to_string(&build_type).unwrap();
    assert!(yaml.contains("location: demo-bazel"));
    assert!(yaml.contains("output_dir: ./bazel-out"));
}
```

### TC-2: Deserialize Bazel variant from YAML
**Given:** Valid YAML with Bazel build type configuration
**When:** Deserialized to `BuildType`
**Then:** Creates correct `BuildType::Bazel` variant

### TC-3: location() method returns workspace path
**Given:** A `BuildType::Bazel` instance
**When:** Calling `location()` method
**Then:** Returns `Some(location)` with the workspace path

### TC-4: requires_docker_build() returns true
**Given:** A `BuildType::Bazel` instance
**When:** Calling `requires_docker_build()` method
**Then:** Returns `true`
