---
id: UC-444-002
type: use-case
progress: backlog
parents: []
priority: high
created: 2026-02-27T10:50:50.916803Z
updated: 2026-02-27T10:50:50.916803Z
author: agent
---

# Developer configures persistent Bazel output directory via rushd.yaml

## Primary Actor
Developer or DevOps engineer configuring Rush projects

## Preconditions
- Rush is installed
- A valid `rushd.yaml` file exists in the project root

## Main Success Scenario

1. Developer opens `rushd.yaml` configuration file
2. Developer adds or modifies the `bazel` section:
   ```yaml
   bazel:
     output_dir: "/path/to/bazel/outputs"  # absolute path
     # or
     output_dir: ".bazel-out"  # relative to project root
   ```
3. Developer saves the configuration
4. Developer runs any Rush command that involves Bazel builds
5. Rush reads the `rushd.yaml` configuration
6. Rush uses the configured output directory for all Bazel build artifacts
7. Build artifacts persist across Rush sessions in the specified location

## Alternative Flows

### 2a. Using relative path
1. Developer specifies a relative path (e.g., `output_dir: "build/bazel"`)
2. Rush resolves the path relative to the project root directory
3. Rush creates the directory structure if it doesn't exist

### 2b. Component-level override
1. Developer also specifies `output_dir` in the component's `stack.spec.yaml`
2. The component-level setting takes precedence over the global `rushd.yaml` setting

### 5a. Invalid path specified
1. Rush validates the path configuration
2. If the path is invalid or inaccessible, Rush displays a clear error message
3. Developer corrects the path configuration

## Postconditions
- The Bazel output directory configuration is persisted in `rushd.yaml`
- All subsequent Bazel builds use the configured output directory
- Build artifacts are organized and persistent across sessions

## Business Rules
- Absolute paths are used as-is
- Relative paths are resolved relative to the project root (where `rushd.yaml` is located)
- Component-level `output_dir` overrides the global setting from `rushd.yaml`
- The configuration must be valid YAML format
- Rush must have write permissions to the specified directory
