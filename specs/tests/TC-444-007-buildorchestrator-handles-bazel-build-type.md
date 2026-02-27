---
id: TC-444-007
type: test-case
progress: backlog
parents:
- REQ-444-007
priority: high
created: 2026-02-27T11:10:35.262057Z
updated: 2026-02-27T11:10:35.262057Z
author: agent
---

# BuildOrchestrator handles Bazel build type

## Test Objective
Verify that `BuildOrchestrator::build_single` correctly handles `BuildType::Bazel` and produces runnable Docker images.

## Test Cases

### TC-1: build_single returns image name for Bazel type
**Given:** A `ComponentBuildSpec` with `BuildType::Bazel`
**When:** `build_single()` is called
**Then:** Returns `Ok(full_image_name)` with correct format

### TC-2: Image is added to built_images HashMap
**Given:** Successful Bazel build
**When:** `build_all()` completes
**Then:** `built_images` contains entry for Bazel component

### TC-3: Container starts with built image
**Given:** Bazel image in `built_images`
**When:** `SimpleLifecycleManager::start_service()` is called
**Then:** Container starts successfully

### TC-4: Build logs show Bazel execution
**Given:** A Bazel component being built
**When:** Build executes
**Then:** Logs show `"Building Bazel component: demo-bazel"`

### TC-5: Container appears in system logs
**Given:** Bazel container starting
**When:** Container starts
**Then:** Logs show `"Starting container helloworld.wonop.io-demo-bazel"`

### TC-6: Bazel build failure is handled
**Given:** Invalid Bazel targets
**When:** Build is attempted
**Then:** Error is captured, component marked as failed, error event published

### TC-7: Force rebuild works for Bazel
**Given:** Existing Bazel image in cache
**When:** `build_all(force_rebuild=true)` is called
**Then:** Bazel component is rebuilt regardless of cache
