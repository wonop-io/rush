//! File change processing for container watchers
//!
//! This module provides functionality for processing file changes detected
//! by container watchers, determining which containers need to be rebuilt.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{debug, info, trace, warn};
use tokio::time;

use crate::container::BuildProcessor;
use crate::error::Result;
use crate::utils::PathMatcher;

/// Processes file changes and determines rebuild needs
#[derive(Debug, Clone)]
pub struct ChangeProcessor {
    /// Files that have changed since last processing
    changed_files: Arc<Mutex<Vec<PathBuf>>>,
    /// Path matcher for excluding files (e.g., from .gitignore)
    gitignore_matcher: PathMatcher,
    /// Debounce timer for batching file changes
    debounce_delay: Duration,
}

impl ChangeProcessor {
    /// Creates a new change processor
    ///
    /// # Arguments
    ///
    /// * `product_dir` - Root directory of the product
    /// * `debounce_ms` - Milliseconds to wait for additional changes before processing
    pub fn new(product_dir: &Path, debounce_ms: u64) -> Self {
        Self {
            changed_files: Arc::new(Mutex::new(Vec::new())),
            gitignore_matcher: PathMatcher::from_gitignore(product_dir),
            debounce_delay: Duration::from_millis(debounce_ms),
        }
    }

    /// Adds a file change to be processed
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file that changed
    pub fn add_change(&self, path: PathBuf) {
        if self.should_ignore(&path) {
            info!("Ignoring change to file: {} (matched gitignore)", path.display());
            return;
        }

        info!("Recording change to file: {}", path.display());
        let mut files = self.changed_files.lock().unwrap();
        files.push(path);
        info!("Total recorded changes: {}", files.len());
    }

    /// Determines if a file should be ignored based on .gitignore rules
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check
    fn should_ignore(&self, path: &Path) -> bool {
        // Skip hidden files and common temporary files
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if filename.starts_with('.') || filename.ends_with('~') || filename.ends_with(".tmp") {
            return true;
        }

        // Check against .gitignore patterns
        self.gitignore_matcher.matches(path)
    }

    /// Gets a reference to the changed files collection
    pub fn changed_files(&self) -> Arc<Mutex<Vec<PathBuf>>> {
        self.changed_files.clone()
    }

    /// Processes any pending file changes after the debounce period
    ///
    /// Returns the list of changed files if any
    pub async fn process_pending_changes(&self) -> Result<Vec<PathBuf>> {
        // Check if we have any pending changes first
        let has_changes = {
            let files = self.changed_files.lock().unwrap();
            !files.is_empty()
        };
        
        if !has_changes {
            return Ok(Vec::new());
        }
        
        // Wait for the debounce period to collect multiple rapid changes
        time::sleep(self.debounce_delay).await;

        let mut files = self.changed_files.lock().unwrap();
        if files.is_empty() {
            return Ok(Vec::new());
        }

        // Get unique paths
        let mut unique_paths: HashSet<PathBuf> = HashSet::new();
        let changed_files: Vec<PathBuf> = files
            .drain(..)
            .filter(|path| unique_paths.insert(path.clone()))
            .collect();

        debug!("Processing {} file changes", changed_files.len());

        // Log the changes
        for path in &changed_files {
            info!("Detected change to file: {}", path.display());
        }

        // Return the list of changed files
        Ok(changed_files)
    }

    /// Checks if any of the changed files affects a specific component
    ///
    /// # Arguments
    ///
    /// * `component_matcher` - Path matcher for the component
    pub fn affects_component(&self, component_matcher: &PathMatcher) -> bool {
        let files = self.changed_files.lock().unwrap();
        files.iter().any(|path| component_matcher.matches(path))
    }

    /// Clear all pending changes
    pub fn clear(&self) {
        let mut files = self.changed_files.lock().unwrap();
        files.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_directory() -> TempDir {
        let temp_dir = TempDir::new().unwrap();

        // Create a .gitignore file
        let mut gitignore = File::create(temp_dir.path().join(".gitignore")).unwrap();
        writeln!(gitignore, "*.log").unwrap();
        writeln!(gitignore, "dist/").unwrap();
        writeln!(gitignore, "node_modules/").unwrap();

        // Create some test directories
        fs::create_dir_all(temp_dir.path().join("src")).unwrap();
        fs::create_dir_all(temp_dir.path().join("dist")).unwrap();
        fs::create_dir_all(temp_dir.path().join("node_modules")).unwrap();

        temp_dir
    }

    #[test]
    fn test_should_ignore_gitignore_patterns() {
        let temp_dir = create_test_directory();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Should ignore files from .gitignore
        assert!(processor.should_ignore(&temp_dir.path().join("app.log")));
        assert!(processor.should_ignore(&temp_dir.path().join("dist/main.js")));
        assert!(processor.should_ignore(&temp_dir.path().join("node_modules/some-lib")));

        // Should not ignore regular files
        assert!(!processor.should_ignore(&temp_dir.path().join("src/main.rs")));
        assert!(!processor.should_ignore(&temp_dir.path().join("Cargo.toml")));
    }

    #[test]
    fn test_should_ignore_hidden_and_temp_files() {
        let temp_dir = TempDir::new().unwrap();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Should ignore hidden files
        assert!(processor.should_ignore(&temp_dir.path().join(".env")));
        assert!(processor.should_ignore(&temp_dir.path().join(".DS_Store")));

        // Should ignore temporary files
        assert!(processor.should_ignore(&temp_dir.path().join("backup~")));
        assert!(processor.should_ignore(&temp_dir.path().join("temp.tmp")));
    }

    #[test]
    fn test_add_change() {
        let temp_dir = TempDir::new().unwrap();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Add a change
        let test_file = temp_dir.path().join("src/main.rs");
        processor.add_change(test_file.clone());

        // Verify it was added
        let files = processor.changed_files.lock().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], test_file);
    }

    #[test]
    fn test_add_change_ignores_filtered_files() {
        let temp_dir = create_test_directory();
        let processor = ChangeProcessor::new(temp_dir.path(), 100);

        // Try to add an ignored file
        processor.add_change(temp_dir.path().join("app.log"));

        // Verify it was not added
        let files = processor.changed_files.lock().unwrap();
        assert_eq!(files.len(), 0);
    }
}
