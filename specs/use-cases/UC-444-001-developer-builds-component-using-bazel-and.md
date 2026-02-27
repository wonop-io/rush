---
id: UC-444-001
type: use-case
progress: backlog
parents: []
priority: high
created: 2026-02-27T10:50:38.896407Z
updated: 2026-02-27T10:50:38.896407Z
author: agent
---

# Developer builds component using Bazel and deploys as OCI image

## Primary Actor
Developer using Rush to build and deploy applications

## Preconditions
- Rush is installed and configured
- Bazel is installed on the system
- A valid `stack.spec.yaml` file exists with a Bazel build type component
- The rushd.yaml configuration file is present with optional Bazel output directory setting

## Main Success Scenario

1. Developer defines a component in `stack.spec.yaml` with `build_type: "Bazel"`
2. Developer specifies Bazel-specific configuration:
   - `location`: Path to the Bazel workspace
   - `targets`: Optional list of Bazel targets to build (defaults to `//...`)
   - `output_dir`: Output directory for build artifacts (can be relative or absolute)
3. Developer optionally configures global Bazel output directory in `rushd.yaml`:
   - `bazel.output_dir`: Persistent output directory for all Bazel builds
4. Developer runs `rush dev` or `rush build`
5. Rush detects the Bazel build type and invokes the Bazel builder
6. Bazel builder:
   - Resolves the output directory (component-level overrides global setting)
   - Creates the output directory if it doesn't exist
   - Executes `bazel build` with the specified targets
   - Generates an OCI image from the build outputs
7. Rush loads the OCI image into Docker for local development or pushes to registry for deployment
8. Developer sees the component running with the Bazel-built artifacts

## Alternative Flows

### 3a. No output directory specified
1. Rush uses a default output directory relative to the product directory (e.g., `target/bazel-out`)

### 5a. Bazel build fails
1. Rush displays the Bazel error output to the developer
2. Rush marks the component as failed
3. Developer fixes the issue and re-runs the build

### 6a. OCI image generation fails
1. Rush displays an error indicating the image generation failure
2. Developer checks the build outputs and Dockerfile configuration

## Postconditions
- Bazel build artifacts are created in the specified output directory
- An OCI image is created and available for Rush to use
- The component can be deployed alongside other Rush components

## Business Rules
- The output directory path can be absolute or relative to the product directory
- Bazel output directory in rushd.yaml provides a global default that can be overridden per-component
- Build caching should be leveraged via Bazel's native caching mechanisms
