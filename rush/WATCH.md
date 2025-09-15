# Watch Functionality Implementation Analysis

## Executive Summary

The `watch:` configuration in Rush allows components to specify file patterns that should trigger rebuilds when changed. Currently, the watch functionality is partially implemented but not fully integrated with glob pattern expansion and the rebuild decision process. This document provides a comprehensive analysis of the current state and a detailed implementation plan.

## Current State Analysis

### 1. Watch Configuration Parsing

**Location**: `rush/crates/rush-build/src/spec.rs:419-428`

The watch configuration is currently parsed from YAML:
```rust
let watch = yaml_section.get("watch").map(|v| {
    let paths: Vec<String> = v
        .as_sequence()
        .unwrap()
        .iter()
        .map(|item| Self::process_template_string(item.as_str().unwrap(), &variables))
        .collect();
    Arc::new(PathMatcher::new(Path::new(&product_dir), paths))
});
```

**Current Behavior**:
- Reads `watch:` as a list of strings from stack.spec.yaml
- Creates a PathMatcher with these paths
- Stores in `ComponentBuildSpec.watch: Option<Arc<PathMatcher>>`

**Example Configuration** (from io.wonop.helloworld):
```yaml
ingress:
  watch:
    - "**/*_app"
    - "**/*_api"
```

### 2. PathMatcher Implementation

**Location**: `rush/crates/rush-utils/src/path_matcher.rs`

Current PathMatcher capabilities:
- Uses glob crate for pattern matching
- Supports gitignore-style patterns
- Can check if a path matches patterns
- **LIMITATION**: Patterns are compiled once at creation, not expanded to actual file paths

### 3. Tag Generation

**Location**: `rush/crates/rush-container/src/tagging/mod.rs`

Current implementation:
```rust
fn get_watch_directories(&self, spec: &ComponentBuildSpec) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // Main component directory
    let component_dir = self.get_component_directory(spec);
    if component_dir.exists() {
        dirs.push(component_dir);
    }

    // Note: watch directories are not extracted from spec.watch
    // This is a missing piece

    dirs
}
```

**Issues**:
1. Does not use `spec.watch` field at all
2. Only considers the main component directory
3. No glob expansion for watch patterns

### 4. Rebuild Decision Process

**Location**: `rush/crates/rush-container/src/build/orchestrator.rs`

Current approach:
- Uses BuildCache to check if component needs rebuild
- Cache checks file modification times
- **MISSING**: No integration with watch patterns or tag changes

### 5. File Watcher Infrastructure

**Location**: `rush/crates/rush-container/src/watcher/`

Components:
- `setup.rs`: Creates file watchers using notify crate
- `processor.rs`: Processes file change events
- **NOT INTEGRATED**: Watcher is defined but not connected to reactor's rebuild logic

## Problems to Solve

### Problem 1: Glob Pattern Expansion
Watch patterns like `"**/*_app"` need to be expanded to actual file paths for:
- Computing accurate git hashes
- Determining which files to monitor
- Calculating content hashes for dirty state

### Problem 2: Dynamic File Discovery
New files matching patterns must be detected:
- Pattern expansion must happen each time we compute tags
- Cannot cache expanded file lists indefinitely

### Problem 3: Tag-Based Rebuild Decision
Current rebuild logic doesn't use tag changes as the trigger

### Problem 4: Integration Gaps
Watch configuration exists but isn't used by:
- Tag generator
- Rebuild decision logic
- File watcher setup

## Proposed Implementation

### Phase 1: Enhance PathMatcher with Glob Expansion

**New Methods for PathMatcher**:
```rust
impl PathMatcher {
    /// Expand glob patterns to actual file paths
    pub fn expand_patterns(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for pattern in &self.match_patterns {
            // Use glob crate to find matching files
            let pattern_str = self.pattern_to_glob_string(pattern);
            for entry in glob::glob(&pattern_str)? {
                if let Ok(path) = entry {
                    files.push(path);
                }
            }
        }

        // Remove duplicates and sort
        files.sort();
        files.dedup();
        Ok(files)
    }

    /// Expand patterns relative to a base directory
    pub fn expand_patterns_from(&self, base: &Path) -> Result<Vec<PathBuf>> {
        // Similar to above but with base directory handling
    }
}
```

### Phase 2: Update Tag Generator

