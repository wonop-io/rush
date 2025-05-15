use glob::Pattern as GlobPattern;
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

/// Represents a single pattern from a .gitignore file
#[derive(Debug)]
pub struct Pattern {
    /// Compiled glob pattern
    pattern: GlobPattern,
    /// Original pattern string from .gitignore
    original_pattern: String,
    /// Indicates if this is a negation pattern (starts with !)
    is_negation: bool,
    /// Indicates if this pattern applies only to directories (ends with /)
    is_directory_only: bool,
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

impl PathMatcher {
    /// Creates a new PathMatcher instance
    ///
    /// # Arguments
    ///
    /// * `start_path` - The path to start searching for .gitignore files
    pub fn new(start_path: &Path, paths: Vec<String>) -> Self {
        let match_patterns = paths.into_iter().map(Pattern::new).collect();

        PathMatcher {
            match_patterns,
            root_path: start_path.to_path_buf(),
        }
    }

    pub fn from_gitignore(start_path: &Path) -> Self {
        let mut current_path = start_path.to_path_buf();
        let mut gitignore_paths = Vec::new();

        // Walk up the directory tree to find all .gitignore files
        loop {
            let gitignore_path = current_path.join(".gitignore");
            if gitignore_path.exists() {
                gitignore_paths.push(gitignore_path);
            }
            if !current_path.pop() {
                break;
            }
        }

        // Read all .gitignore files and collect patterns
        let mut match_patterns = Vec::new();
        for path in gitignore_paths.into_iter().rev() {
            let gitignore_content =
                fs::read_to_string(&path).expect("Failed to read .gitignore file");
            match_patterns.extend(
                gitignore_content
                    .lines()
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(Pattern::new),
            );
        }

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

        let mut matched = false;
        for ancestor in relative_path.ancestors() {
            for pattern in &self.match_patterns {
                if pattern.matches(ancestor, true) {
                    matched = !pattern.is_negation;
                }
            }
        }

        if !matched {
            for pattern in &self.match_patterns {
                if pattern.matches(relative_path, is_dir) {
                    matched = !pattern.is_negation;
                }
            }
        }

        matched
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_gitignore(temp_dir: &TempDir, content: &str) {
        fs::write(temp_dir.path().join(".gitignore"), content).unwrap();
    }

    #[test]
    fn test_basic_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "*.txt\n!important.txt\ntest_dir/\n");

        let gitignore = PathMatcher::from_gitignore(temp_dir.path());
        fs::create_dir(temp_dir.path().join("test_dir")).unwrap();
        assert!(gitignore.matches(&temp_dir.path().join("file.txt")));
        assert!(!gitignore.matches(&temp_dir.path().join("important.txt")));
        assert!(gitignore.matches(&temp_dir.path().join("test_dir")));
        assert!(!gitignore.matches(&temp_dir.path().join("file.rs")));
    }

    #[test]
    fn test_directory_only_pattern() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "logs/\n");

        let gitignore = PathMatcher::from_gitignore(temp_dir.path());

        // Create the "logs" directory
        fs::create_dir(temp_dir.path().join("logs")).unwrap();

        assert!(gitignore.matches(&temp_dir.path().join("logs")));
        assert!(!gitignore.matches(&temp_dir.path().join("logs.txt")));
    }

    #[test]
    fn test_nested_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "*.log\n");

        let nested_dir = temp_dir.path().join("nested");
        fs::create_dir(&nested_dir).unwrap();
        fs::write(nested_dir.join(".gitignore"), "!important.log\n").unwrap();

        let gitignore = PathMatcher::from_gitignore(&nested_dir);

        assert!(gitignore.matches(&nested_dir.join("test.log")));
        assert!(!gitignore.matches(&nested_dir.join("important.log")));
    }

    #[test]
    fn test_complex_patterns() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "**/*.bak\n**/build/\n!src/**/*.bak\n");

        let gitignore = PathMatcher::from_gitignore(temp_dir.path());

        // Create the "build" directory
        fs::create_dir(temp_dir.path().join("build")).unwrap();
        fs::create_dir_all(temp_dir.path().join("subdir").join("build")).unwrap();

        assert!(gitignore.matches(&temp_dir.path().join("file.bak")));
        assert!(gitignore.matches(&temp_dir.path().join("subdir").join("file.bak")));
        assert!(gitignore.matches(&temp_dir.path().join("build")));
        assert!(gitignore.matches(&temp_dir.path().join("subdir").join("build")));
        assert!(!gitignore.matches(&temp_dir.path().join("src").join("file.bak")));
        assert!(!gitignore.matches(&temp_dir.path().join("src").join("subdir").join("file.bak")));
        assert!(gitignore.matches(
            &temp_dir
                .path()
                .join("subdir")
                .join("build")
                .join("text.txt")
        ));
    }

    #[test]
    fn test_pattern_matching() {
        let pattern = Pattern::new("*.txt".to_string());
        assert!(pattern.matches(Path::new("file.txt"), false));
        assert!(!pattern.matches(Path::new("file.rs"), false));

        let pattern = Pattern::new("!important.txt".to_string());
        assert!(pattern.matches(Path::new("important.txt"), false));
        assert!(pattern.is_negation);

        let pattern = Pattern::new("logs/".to_string());
        assert!(pattern.matches(Path::new("logs"), true));
        assert!(!pattern.matches(Path::new("logs"), false));
        assert!(pattern.is_directory_only);
    }

    #[test]
    fn test_dist_ignore() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "dist\n**/dist\ndist/\n");

        let gitignore = PathMatcher::from_gitignore(temp_dir.path());

        // Create the directory structure
        fs::create_dir_all(temp_dir.path().join("dist")).unwrap();
        fs::create_dir_all(
            temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/dist"),
        )
        .unwrap();

        // Test a file that should not be matched
        assert!(!gitignore.matches(
            &temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/src/index.html")
        ));

        // Test various paths
        assert!(gitignore.matches(&temp_dir.path().join("dist")));
        assert!(gitignore.matches(
            &temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/dist")
        ));
        assert!(gitignore.matches(
            &temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/dist/index.html")
        ));
        assert!(gitignore.matches(
            &temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/dist/assets/logo.png")
        ));
    }

    #[test]
    fn test_nonexistent_paths() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "*.txt\ntest_dir/\n");

        let gitignore = PathMatcher::from_gitignore(temp_dir.path());
        fs::create_dir_all(temp_dir.path().join("test_dir")).unwrap();
        // Test nonexistent paths
        assert!(gitignore.matches(&temp_dir.path().join("nonexistent.txt")));
        assert!(gitignore.matches(&temp_dir.path().join("test_dir")));
        assert!(!gitignore.matches(&temp_dir.path().join("nonexistent.rs")));
    }

    #[test]
    fn test_subdirectory_ignore() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "matched_dir/\n");

        let gitignore = PathMatcher::from_gitignore(temp_dir.path());

        fs::create_dir_all(temp_dir.path().join("matched_dir/subdir")).unwrap();
        fs::write(
            temp_dir.path().join("matched_dir/subdir/file.txt"),
            "content",
        )
        .unwrap();

        assert!(gitignore.matches(&temp_dir.path().join("matched_dir")));
        assert!(gitignore.matches(&temp_dir.path().join("matched_dir/subdir")));
        assert!(gitignore.matches(&temp_dir.path().join("matched_dir/subdir/file.txt")));
    }
}
