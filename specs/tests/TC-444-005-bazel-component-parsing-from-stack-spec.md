---
id: TC-444-005
type: test-case
progress: backlog
parents:
- REQ-444-005
priority: high
created: 2026-02-27T11:10:16.977131Z
updated: 2026-02-27T11:10:16.977131Z
author: agent
---

# Bazel component parsing from stack.spec.yaml

## Test Objective
Verify that Bazel components are correctly parsed from `stack.spec.yaml`.

## Test Cases

### TC-1: Parse minimal Bazel component
**Given:** YAML with `build_type: "Bazel"` and `location: "demo-bazel"`
**When:** `ComponentBuildSpec::from_yaml()` is called
**Then:** Creates spec with `BuildType::Bazel` and correct location

```yaml
demo-bazel:
  build_type: "Bazel"
  location: "demo-bazel"
  color: "cyan"
```

### TC-2: Parse Bazel component with all options
**Given:** YAML with all Bazel options specified
**When:** Parsed
**Then:** All fields are correctly populated

```yaml
demo-bazel:
  build_type: "Bazel"
  location: "demo-bazel"
  output_dir: "./custom-output"
  targets:
    - "//src:app"
    - "//src:lib"
  additional_args:
    - "--jobs=8"
  base_image: "gcr.io/distroless/static"
  port: 8080
  k8s: demo-bazel/infrastructure
```

### TC-3: Default output_dir when not specified
**Given:** Bazel component without `output_dir` field
**When:** Parsed
**Then:** `output_dir` defaults to `"target/bazel-out"`

### TC-4: Error on missing location
**Given:** Bazel component without `location` field
**When:** Parsing attempted
**Then:** Panics with "location is required for Bazel" message

### TC-5: Parse targets as vector
**Given:** YAML with `targets` as a list
**When:** Parsed
**Then:** `targets` is `Some(Vec<String>)` with correct values
