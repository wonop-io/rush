---
id: REQ-444-002
type: requirement
progress: backlog
parents:
- UC-444-001
priority: high
created: 2026-02-27T10:51:12.196109Z
updated: 2026-02-27T10:51:12.196109Z
author: agent
---

# BazelStrategy implementation in rush-build crate

## Description
Implement a `BazelStrategy` struct that implements the `BuildStrategy` trait for executing Bazel builds and generating OCI images.

## Specification

### BazelStrategy struct
Located in `rush-build/src/strategy.rs` or a new `rush-build/src/bazel_strategy.rs` file.

```rust
pub struct BazelStrategy;

#[async_trait]
impl BuildStrategy for BazelStrategy {
    fn name(&self) -> &str { "Bazel" }
    fn can_handle(&self, build_type: &BuildType) -> bool;
    fn validate(&self, spec: &ComponentBuildSpec) -> Result<()>;
    async fn build(&self, spec: &ComponentBuildSpec, context: &BuildContext) -> Result<()>;
    fn requires_docker(&self, spec: &ComponentBuildSpec) -> bool;
    fn artifacts_path(&self, spec: &ComponentBuildSpec) -> Option<String>;
}
```

### Build Process
1. **Validation**: Check that the Bazel workspace exists (WORKSPACE or WORKSPACE.bazel file present)
2. **Path Resolution**: Resolve output_dir (support both relative and absolute paths)
3. **Directory Creation**: Create output directory if it doesn't exist
4. **Bazel Build**: Execute `bazel build` with:
   - Targets from configuration (default: `//...`)
   - `--compilation_mode=opt` for release builds
   - Any additional arguments from configuration
5. **OCI Image Generation**: Create OCI-compliant image from build outputs

### Error Handling
- Clear error messages when Bazel is not installed
- Clear error messages when workspace is invalid
- Proper propagation of Bazel build errors with output

## Acceptance Criteria
- [ ] `BazelStrategy` is registered in `BuildStrategyRegistry::new()`
- [ ] Validation checks for WORKSPACE file presence
- [ ] Build execution calls Bazel with correct arguments
- [ ] Output directory is created if it doesn't exist
- [ ] Build errors are properly captured and reported
- [ ] Integration with existing Rush build pipeline
