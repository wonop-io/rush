---
id: TC-444-003
type: test-case
progress: backlog
parents:
- REQ-444-003
priority: high
created: 2026-02-27T11:09:53.972026Z
updated: 2026-02-27T11:09:53.972026Z
author: agent
---

# OCI image generation from Bazel outputs

## Test Objective
Verify that OCI images are correctly generated from Bazel build outputs.

## Test Cases

### TC-1: Generate Dockerfile with default base image
**Given:** Build outputs in a directory, no base_image specified
**When:** `generate_bazel_dockerfile()` is called
**Then:** Creates Dockerfile with `FROM scratch` base

### TC-2: Generate Dockerfile with custom base image
**Given:** Build outputs with `base_image: "gcr.io/distroless/static"`
**When:** `generate_bazel_dockerfile()` is called
**Then:** Creates Dockerfile with specified base image

### TC-3: Docker build succeeds from generated Dockerfile
**Given:** A generated Dockerfile and staged build outputs
**When:** `docker build` is executed
**Then:** Image is created successfully

### TC-4: Image follows Rush naming convention
**Given:** Product "helloworld", component "demo-bazel"
**When:** Image is built
**Then:** Image name is `helloworld/demo-bazel:tag`

### TC-5: Build outputs are copied to image
**Given:** Bazel outputs in `bazel-bin/src/app`
**When:** OCI image is generated
**Then:** Outputs are available at `/app` in the container