**Modify `ImageTagGenerator::get_watch_directories()`**:
```rust
fn get_watch_directories(&self, spec: &ComponentBuildSpec) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // Main component directory
    let component_dir = self.get_component_directory(spec);
    if component_dir.exists() {
        dirs.push(component_dir.clone());
    }

    // Expand watch patterns if present
    if let Some(watch) = &spec.watch {
        match watch.expand_patterns_from(&self.base_dir) {
            Ok(mut paths) => {
                // Group files by directory for git operations
                for path in paths {
                    if let Some(parent) = path.parent() {
                        dirs.push(parent.to_path_buf());
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to expand watch patterns: {}", e);
            }
        }
    }

    // Remove duplicates
    dirs.sort();
    dirs.dedup();

    dirs
}
```

**Update `compute_content_hash()` to use expanded files**:
```rust
fn compute_content_hash(&self, spec: &ComponentBuildSpec) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut files = Vec::new();

    // Get main directories
    let dirs = self.get_watch_directories(spec);

    // If watch patterns exist, use expanded files directly
    if let Some(watch) = &spec.watch {
        if let Ok(watch_files) = watch.expand_patterns_from(&self.base_dir) {
            files.extend(watch_files);
        }
    }

    // Also walk directories for non-pattern watches
    for dir in dirs {
        // ... existing directory walking code
    }

    // Sort and hash files
    files.sort();
    files.dedup();

    for file in files {
        // ... existing hashing code
    }

    Ok(hex::encode(hasher.finalize()))
}
```

### Phase 3: Implement Tag-Based Rebuild Decision

**New method in Reactor**:
```rust
impl Reactor {
    /// Check if component needs rebuild based on tag change
    async fn needs_rebuild(&self, spec: &ComponentBuildSpec) -> Result<bool> {
        // Get current tag
        let current_tag = self.tag_generator.compute_tag(spec)?;

        // Get deployed tag (from running container or cache)
        let deployed_tag = self.get_deployed_tag(&spec.component_name).await?;

        // Simple comparison
        Ok(current_tag != deployed_tag)
    }

    /// Get the tag of the currently deployed container
    async fn get_deployed_tag(&self, component_name: &str) -> Result<String> {
        // Check running container's image tag
        if let Some(container) = self.get_running_container(component_name).await? {
            if let Some(tag) = container.image_tag() {
                return Ok(tag);
            }
        }

        // Fall back to cache
        if let Some(cached) = self.cache.get(component_name).await? {
            return Ok(cached.tag);
        }

        // No existing deployment
        Ok(String::new())
    }
}
```

**Update build decision in orchestrator**:
```rust
// In BuildOrchestrator::build_components()
for spec in component_specs {
    // Compute current tag
    let current_tag = self.tag_generator.compute_tag(&spec)?;

    // Check if image exists with this tag
    let image_name = format!("{}/{}:{}",
        self.config.product_name,
        spec.component_name,
        current_tag
    );

    if self.docker_client.image_exists(&image_name).await? && !force_rebuild {
        info!("Component '{}' is up-to-date (tag: {})",
            spec.component_name, current_tag);
        continue;
    }

    // Build is needed
    info!("Component '{}' needs rebuild (tag: {})",
        spec.component_name, current_tag);
    // ... proceed with build
}
```

### Phase 4: Integrate File Watcher

**Setup watcher with watch patterns**:
```rust
impl Reactor {
    /// Setup file watchers for all components
    async fn setup_watchers(&mut self) -> Result<()> {
        for service in &self.services {
            if let Some(spec) = self.get_component_spec(&service.name) {
                if let Some(watch) = &spec.watch {
                    // Expand patterns to get initial watch paths
                    let paths = watch.expand_patterns_from(&self.product_dir)?;

                    // Setup watcher for these paths
                    let config = WatcherConfig {
                        root_dir: self.product_dir.clone(),
                        watch_paths: paths,
                        debounce_ms: 500,
                        use_gitignore: true,
                    };

                    let (watcher, processor) = setup_file_watcher(config)?;

                    // Store for later use
                    self.watchers.insert(service.name.clone(), (watcher, processor));
                }
            }
        }
        Ok(())
    }

    /// Check if any watched files changed
    async fn check_for_changes(&self) -> Vec<String> {
        let mut changed_components = Vec::new();

        for (component_name, (_watcher, processor)) in &self.watchers {
            if processor.has_changes() {
                changed_components.push(component_name.clone());
            }
        }

        changed_components
    }
}
```

## Implementation Steps

