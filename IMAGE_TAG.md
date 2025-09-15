# Image Tag Computation Analysis and Proposal

## Executive Summary

The current Rush codebase has **multiple, inconsistent methods** for computing Docker image tags, leading to incorrect tagging and cache invalidation issues. This document analyzes the current state and proposes a centralized, deterministic approach for computing tags.

## Current State Analysis

### 1. Multiple Tag Generation Methods

The codebase currently has **THREE different approaches** for generating image tags:

#### A. Timestamp-based Tags (Primary Issue)
**Location**: `rush/crates/rush-container/src/build/orchestrator.rs:539-544`
```rust
fn generate_tag(&self, _spec: &ComponentBuildSpec) -> String {
    // For now, use a timestamp-based tag
    // TODO: Use git commit hash or version from Cargo.toml
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    format!("{}", timestamp)
}
```
**Problem**: This is non-deterministic and changes on every build, completely breaking caching.

#### B. Simple Git Hash (Secondary Approach)
**Location**: `rush/crates/rush-container/src/reactor/modular_core.rs:1711-1736`
```rust
let git_hash = {
    let hash_output = Command::new("git")
        .args(["log", "-n", "1", "--format=%H", "--", &config.product_path().display().to_string()])
        .output()
        .ok();
    // ... truncates to 8 characters
};
// Later used as:
spec.tagged_image_name = Some(format!("{}:{}", name_str, git_hash));
```
**Problem**: Only considers the product directory, doesn't handle dirty state, and is computed once for all components.

#### C. Component-Specific Git Tag with WIP Support
**Location**: `rush/crates/rush-container/src/image_builder.rs:216-323`
```rust
pub fn compute_git_tag(&mut self) -> Result<String> {
    // Gets git hash for component's specific directory
    let git_hash = toolchain.get_git_folder_hash(&context_dir)?;
    // Checks for uncommitted changes
    let wip_suffix = toolchain.get_git_wip(&context_dir)?;
    let tag = format!("{short_hash}{wip_suffix}");
    // ...
}
```
**Problem**: Only used by `ImageBuilder`, not by the main build orchestrator.

### 2. Git Operations Implementation

The toolchain provides proper git operations but they're not consistently used:

**Location**: `rush/crates/rush-toolchain/src/context.rs:352-445`
- `get_git_folder_hash()`: Gets the latest commit hash for a specific directory
- `get_git_wip()`: Computes SHA256 hash of uncommitted changes, returns `-wip-{hash}` suffix

**Current WIP Implementation Issues**:
1. Only hashes the diff, not the actual file contents
2. Uses `git diff HEAD` which may not be deterministic across different git configurations
3. Doesn't consider staged changes separately

### 3. Usage Locations

Image tags are used/needed in multiple places:
1. **Build Orchestrator** (`build/orchestrator.rs`): Main build process - uses timestamps ❌
2. **Reactor Setup** (`reactor/modular_core.rs`): Initial component setup - uses simple git hash ⚠️
3. **Image Builder** (`image_builder.rs`): Component-specific builds - uses proper git+WIP ✅
4. **Manifest Generation** (`reactor/modular_core.rs:1554`): Uses built image tags
5. **Cache Validation** (`build/cache.rs`): Relies on consistent tags for caching

## Problems with Current Implementation

1. **Non-Deterministic Tags**: Timestamp-based tags break caching and reproducibility
2. **Inconsistent Methods**: Different parts of the code compute tags differently
3. **Wrong Scope**: Product-level hash applied to all components instead of component-specific
4. **Incomplete WIP Detection**: Current WIP only hashes diffs, not full context
5. **No Watch Directory Support**: Doesn't consider watch directories for change detection

## Proposed Solution

### 1. Centralized Tag Generation

Create a single, authoritative tag generation service:

