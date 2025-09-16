# Container Restart Problem Analysis Report - CORRECTED

## Executive Summary

The Rush container reactor fails to rebuild images because **the hash computation only includes files matching watch patterns, not the component's actual source files**. When watch patterns don't match any files, the hash becomes `e3b0c442` (SHA256 of empty string), preventing rebuilds even when component files change.

## The Real Problem

**Watch patterns should define ADDITIONAL files to monitor, not REPLACE the component's files for hashing.**

Current (WRONG) behavior:
- Hash = SHA256(files matching watch patterns only)
- If no patterns match: Hash = SHA256("") = e3b0c442

Correct behavior should be:
- Hash = SHA256(component files + additional watched files)
- Watch patterns add extra dependencies, not replace base files

## Root Cause Analysis

### The Conceptual Error

The `ImageTagGenerator::compute_tag()` method uses `get_watch_files_and_directories()` which:

1. **With watch patterns**: Returns ONLY files matching patterns
2. **Without watch patterns**: Returns ALL component files
3. **Problem**: Watch patterns REPLACE instead of EXTEND the file list

### Code Flow Analysis

```rust
// rush-container/src/tagging/mod.rs:30-57
pub fn compute_tag(&self, spec: &ComponentBuildSpec) -> Result<String> {
    // Gets files based on watch patterns OR component directory
    let (watch_files, watch_dirs) = self.get_watch_files_and_directories(spec);

    // Hash computed from watch_files only
    let content_hash = self.compute_content_hash_from_files(&watch_files)?;
    // ...
}
```

The `get_watch_files_and_directories()` function (lines 60-143):
```rust
if let Some(watches) = &spec.watch {
    // ONLY returns files matching watch patterns
    // Ignores component's actual source files!
} else {
    // Returns all component files (correct behavior)
}
```

## The Architectural Flaw

### Current Logic (INCORRECT)
```
Has watch patterns?
├─ YES: Hash = SHA256(watch pattern matches only)
│       Problem: Ignores component source files!
└─ NO:  Hash = SHA256(all component files)
        This works correctly
```

### Correct Logic Should Be
```
Component files = All files in component directory
Watch files = Files matching watch patterns (if any)
Hash = SHA256(Component files ∪ Watch files)
```

## Why This Matters

1. **Build Determinism**: The hash should represent the actual build inputs (component source)
2. **Watch Patterns Purpose**: Should extend monitoring to dependencies, not restrict it
3. **Developer Experience**: Changes to ANY source file should trigger rebuilds

## Example Scenario

Given:
- Component: `frontend` at `app/frontend/`
- Source file: `app/frontend/src/lib.rs`
- Watch patterns: `["**/*_screens", "**/*_api"]`

Current behavior:
1. `src/lib.rs` changes
2. Doesn't match watch patterns
3. Hash = SHA256("") = e3b0c442
4. No rebuild (WRONG!)

Expected behavior:
1. `src/lib.rs` changes
2. Component files include `src/lib.rs`
3. Hash = SHA256(component files including lib.rs)
4. Hash changes, triggers rebuild (CORRECT!)

## The Fix

### Solution: Separate Component Files from Watch Files

Modify `get_watch_files_and_directories()` to always include component files:

```rust
fn get_watch_files_and_directories(&self, spec: &ComponentBuildSpec)
    -> (Vec<PathBuf>, Vec<PathBuf>) {

    let mut files = Vec::new();
    let mut dirs = Vec::new();

    // ALWAYS include component directory files
    let component_dir = self.get_component_directory(spec);
    for entry in WalkDir::new(&component_dir) {
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }

    // ADDITIONALLY include watch pattern matches
    if let Some(watches) = &spec.watch {
        // Add files matching watch patterns
        // These are IN ADDITION to component files
        for pattern in watches {
            // ... find and add matching files
        }
    }

    (files, dirs)
}
```

### Alternative Solution: Two Separate Methods

```rust
// Get component files for hashing
fn get_component_files(&self, spec: &ComponentBuildSpec) -> Vec<PathBuf>

// Get additional watch files for monitoring
fn get_watch_files(&self, spec: &ComponentBuildSpec) -> Vec<PathBuf>

// In compute_tag:
let component_files = self.get_component_files(spec);
let watch_files = self.get_watch_files(spec);
let all_files = [component_files, watch_files].concat();
let content_hash = self.compute_content_hash_from_files(&all_files)?;
```

## Impact of Current Bug

### Severity: CRITICAL
- **Data Loss Risk**: Developers lose changes when containers don't rebuild
- **Debugging Nightmare**: Hash appears stable despite file changes
- **Trust Erosion**: Developers lose confidence in the build system

### Affected Scenarios
1. Any component with watch patterns that don't cover all source files
2. Components where watch patterns are meant for external dependencies
3. Refactoring that moves files outside watch patterns

## Proper Watch Pattern Semantics

### What Watch Patterns Should Do
- **Monitor external dependencies**: Files outside the component that affect it
- **Cross-component dependencies**: Watch other components' output
- **Config files**: System-wide configuration that affects the component
- **Additional source locations**: Secondary source directories

### What Watch Patterns Should NOT Do
- **Restrict component files**: Should never exclude the component's own files
- **Replace default monitoring**: Should be additive, not replacement
- **Define build inputs**: Build always needs component files

## Testing Requirements

```rust
#[test]
fn test_hash_includes_component_files_with_watch_patterns() {
    // Setup component with watch patterns
    let spec = ComponentBuildSpec {
        watch: Some(vec!["**/*_api".to_string()]),
        // ...
    };

    // Modify component file that doesn't match pattern
    write_file("component/src/main.rs", "changed");

    // Hash should change despite not matching watch pattern
    let hash1 = tag_generator.compute_tag(&spec);
    write_file("component/src/main.rs", "changed again");
    let hash2 = tag_generator.compute_tag(&spec);

    assert_ne!(hash1, hash2, "Component files must always affect hash");
}

#[test]
fn test_empty_watch_patterns_dont_produce_empty_hash() {
    let spec = ComponentBuildSpec {
        watch: Some(vec!["nonexistent/*".to_string()]),
        // ...
    };

    let hash = tag_generator.compute_tag(&spec);
    assert_ne!(hash, "xxxxxxxx-wip-e3b0c442",
               "Should never produce empty string hash");
}
```

## Conclusion

The restart problem stems from a fundamental misunderstanding of watch patterns' purpose. They should **extend** the files monitored for changes, not **replace** the component's source files in hash computation. The fix is straightforward: always include component files in the hash, with watch patterns adding additional files to monitor. This ensures build determinism while allowing flexible dependency monitoring.