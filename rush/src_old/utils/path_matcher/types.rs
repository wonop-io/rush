use glob::Pattern as GlobPattern;
use std::path::{Path, PathBuf};

/// Represents a single pattern from a .gitignore file
#[derive(Debug)]
pub struct Pattern {
    /// Compiled glob pattern
    pattern: GlobPattern,
    /// Original pattern string from .gitignore
    original_pattern: String,
    /// Indicates if this is a negation pattern (starts with !)
    pub is_negation: bool,
    /// Indicates if this pattern applies only to directories (ends with /)
    pub is_directory_only: bool,
}

impl Pattern {
    /// Creates a new Pattern instance from a string
    ///
    /// # Arguments
    ///
    /// * `pattern` - A string slice that holds the pattern from .gitignore
    pub fn new(pattern: String) -> Self {
        let is_negation = pattern.starts_with('!');
        let is_directory_only = pattern.ends_with('/');
        let cleaned_pattern = pattern
            .trim_start_matches('!')
            .trim_end_matches('/')
            .to_string();

        let glob_pattern = if cleaned_pattern.starts_with('/') {
            GlobPattern::new(&cleaned_pattern).expect("Failed to compile glob pattern")
        } else {
            GlobPattern::new(&format!("**/{}", cleaned_pattern))
                .expect("Failed to compile glob pattern")
        };

        Pattern {
            pattern: glob_pattern,
            original_pattern: pattern,
            is_negation,
            is_directory_only,
        }
    }

    /// Checks if the given path matches this pattern
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check
    /// * `is_dir` - Whether the path is a directory
    pub fn matches(&self, path: &Path, is_dir: bool) -> bool {
        if self.is_directory_only && !is_dir {
            return false;
        }

        let path_str = path
            .to_str()
            .expect("Path could not be converted to string");
        self.pattern.matches(path_str)
    }
}
