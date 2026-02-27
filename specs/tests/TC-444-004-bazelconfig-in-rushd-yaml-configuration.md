---
id: TC-444-004
type: test-case
progress: backlog
parents:
- REQ-444-004
priority: high
created: 2026-02-27T11:10:03.777306Z
updated: 2026-02-27T11:10:03.777306Z
author: agent
---

# BazelConfig in rushd.yaml configuration

## Test Objective
Verify that `BazelConfig` is correctly loaded from `rushd.yaml` and paths are properly resolved.

## Test Cases

### TC-1: Load default BazelConfig when section is missing
**Given:** A `rushd.yaml` without a `bazel` section
**When:** Configuration is loaded
**Then:** Default values are used (`output_dir: "target/bazel-out"`)

### TC-2: Load BazelConfig with absolute output_dir
**Given:** `rushd.yaml` with `bazel.output_dir: "/tmp/bazel-outputs"`
**When:** Configuration is loaded and path resolved
**Then:** Path is `/tmp/bazel-outputs` (unchanged)

### TC-3: Resolve relative output_dir against project root
**Given:** `rushd.yaml` with `bazel.output_dir: ".bazel-out"` and project root `/home/user/project`
**When:** `resolve_output_dir()` is called
**Then:** Path is `/home/user/project/.bazel-out`

### TC-4: Load global_args from configuration
**Given:** `rushd.yaml` with `bazel.global_args: ["--remote_cache=..."]`
**When:** Configuration is loaded
**Then:** `global_args` contains the specified arguments

### TC-5: Backwards compatibility with existing rushd.yaml
**Given:** An existing `rushd.yaml` without bazel section
**When:** Rush starts
**Then:** No errors, default BazelConfig is used