### Step 1: Extend PathMatcher (2 hours)
1. Add `expand_patterns()` method using glob crate
2. Add `expand_patterns_from()` for base directory support
3. Handle edge cases (non-existent paths, invalid patterns)
4. Add comprehensive tests

### Step 2: Update Tag Generator (1 hour)
1. Modify `get_watch_directories()` to use watch patterns
2. Update `compute_content_hash()` to use expanded files
3. Ensure pattern expansion happens on each tag computation
4. Add tests for watch pattern handling

### Step 3: Implement Tag-Based Rebuilds (2 hours)
1. Add `needs_rebuild()` method to Reactor
2. Add `get_deployed_tag()` to check current deployments
3. Update BuildOrchestrator to use tag comparison
4. Remove or deprecate time-based cache checks
5. Add tests for rebuild decision logic

### Step 4: Connect File Watcher (1 hour)
1. Add watcher setup to Reactor initialization
2. Implement periodic change checking
3. Trigger rebuilds on detected changes
4. Add tests for file watching

### Step 5: Handle Edge Cases (1 hour)
1. New files matching patterns
2. Deleted files that matched patterns
3. Pattern changes in configuration
4. Invalid glob patterns
5. Performance with large file sets

## Benefits of This Approach

### 1. Simplicity
- Single source of truth: the computed tag
- If tag changes, rebuild; if not, skip
- No complex cache invalidation logic

### 2. Correctness
- Captures all file changes via content hash
- Handles new files automatically via pattern re-expansion
- Git-aware for version control integration

### 3. Performance
- Only expands patterns when computing tags
- Can cache tags between checks
- Efficient file watching for development mode

### 4. Flexibility
- Supports glob patterns for broad matching
- Can watch specific files or entire directories
- Works with git-ignored files if needed

## Testing Plan

### Unit Tests
1. PathMatcher glob expansion with various patterns
2. Tag computation with watch patterns
3. Rebuild decision logic
4. File watcher integration

### Integration Tests
1. Full rebuild cycle with watch patterns
2. Detection of new files matching patterns
3. Handling of file deletions
4. Multi-component dependencies with watches

### Manual Testing
1. Test with io.wonop.helloworld's ingress watch patterns
2. Add new files matching patterns, verify rebuild
3. Modify watched files, verify tag changes
4. Test with complex glob patterns

## Migration Considerations

### Backward Compatibility
- Components without `watch:` continue to work
- Default behavior: watch component directory
- Existing cache can coexist during transition

### Configuration Examples
```yaml
# Watch all TypeScript files
frontend:
  watch:
    - "**/*.ts"
    - "**/*.tsx"
    - "package.json"

# Watch shared types
backend:
  watch:
    - "backend/**/*.rs"
    - "../shared/types/**/*"

# Watch multiple patterns
ingress:
  watch:
    - "**/*_app"
    - "**/*_api"
    - "nginx.conf"
```

## Performance Considerations

### Pattern Expansion Cost
- Expansion happens during tag computation
- Can be expensive with broad patterns
- Solution: Limit depth or file count
- Cache expanded results for short duration

### File Hashing Cost
- SHA256 hashing of all matched files
- Can be slow with many/large files
- Solution: Parallel hashing, incremental updates

### Git Operations
- Multiple git commands per tag computation
- Can be slow in large repositories
- Solution: Batch operations, use libgit2

## Future Enhancements

### 1. Incremental Hashing
- Track file hashes between computations
- Only rehash changed files
- Significantly faster for large file sets

### 2. Pattern Validation
- Validate patterns at configuration load time
- Warn about overly broad patterns
- Suggest optimizations

### 3. Watch Profiles
- Development: aggressive watching
- Production: minimal watching
- Custom profiles for different scenarios

### 4. Dependency Tracking
- Automatically watch dependencies
- Traverse import/include statements
- Build dependency graph

## Conclusion

The watch functionality requires integration across multiple components:
1. **PathMatcher**: Needs glob expansion capability
2. **Tag Generator**: Must use watch patterns for directories and files
3. **Rebuild Logic**: Should be tag-based, not time-based
4. **File Watcher**: Needs connection to rebuild triggers

The proposed implementation provides a clean, simple approach where the computed tag becomes the single source of truth for rebuild decisions. This eliminates complex cache invalidation logic and ensures correctness even with dynamic file additions.

The key insight is that by re-expanding glob patterns on each tag computation, we automatically handle new files without maintaining complex file watch lists. Combined with content hashing for dirty state, this provides a robust solution for development and production builds.