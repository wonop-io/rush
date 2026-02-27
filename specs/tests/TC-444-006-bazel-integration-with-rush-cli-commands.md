---
id: TC-444-006
type: test-case
progress: backlog
parents:
- REQ-444-006
priority: high
created: 2026-02-27T11:10:25.450902Z
updated: 2026-02-27T11:10:25.450902Z
author: agent
---

# Bazel integration with Rush CLI commands

## Test Objective
Verify that Bazel components work correctly with Rush CLI commands.

## Test Cases

### TC-1: rush build includes Bazel component
**Given:** A product with a Bazel component defined
**When:** `rush build` is executed
**Then:** Bazel component is built and image is created

### TC-2: rush dev starts Bazel component container
**Given:** A built Bazel component
**When:** `rush dev` is executed
**Then:** Container starts and appears in logs

### TC-3: File watching triggers Bazel rebuild
**Given:** Bazel component with `watch` configuration
**When:** Source file is modified
**Then:** Bazel rebuild is triggered

### TC-4: Bazel build errors are displayed
**Given:** A Bazel workspace with syntax errors
**When:** Build is attempted
**Then:** Bazel error output is shown to user

### TC-5: Component dependencies work with Bazel
**Given:** Bazel component with `depends_on: [database]`
**When:** `rush dev` is executed
**Then:** Database starts before Bazel component
