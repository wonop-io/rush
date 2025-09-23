# Hash Computation with .gitignore Support - Implementation Plan

## Overview

Currently, the Rush hash computation for container images includes all files in the component directory except for hardcoded exclusions (`.git/`, `target/`, `dist/`, `node_modules/`, `.rush/`). This plan outlines how to properly respect `.gitignore` files when computing hashes, ensuring build reproducibility while excluding files that shouldn't affect builds.

## Current State Analysis

### Current Implementation
**Location**: `rush/crates/rush-container/src/tagging/mod.rs`

The current file filtering logic (lines 84-90, 122-128):
```rust
// Skip common build artifacts
let path_str = path.to_str().unwrap_or("");
if path_str.contains("/.git/") ||
   path_str.contains("/target/") ||
   path_str.contains("/dist/") ||
   path_str.contains("/node_modules/") ||
   path_str.contains("/.rush/") {
    continue;
}
```

### Problems with Current Approach
1. **Hardcoded exclusions**: Only excludes specific directories
2. **No .gitignore respect**: Includes files that git ignores
3. **Inconsistent with git**: Hash includes files not tracked by git
4. **Build artifacts**: May include temporary files affecting hash stability

## Proposed Solution

### High-Level Design
1. Use the `ignore` crate (same as ripgrep) to parse `.gitignore` files
2. Build a gitignore matcher for each component directory
3. Apply gitignore rules when walking directories for hash computation
4. Maintain backwards compatibility with explicit exclusions

### Implementation Steps

## Step 1: Add Dependencies

**File**: `rush/crates/rush-container/Cargo.toml`

Add the `ignore` crate dependency:
```toml
[dependencies]
ignore = "0.4"  # Same crate used by ripgrep
```

## Step 2: Create Gitignore Manager

**New File**: `rush/crates/rush-container/src/tagging/gitignore.rs`

```rust
use ignore::{WalkBuilder, Walk};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::{Path, PathBuf};
use rush_core::error::Result;

pub struct GitignoreManager {
    /// Root gitignore from repository root
    root_gitignore: Option<Gitignore>,

    /// Component-specific gitignores
    component_gitignores: Vec<Gitignore>,
}

impl GitignoreManager {
    /// Create a new GitignoreManager for a base directory
    pub fn new(base_dir: &Path) -> Result<Self> {
        let mut manager = GitignoreManager {
            root_gitignore: None,
            component_gitignores: Vec::new(),
        };

        // Load root .gitignore if it exists
        let root_gitignore_path = base_dir.join(".gitignore");
        if root_gitignore_path.exists() {
            let mut builder = GitignoreBuilder::new(base_dir);
            builder.add(&root_gitignore_path);
            if let Ok(gitignore) = builder.build() {
                manager.root_gitignore = Some(gitignore);
            }
        }

        Ok(manager)
    }

    /// Add a component-specific gitignore
    pub fn add_component_gitignore(&mut self, component_dir: &Path) -> Result<()> {
        let gitignore_path = component_dir.join(".gitignore");
        if gitignore_path.exists() {
            let mut builder = GitignoreBuilder::new(component_dir);
            builder.add(&gitignore_path);
            if let Ok(gitignore) = builder.build() {
                self.component_gitignores.push(gitignore);
            }
        }
        Ok(())
    }

    /// Check if a path should be ignored
    pub fn should_ignore(&self, path: &Path, is_dir: bool) -> bool {
        // Check root gitignore
        if let Some(ref root_ignore) = self.root_gitignore {
            if root_ignore.matched(path, is_dir).is_ignore() {
                return true;
            }
        }

        // Check component gitignores
        for gitignore in &self.component_gitignores {
            if gitignore.matched(path, is_dir).is_ignore() {
                return true;
            }
        }

        false
    }

    /// Create a Walk iterator that respects gitignore
    pub fn walk(&self, dir: &Path) -> Walk {
        WalkBuilder::new(dir)
            .standard_filters(true)  // Applies .gitignore, .ignore, .git/info/exclude
            .hidden(false)           // Don't skip hidden files by default
            .parents(true)           // Check parent .gitignore files
            .ignore(true)            // Enable .ignore file checking
            .git_ignore(true)        // Enable .gitignore checking
            .git_global(true)        // Check global gitignore
            .git_exclude(true)       // Check .git/info/exclude
            .max_depth(Some(10))     // Limit recursion depth
            .build()
    }
}
```

## Step 3: Update ImageTagGenerator

**File**: `rush/crates/rush-container/src/tagging/mod.rs`

### 3.1 Add imports and field

```rust
// Add to imports
mod gitignore;
use gitignore::GitignoreManager;

// Add to ImageTagGenerator struct
pub struct ImageTagGenerator {
    toolchain: Arc<ToolchainContext>,
    base_dir: PathBuf,
    gitignore_manager: GitignoreManager,  // NEW FIELD
}
```

### 3.2 Update constructor

