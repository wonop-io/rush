use super::types::Pattern;
use crate::utils::path_matcher::find_gitignore_files;
use crate::utils::path_matcher::parse_gitignore_files;
use std::fs;
use std::path::{Path, PathBuf};

/// Represents a .gitignore file and its patterns
#[derive(Debug)]
pub struct PathMatcher {
    /// List of ignore patterns parsed from .gitignore files
    match_patterns: Vec<Pattern>,
    /// Root path where the PathMatcher instance was created
    root_path: PathBuf,
}

impl PathMatcher {
    /// Creates a new PathMatcher instance
    ///
    /// # Arguments
    ///
    /// * `start_path` - The path to start searching for .gitignore files
    /// * `paths` - Vector of pattern strings to match
    pub fn new(start_path: &Path, paths: Vec<String>) -> Self {
        let match_patterns = paths.into_iter().map(Pattern::new).collect();

        PathMatcher {
            match_patterns,
            root_path: start_path.to_path_buf(),
        }
    }

    /// Creates a PathMatcher from .gitignore files
    ///
    /// # Arguments
    ///
    /// * `start_path` - The path to start searching for .gitignore files
    pub fn from_gitignore(start_path: &Path) -> Self {
        let gitignore_paths = find_gitignore_files(start_path);
        let match_patterns = parse_gitignore_files(&gitignore_paths);

        PathMatcher {
            match_patterns,
            root_path: start_path.to_path_buf(),
        }
    }

    /// Checks if a given path should be matched
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check
    pub fn matches(&self, path: &Path) -> bool {
        let relative_path = path.strip_prefix(&self.root_path).unwrap_or(path);
        let is_dir = path.is_dir();

        let mut matched = self.check_ancestor_matches(relative_path);

        if !matched {
            matched = self.check_path_matches(relative_path, is_dir);
        }

        matched
    }

    /// Checks if any ancestors of the path match any patterns
    fn check_ancestor_matches(&self, path: &Path) -> bool {
        let mut matched = false;
        for ancestor in path.ancestors() {
            for pattern in &self.match_patterns {
                if pattern.matches(ancestor, true) {
                    matched = !pattern.is_negation;
                }
            }
        }
        matched
    }

    /// Checks if the path itself matches any patterns
    fn check_path_matches(&self, path: &Path, is_dir: bool) -> bool {
        let mut matched = false;
        for pattern in &self.match_patterns {
            if pattern.matches(path, is_dir) {
                matched = !pattern.is_negation;
            }
        }
        matched
    }
}
