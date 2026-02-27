---
id: TC-444-002
type: test-case
progress: backlog
parents:
- REQ-444-002
priority: high
created: 2026-02-27T11:09:44.459983Z
updated: 2026-02-27T11:09:44.459983Z
author: agent
---

# BazelStrategy validates workspace and executes build

## Test Objective
Verify that `BazelStrategy` correctly validates Bazel workspaces and executes builds.

## Test Cases

### TC-1: Validate workspace with WORKSPACE file
**Given:** A directory containing a WORKSPACE file
**When:** `BazelStrategy.validate()` is called
**Then:** Returns `Ok(())`

### TC-2: Reject directory without WORKSPACE file
**Given:** A directory without WORKSPACE or WORKSPACE.bazel file
**When:** `BazelStrategy.validate()` is called
**Then:** Returns error with clear message

### TC-3: can_handle returns true for Bazel type
**Given:** A `BuildType::Bazel` instance
**When:** `BazelStrategy.can_handle()` is called
**Then:** Returns `true`

### TC-4: can_handle returns false for other types
**Given:** A `BuildType::RustBinary` instance
**When:** `BazelStrategy.can_handle()` is called
**Then:** Returns `false`

### TC-5: Build execution calls bazel with correct arguments
**Given:** A valid Bazel workspace with targets `["//src:app"]`
**When:** `BazelStrategy.build()` is called
**Then:** Executes `bazel build //src:app --compilation_mode=opt`

### TC-6: Build with additional arguments
**Given:** A Bazel config with `additional_args: ["--jobs=8"]`
**When:** `BazelStrategy.build()` is called
**Then:** Arguments are appended to bazel command