```rust
impl ImageTagGenerator {
    pub fn new(toolchain: Arc<ToolchainContext>, base_dir: PathBuf) -> Self {
        let gitignore_manager = GitignoreManager::new(&base_dir)
            .unwrap_or_else(|e| {
                log::warn!("Failed to initialize gitignore manager: {}", e);
                GitignoreManager::default()
            });

        Self {
            toolchain,
            base_dir,
            gitignore_manager,
        }
    }
}
```

### 3.3 Update get_watch_files_and_directories method

Replace the current walking logic with gitignore-aware walking:

```rust
fn get_watch_files_and_directories(&self, spec: &ComponentBuildSpec)
    -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();

    let component_dir = self.get_component_directory(spec);
    if !component_dir.exists() {
        log::warn!("Component directory does not exist: {:?}", component_dir);
        return (files, dirs);
    }

    dirs.push(component_dir.clone());

    // Create a gitignore manager for this component
    let mut local_gitignore = self.gitignore_manager.clone();
    local_gitignore.add_component_gitignore(&component_dir)
        .unwrap_or_else(|e| {
            log::debug!("No component .gitignore for {}: {}",
                       spec.component_name, e);
        });

    // ALWAYS walk component directory with gitignore respect
    log::debug!("Walking component directory for '{}' with gitignore rules",
               spec.component_name);

    for entry in local_gitignore.walk(&component_dir) {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                log::debug!("Error walking directory: {}", e);
                continue;
            }
        };

        let path = entry.path();

        // Skip if gitignore says to ignore
        if local_gitignore.should_ignore(path, entry.file_type().is_dir()) {
            log::trace!("Ignoring gitignored file: {:?}", path);
            continue;
        }

        if entry.file_type().map_or(false, |ft| ft.is_file()) {
            // Apply additional Rush-specific exclusions
            // (backwards compatibility for explicit exclusions)
            let path_str = path.to_str().unwrap_or("");
            if path_str.contains("/.rush/") {
                continue;  // Always exclude .rush directory
            }

            files.push(path.to_path_buf());
        }
    }

    let component_file_count = files.len();
    log::debug!("Found {} component files for '{}' (after gitignore filtering)",
                component_file_count, spec.component_name);

    // Handle watch patterns for additional files
    if let Some(watch) = &spec.watch {
        // ... existing watch pattern logic ...
        // But also apply gitignore to watched files
    }

    // Remove duplicates and sort
    files.sort();
    files.dedup();
    dirs.sort();
    dirs.dedup();

    if files.is_empty() {
        log::warn!("No files found for component '{}' after gitignore filtering!",
                  spec.component_name);
    }

    (files, dirs)
}
```

## Step 4: Handle Edge Cases

### 4.1 Nested .gitignore files

The `ignore` crate automatically handles nested `.gitignore` files in subdirectories. Each directory's `.gitignore` rules apply to files within that directory and its subdirectories.

### 4.2 Global gitignore

The system respects:
- `$HOME/.config/git/ignore`
- `$XDG_CONFIG_HOME/git/ignore`
- Git's `core.excludesFile` config

### 4.3 Performance considerations

```rust
// Cache gitignore parsing results
use std::collections::HashMap;
use std::sync::RwLock;

struct GitignoreCache {
    cache: RwLock<HashMap<PathBuf, Arc<Gitignore>>>,
}

impl GitignoreCache {
    fn get_or_create(&self, path: &Path) -> Arc<Gitignore> {
        // Check cache first
        if let Some(gitignore) = self.cache.read().unwrap().get(path) {
            return gitignore.clone();
        }

        // Parse and cache
        let gitignore = Arc::new(parse_gitignore(path));
        self.cache.write().unwrap().insert(path.to_path_buf(), gitignore.clone());
        gitignore
    }
}
```

## Step 5: Testing

### 5.1 Unit tests

**File**: `rush/crates/rush-container/src/tagging/gitignore.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_gitignore_excludes_files() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create files
        fs::write(base_path.join("included.rs"), "").unwrap();
        fs::write(base_path.join("excluded.tmp"), "").unwrap();

        // Create .gitignore
        fs::write(base_path.join(".gitignore"), "*.tmp\n").unwrap();

        let manager = GitignoreManager::new(base_path).unwrap();

        assert!(!manager.should_ignore(&base_path.join("included.rs"), false));
        assert!(manager.should_ignore(&base_path.join("excluded.tmp"), false));
    }

    #[test]
    fn test_nested_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create nested structure
        let sub_dir = base_path.join("subdir");
        fs::create_dir(&sub_dir).unwrap();

        fs::write(sub_dir.join("file.rs"), "").unwrap();
        fs::write(sub_dir.join("ignored.log"), "").unwrap();

        // Root .gitignore
        fs::write(base_path.join(".gitignore"), "*.tmp\n").unwrap();

        // Nested .gitignore
        fs::write(sub_dir.join(".gitignore"), "*.log\n").unwrap();

        let mut manager = GitignoreManager::new(base_path).unwrap();
        manager.add_component_gitignore(&sub_dir).unwrap();

        assert!(!manager.should_ignore(&sub_dir.join("file.rs"), false));
        assert!(manager.should_ignore(&sub_dir.join("ignored.log"), false));
    }
}
```