```rust
// New file: rush/crates/rush-container/src/tagging/mod.rs
pub struct ImageTagGenerator {
    toolchain: Arc<ToolchainContext>,
    base_dir: PathBuf,
}

impl ImageTagGenerator {
    /// Compute deterministic tag for a component
    pub fn compute_tag(&self, spec: &ComponentBuildSpec) -> Result<String> {
        // 1. Determine watch directories
        let watch_dirs = self.get_watch_directories(spec);

        // 2. Compute git hash for watch directories
        let git_hash = self.compute_git_hash_for_directories(&watch_dirs)?;

        // 3. Check if working directory is dirty
        if self.is_dirty(&watch_dirs)? {
            // 4. Compute SHA256 hash of actual content
            let content_hash = self.compute_content_hash(&watch_dirs)?;
            Ok(format!("{}-wip-{}", &git_hash[..8], &content_hash[..8]))
        } else {
            Ok(git_hash[..8].to_string())
        }
    }

    fn get_watch_directories(&self, spec: &ComponentBuildSpec) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // Main component directory
        dirs.push(self.get_component_directory(spec));

        // Add watch directories from build type
        match &spec.build_type {
            BuildType::TrunkWasm { watch, .. } |
            BuildType::RustBinary { watch, .. } => {
                if let Some(watch_dirs) = watch {
                    for dir in watch_dirs {
                        dirs.push(self.base_dir.join(dir));
                    }
                }
            }
            _ => {}
        }

        dirs
    }

    fn compute_git_hash_for_directories(&self, dirs: &[PathBuf]) -> Result<String> {
        // Get the most recent commit that touched any of these directories
        let mut latest_hash = String::new();
        let mut latest_time = 0i64;

        for dir in dirs {
            let hash_output = Command::new("git")
                .args(["log", "-n", "1", "--format=%H %ct", "--", dir.to_str().unwrap()])
                .output()?;

            if let Ok(output) = String::from_utf8(hash_output.stdout) {
                let parts: Vec<&str> = output.trim().split(' ').collect();
                if parts.len() == 2 {
                    if let Ok(timestamp) = parts[1].parse::<i64>() {
                        if timestamp > latest_time {
                            latest_time = timestamp;
                            latest_hash = parts[0].to_string();
                        }
                    }
                }
            }
        }

        Ok(latest_hash)
    }

    fn is_dirty(&self, dirs: &[PathBuf]) -> Result<bool> {
        for dir in dirs {
            let status = Command::new("git")
                .args(["status", "--porcelain", "--untracked-files=no", dir.to_str().unwrap()])
                .output()?;

            if !status.stdout.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn compute_content_hash(&self, dirs: &[PathBuf]) -> Result<String> {
        use sha2::{Sha256, Digest};
        use walkdir::WalkDir;

        let mut hasher = Sha256::new();
        let mut files = Vec::new();

        // Collect all files in watch directories
        for dir in dirs {
            for entry in WalkDir::new(dir)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    // Skip .git and build artifacts
                    let path = entry.path();
                    if !path.to_str().unwrap().contains("/.git/") &&
                       !path.to_str().unwrap().contains("/target/") &&
                       !path.to_str().unwrap().contains("/dist/") {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }

        // Sort files for deterministic hashing
        files.sort();

        // Hash file paths and contents
        for file in files {
            // Hash the relative path
            if let Ok(rel_path) = file.strip_prefix(&self.base_dir) {
                hasher.update(rel_path.to_str().unwrap().as_bytes());
            }

            // Hash the file content
            if let Ok(content) = std::fs::read(&file) {
                hasher.update(&content);
            }
        }

        Ok(hex::encode(hasher.finalize()))
    }
}
```

### 2. Integration Points

Replace all existing tag generation with calls to the centralized service:

#### A. Build Orchestrator
```rust
// rush/crates/rush-container/src/build/orchestrator.rs
impl BuildOrchestrator {
    fn generate_tag(&self, spec: &ComponentBuildSpec) -> String {
        self.tag_generator.compute_tag(spec)
            .unwrap_or_else(|e| {
                warn!("Failed to compute git tag: {}, using timestamp", e);
                chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
            })
    }
}
```

#### B. Reactor Setup
```rust
// rush/crates/rush-container/src/reactor/modular_core.rs
impl Reactor {
    pub async fn from_product_dir(...) -> Result<Self> {
        let tag_generator = Arc::new(ImageTagGenerator::new(toolchain.clone(), product_path));

        // For each component spec
        for spec in &mut component_specs {
            let tag = tag_generator.compute_tag(&spec)?;
            spec.tagged_image_name = Some(format!("{}:{}", spec.component_name, tag));
        }
    }
}
```

