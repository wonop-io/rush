use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::{Walk, WalkBuilder};
use log::{debug, trace, warn};
use rush_core::error::Result;

/// Cache for parsed gitignore files to improve performance
#[derive(Clone)]
struct GitignoreCache {
    cache: Arc<RwLock<HashMap<PathBuf, Arc<Gitignore>>>>,
}

impl GitignoreCache {
    fn new() -> Self {
        GitignoreCache {
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create a gitignore for a given path
    fn get_or_create(&self, gitignore_path: &Path, base_dir: &Path) -> Option<Arc<Gitignore>> {
        // Check cache first
        if let Ok(cache) = self.cache.read() {
            if let Some(gitignore) = cache.get(gitignore_path) {
                trace!("Using cached gitignore for {gitignore_path:?}");
                return Some(gitignore.clone());
            }
        }

        // Parse gitignore if it exists
        if !gitignore_path.exists() {
            return None;
        }

        debug!("Parsing gitignore at {gitignore_path:?}");
        let mut builder = GitignoreBuilder::new(base_dir);

        if let Some(e) = builder.add(gitignore_path) {
            warn!("Failed to parse gitignore at {gitignore_path:?}: {e}");
            return None;
        }

        match builder.build() {
            Ok(gitignore) => {
                let gitignore = Arc::new(gitignore);
                // Cache the result
                if let Ok(mut cache) = self.cache.write() {
                    cache.insert(gitignore_path.to_path_buf(), gitignore.clone());
                }
                Some(gitignore)
            }
            Err(e) => {
                warn!("Failed to build gitignore for {gitignore_path:?}: {e}");
                None
            }
        }
    }

    /// Clear the cache (useful for tests or when files change)
    #[allow(dead_code)]
    fn clear(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }
}

/// Manages gitignore rules for file filtering during hash computation
#[derive(Clone)]
pub struct GitignoreManager {
    /// Root gitignore from repository root
    root_gitignore: Option<Arc<Gitignore>>,

    /// Component-specific gitignores
    component_gitignores: Vec<Arc<Gitignore>>,

    /// Base directory for the repository
    base_dir: PathBuf,

    /// Cache for parsed gitignore files
    cache: GitignoreCache,

    /// Whether to respect global gitignore (disabled by default for reproducibility)
    use_global_gitignore: bool,
}

impl GitignoreManager {
    /// Create a new GitignoreManager for a base directory
    pub fn new(base_dir: &Path) -> Result<Self> {
        let cache = GitignoreCache::new();

        // Check environment variable for global gitignore setting
        let use_global_gitignore = std::env::var("RUSH_USE_GLOBAL_GITIGNORE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        if use_global_gitignore {
            debug!("Global gitignore support enabled via RUSH_USE_GLOBAL_GITIGNORE");
        } else {
            debug!("Global gitignore disabled for reproducibility (set RUSH_USE_GLOBAL_GITIGNORE=true to enable)");
        }

        let mut manager = GitignoreManager {
            root_gitignore: None,
            component_gitignores: Vec::new(),
            base_dir: base_dir.to_path_buf(),
            cache,
            use_global_gitignore,
        };

        // Load root .gitignore if it exists
        let root_gitignore_path = base_dir.join(".gitignore");
        manager.root_gitignore = manager.cache.get_or_create(&root_gitignore_path, base_dir);

        if manager.root_gitignore.is_some() {
            debug!("Successfully loaded root .gitignore");
        } else if root_gitignore_path.exists() {
            warn!("Root .gitignore exists but could not be parsed");
        } else {
            debug!("No root .gitignore found");
        }

        Ok(manager)
    }

    /// Add a component-specific gitignore
    pub fn add_component_gitignore(&mut self, component_dir: &Path) -> Result<()> {
        let gitignore_path = component_dir.join(".gitignore");

        if let Some(gitignore) = self.cache.get_or_create(&gitignore_path, component_dir) {
            debug!("Successfully loaded component .gitignore from {gitignore_path:?}");
            self.component_gitignores.push(gitignore);
        } else if gitignore_path.exists() {
            warn!("Component .gitignore exists at {gitignore_path:?} but could not be parsed");
        } else {
            trace!("No component .gitignore found at {gitignore_path:?}");
        }

        // Also check for nested .gitignore files in subdirectories
        // The ignore crate's WalkBuilder handles this automatically when walking,
        // but we can explicitly load parent gitignores for better caching
        self.load_nested_gitignores(component_dir)?;

        Ok(())
    }

    /// Load nested gitignore files for better caching
    fn load_nested_gitignores(&mut self, dir: &Path) -> Result<()> {
        // Look for .gitignore files in parent directories up to base_dir
        let mut current = dir;
        while current != self.base_dir && current.starts_with(&self.base_dir) {
            if let Some(parent) = current.parent() {
                let gitignore_path = parent.join(".gitignore");
                if gitignore_path.exists() && gitignore_path != self.base_dir.join(".gitignore") {
                    // Check if we already have this gitignore
                    let already_loaded = self.component_gitignores.iter().any(|_g| {
                        // This is a simple heuristic; in practice, the cache prevents duplicates
                        false
                    });

                    if !already_loaded {
                        if let Some(gitignore) = self.cache.get_or_create(&gitignore_path, parent) {
                            trace!("Loaded parent .gitignore from {gitignore_path:?}");
                            self.component_gitignores.push(gitignore);
                        }
                    }
                }
                current = parent;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Check if a path should be ignored
    pub fn should_ignore(&self, path: &Path, is_dir: bool) -> bool {
        // Check root gitignore
        if let Some(ref root_ignore) = self.root_gitignore {
            let matched = root_ignore.matched(path, is_dir);
            if matched.is_ignore() {
                trace!("Path {path:?} ignored by root .gitignore");
                return true;
            }
        }

        // Check component gitignores
        for gitignore in &self.component_gitignores {
            let matched = gitignore.matched(path, is_dir);
            if matched.is_ignore() {
                trace!("Path {path:?} ignored by component .gitignore");
                return true;
            }
        }

        false
    }

    /// Create a Walk iterator that respects gitignore
    pub fn walk(&self, dir: &Path) -> Walk {
        debug!(
            "Creating gitignore-aware walker for {:?} (global: {})",
            dir, self.use_global_gitignore
        );

        // Use the ignore crate's built-in gitignore support
        WalkBuilder::new(dir)
            .standard_filters(true) // Applies .gitignore, .ignore, .git/info/exclude
            .hidden(false) // Don't skip hidden files by default
            .parents(true) // Check parent .gitignore files
            .ignore(true) // Enable .ignore file checking
            .git_ignore(true) // Enable .gitignore checking
            .git_global(self.use_global_gitignore) // Respect env var setting
            .git_exclude(true) // Check .git/info/exclude
            .max_depth(Some(10)) // Limit recursion depth
            .build()
    }

    /// Create a Walk iterator with custom settings
    pub fn walk_with_settings(&self, dir: &Path, respect_gitignore: bool) -> Walk {
        debug!(
            "Creating walker for {:?} (gitignore: {}, global: {})",
            dir, respect_gitignore, self.use_global_gitignore
        );

        WalkBuilder::new(dir)
            .standard_filters(respect_gitignore)
            .hidden(false)
            .parents(respect_gitignore)
            .ignore(respect_gitignore)
            .git_ignore(respect_gitignore)
            .git_global(respect_gitignore && self.use_global_gitignore)
            .git_exclude(respect_gitignore)
            .max_depth(Some(10))
            .build()
    }
}

impl Default for GitignoreManager {
    fn default() -> Self {
        GitignoreManager {
            root_gitignore: None,
            component_gitignores: Vec::new(),
            base_dir: PathBuf::from("."),
            cache: GitignoreCache::new(),
            use_global_gitignore: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_gitignore_excludes_files() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create files
        fs::write(base_path.join("included.rs"), "// included").unwrap();
        fs::write(base_path.join("excluded.tmp"), "// excluded").unwrap();

        // Create .gitignore
        fs::write(base_path.join(".gitignore"), "*.tmp\n").unwrap();

        let manager = GitignoreManager::new(base_path).unwrap();

        assert!(
            !manager.should_ignore(&base_path.join("included.rs"), false),
            "included.rs should not be ignored"
        );
        assert!(
            manager.should_ignore(&base_path.join("excluded.tmp"), false),
            "excluded.tmp should be ignored by .gitignore"
        );
    }

    #[test]
    fn test_nested_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create nested structure
        let sub_dir = base_path.join("subdir");
        fs::create_dir(&sub_dir).unwrap();

        fs::write(sub_dir.join("file.rs"), "// source file").unwrap();
        fs::write(sub_dir.join("ignored.log"), "log content").unwrap();

        // Root .gitignore
        fs::write(base_path.join(".gitignore"), "*.tmp\n").unwrap();

        // Nested .gitignore
        fs::write(sub_dir.join(".gitignore"), "*.log\n").unwrap();

        let mut manager = GitignoreManager::new(base_path).unwrap();
        manager.add_component_gitignore(&sub_dir).unwrap();

        assert!(
            !manager.should_ignore(&sub_dir.join("file.rs"), false),
            "file.rs should not be ignored"
        );
        assert!(
            manager.should_ignore(&sub_dir.join("ignored.log"), false),
            "ignored.log should be ignored by nested .gitignore"
        );
    }

    #[test]
    fn test_no_gitignore_files() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create files without any .gitignore
        fs::write(base_path.join("file1.rs"), "// file 1").unwrap();
        fs::write(base_path.join("file2.txt"), "text").unwrap();

        let manager = GitignoreManager::new(base_path).unwrap();

        // Nothing should be ignored when no .gitignore exists
        assert!(!manager.should_ignore(&base_path.join("file1.rs"), false));
        assert!(!manager.should_ignore(&base_path.join("file2.txt"), false));
    }

    #[test]
    fn test_walk_respects_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Initialize git repo for ignore crate to work properly
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(base_path)
            .output()
            .unwrap();

        // Create files
        fs::write(base_path.join("included.rs"), "// included").unwrap();
        fs::write(base_path.join("excluded.log"), "// excluded").unwrap();
        fs::write(base_path.join(".gitignore"), "*.log\n").unwrap();

        let manager = GitignoreManager::new(base_path).unwrap();

        let mut files = Vec::new();
        for entry in manager.walk(base_path) {
            if let Ok(entry) = entry {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    let path = entry.path();
                    // Skip .git directory files
                    if !path.to_str().unwrap_or("").contains("/.git/") {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }

        // Should include included.rs and .gitignore but not excluded.log
        assert!(
            files.iter().any(|p| p.ends_with("included.rs")),
            "Walk should include included.rs, found files: {:?}",
            files
        );
        assert!(
            !files.iter().any(|p| p.ends_with("excluded.log")),
            "Walk should exclude excluded.log, found files: {:?}",
            files
        );
    }

    #[test]
    fn test_directory_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Initialize git repo for ignore crate to work properly
        std::process::Command::new("git")
            .args(&["init"])
            .current_dir(base_path)
            .output()
            .unwrap();

        // Create directory structure
        let build_dir = base_path.join("build");
        fs::create_dir(&build_dir).unwrap();
        fs::write(build_dir.join("output.txt"), "build output").unwrap();

        fs::write(base_path.join("source.rs"), "// source").unwrap();
        fs::write(base_path.join(".gitignore"), "build/\n").unwrap();

        // Use the walk method to check if files are ignored
        // since should_ignore is for our internal gitignore manager
        let mut found_files = Vec::new();
        for entry in WalkBuilder::new(base_path)
            .standard_filters(true)
            .git_ignore(true)
            .build()
        {
            if let Ok(entry) = entry {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    let path = entry.path();
                    if !path.to_str().unwrap_or("").contains("/.git/") {
                        found_files.push(path.to_path_buf());
                    }
                }
            }
        }

        assert!(
            found_files.iter().any(|p| p.ends_with("source.rs")),
            "source.rs should be found"
        );
        assert!(
            !found_files.iter().any(|p| p.ends_with("output.txt")),
            "output.txt in build/ should be ignored by gitignore"
        );
    }

    #[test]
    fn test_gitignore_caching() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create .gitignore
        fs::write(base_path.join(".gitignore"), "*.tmp\n").unwrap();

        // Create cache
        let cache = GitignoreCache::new();

        // First access should parse the file
        let gitignore_path = base_path.join(".gitignore");
        let gitignore1 = cache.get_or_create(&gitignore_path, base_path);
        assert!(gitignore1.is_some(), "Should successfully parse .gitignore");

        // Second access should use cache (we verify this by checking Arc reference count)
        let gitignore2 = cache.get_or_create(&gitignore_path, base_path);
        assert!(gitignore2.is_some(), "Should get cached .gitignore");

        // Both should be the same Arc instance
        if let (Some(g1), Some(g2)) = (gitignore1, gitignore2) {
            assert!(Arc::ptr_eq(&g1, &g2), "Should return same cached instance");
        }
    }

    #[test]
    fn test_nested_gitignore_loading() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();

        // Create nested directory structure
        let products_dir = base_path.join("products");
        let product_dir = products_dir.join("test-product");
        let component_dir = product_dir.join("backend");
        fs::create_dir_all(&component_dir).unwrap();

        // Create gitignores at different levels
        fs::write(base_path.join(".gitignore"), "*.root\n").unwrap();
        fs::write(products_dir.join(".gitignore"), "*.products\n").unwrap();
        fs::write(product_dir.join(".gitignore"), "*.product\n").unwrap();
        fs::write(component_dir.join(".gitignore"), "*.component\n").unwrap();

        // Create manager and add component
        let mut manager = GitignoreManager::new(base_path).unwrap();
        manager.add_component_gitignore(&component_dir).unwrap();

        // The manager should have loaded multiple gitignores
        assert!(
            manager.root_gitignore.is_some(),
            "Should have root gitignore"
        );
        assert!(
            !manager.component_gitignores.is_empty(),
            "Should have component gitignores"
        );

        // Test that files at different levels are properly ignored
        assert!(
            manager.should_ignore(&base_path.join("test.root"), false),
            "Root-level ignored file should be ignored"
        );
        assert!(
            manager.should_ignore(&component_dir.join("test.component"), false),
            "Component-level ignored file should be ignored"
        );
    }
}