### 5.2 Integration tests

**File**: `rush/crates/rush-container/tests/test_gitignore_hash.rs`

```rust
#[test]
fn test_hash_excludes_gitignored_files() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Setup component
    let component_dir = base_path.join("component");
    fs::create_dir_all(&component_dir).unwrap();

    // Create files
    fs::write(component_dir.join("main.rs"), "fn main() {}").unwrap();
    fs::write(component_dir.join("temp.log"), "log data").unwrap();

    // Create .gitignore
    fs::write(component_dir.join(".gitignore"), "*.log\n").unwrap();

    // Initialize git
    Command::new("git").args(&["init"]).current_dir(&base_path).output().unwrap();
    Command::new("git").args(&["add", "."]).current_dir(&base_path).output().unwrap();
    Command::new("git").args(&["commit", "-m", "test"]).current_dir(&base_path).output().unwrap();

    let tag_generator = ImageTagGenerator::new(toolchain, base_path.to_path_buf());

    // Get hash before
    let hash1 = tag_generator.compute_tag(&spec).unwrap();

    // Modify gitignored file
    fs::write(component_dir.join("temp.log"), "modified log").unwrap();

    // Hash should NOT change
    let hash2 = tag_generator.compute_tag(&spec).unwrap();
    assert_eq!(hash1, hash2, "Hash should not change for gitignored files");

    // Modify tracked file
    fs::write(component_dir.join("main.rs"), "fn main() { println!(\"hi\"); }").unwrap();

    // Hash SHOULD change
    let hash3 = tag_generator.compute_tag(&spec).unwrap();
    assert_ne!(hash2, hash3, "Hash should change for tracked files");
}
```

## Step 6: Documentation

### 6.1 Update CLAUDE.md

Add section about hash computation:

```markdown
## Hash Computation and Build Triggers

Rush computes content hashes for build determination using:
1. All files in the component directory
2. Additional files matching watch patterns
3. Respects `.gitignore` rules at all levels

Files excluded from hashing:
- Git-ignored files (via .gitignore)
- .rush directory (always excluded)
- Files outside component unless matched by watch patterns

This ensures builds are triggered only by relevant source changes.
```

### 6.2 User documentation

Create `docs/hash-computation.md`:

```markdown
# Hash Computation in Rush

Rush uses content-based hashing to determine when components need rebuilding.

## What's included in the hash

- All files in the component's directory
- Additional files matching `watch` patterns
- File paths and contents (for determinism)

## What's excluded

- Files listed in `.gitignore`
- Global git ignores
- The `.rush` directory

## Debugging hash computation

Use `RUST_LOG=debug` to see which files are included:

```bash
RUST_LOG=debug,rush_container::tagging=trace rush describe images
```
```

## Step 7: Migration and Rollback

### 7.1 Feature flag (optional)

```rust
pub struct ImageTagGenerator {
    use_gitignore: bool,  // Feature flag
    // ...
}

impl ImageTagGenerator {
    pub fn new(toolchain: Arc<ToolchainContext>, base_dir: PathBuf) -> Self {
        let use_gitignore = std::env::var("RUSH_USE_GITIGNORE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);  // Default to enabled

        // ...
    }
}
```

### 7.2 Backwards compatibility

Ensure the new implementation:
1. Still excludes `.rush/` directory explicitly
2. Handles missing `.gitignore` files gracefully
3. Falls back to old behavior if gitignore parsing fails

## Implementation Checklist

- [ ] Add `ignore` crate dependency to Cargo.toml
- [ ] Create `gitignore.rs` module with GitignoreManager
- [ ] Update ImageTagGenerator struct with gitignore_manager field
- [ ] Modify constructor to initialize gitignore_manager
- [ ] Update get_watch_files_and_directories to use gitignore
- [ ] Add unit tests for gitignore functionality
- [ ] Add integration tests for hash computation
- [ ] Update documentation
- [ ] Test with real-world projects
- [ ] Add debug logging for troubleshooting
- [ ] Consider performance implications and add caching if needed
- [ ] Verify backwards compatibility
- [ ] Add feature flag if gradual rollout needed

## Expected Outcomes

1. **Consistent hashing**: Hash matches git's view of tracked files
2. **Stable builds**: Temporary files don't trigger rebuilds
3. **Better performance**: Fewer files to hash
4. **Developer experience**: Aligns with git workflow expectations

## Potential Issues and Mitigations

### Issue 1: Different gitignore parsers
**Problem**: Git and the `ignore` crate might interpret rules differently
**Mitigation**: Use well-tested `ignore` crate (same as ripgrep), extensive testing

### Issue 2: Performance regression
**Problem**: Parsing gitignore files adds overhead
**Mitigation**: Cache parsed gitignore files, use efficient walking

### Issue 3: Breaking changes
**Problem**: Existing hashes might change
**Mitigation**: Document in release notes, provide migration guide

## Success Criteria

1. Hash computation respects `.gitignore` files
2. No performance regression (< 5% slower)
3. All existing tests pass
4. New tests verify gitignore behavior
5. Documentation updated
6. No breaking changes for users without `.gitignore`