### 3. Tag Format Specification

The final tag format will be:
1. **Clean state**: `{git_hash}` (8 characters)
   - Example: `a3f5c8d2`
2. **Dirty state**: `{git_hash}-wip-{content_hash}` (8 chars each)
   - Example: `a3f5c8d2-wip-b7e9f1a4`

### 4. Properties of the Solution

1. **Deterministic**: Same content always produces the same tag
2. **Component-Specific**: Each component gets its own tag based on its watch directories
3. **Cache-Friendly**: Clean commits produce stable tags
4. **Change-Aware**: Dirty state includes content hash for uniqueness
5. **Reproducible**: Can recreate the exact same tag on different machines

## Implementation Plan

### Phase 1: Create Tag Generator (Priority: High)
1. Create `rush/crates/rush-container/src/tagging/mod.rs`
2. Implement `ImageTagGenerator` with all methods
3. Add comprehensive unit tests

### Phase 2: Update Build Orchestrator (Priority: High)
1. Modify `BuildOrchestrator::new()` to include tag generator
2. Replace `generate_tag()` implementation
3. Update tests

### Phase 3: Update Reactor (Priority: Medium)
1. Replace git hash computation in `from_product_dir()`
2. Use tag generator for all component specs
3. Ensure consistency with build orchestrator

### Phase 4: Update Image Builder (Priority: Low)
1. Replace existing `compute_git_tag()` with calls to tag generator
2. Remove duplicate implementation
3. Maintain backward compatibility

### Phase 5: Testing & Validation
1. Add integration tests for tag consistency
2. Verify caching works correctly
3. Test dirty state detection
4. Validate reproducibility across machines

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_clean_tag_generation() {
        // Verify clean repository produces git hash only
    }

    #[test]
    fn test_dirty_tag_generation() {
        // Verify dirty repository produces hash-wip-hash format
    }

    #[test]
    fn test_deterministic_content_hash() {
        // Verify same content produces same hash
    }

    #[test]
    fn test_watch_directory_inclusion() {
        // Verify all watch directories are considered
    }
}
```

### Integration Tests
```bash
#!/bin/bash
# Test script to verify tag consistency

# 1. Build component and record tag
TAG1=$(rush build --component frontend --dry-run | grep "tag:")

# 2. Build again without changes
TAG2=$(rush build --component frontend --dry-run | grep "tag:")

# 3. Verify tags match
assert_equal "$TAG1" "$TAG2"

# 4. Make a change
echo "test" >> frontend/src/main.rs

# 5. Build with dirty state
TAG3=$(rush build --component frontend --dry-run | grep "tag:")

# 6. Verify tag includes -wip-
assert_contains "$TAG3" "-wip-"
```

## Migration Path

1. **Week 1**: Implement tag generator without integration
2. **Week 2**: Update build orchestrator (main impact)
3. **Week 3**: Update remaining components
4. **Week 4**: Testing and validation
5. **Week 5**: Remove old implementations

## Expected Benefits

1. **Improved Caching**: Consistent tags enable reliable Docker layer caching
2. **Reproducible Builds**: Same source = same tag across environments
3. **Better Change Detection**: Only rebuild when watch directories change
4. **Simplified Codebase**: Single source of truth for tag generation
5. **Performance**: Avoid unnecessary rebuilds with proper cache hits

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Git not available | Tags fail | Fallback to timestamp-based tags |
| Large watch directories | Slow hashing | Implement file filtering and parallel hashing |
| Breaking existing workflows | CI/CD failures | Gradual rollout with feature flag |
| Hash collisions | Wrong image used | Use longer hash prefixes (12 chars) |

## Conclusion

The current tag generation approach is fragmented and inconsistent, leading to cache misses and non-reproducible builds. By centralizing tag generation with proper git hash and content hash computation, we can achieve deterministic, cache-friendly image tags that properly reflect the state of each component's source code.

The proposed solution maintains the desired format of `{git_hash}` for clean states and `{git_hash}-wip-{content_hash}` for dirty states, while ensuring all components use the same, correct algorithm.