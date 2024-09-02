use glob::Pattern as GlobPattern;
use std::fs;
use std::path::{Path, PathBuf};

/// Represents a .gitignore file and its patterns
pub struct GitIgnore {
    /// List of ignore patterns parsed from .gitignore files
    ignore_patterns: Vec<Pattern>,
    /// Root path where the GitIgnore instance was created
    root_path: PathBuf,
}

/// Represents a single pattern from a .gitignore file
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

impl GitIgnore {
    /// Creates a new GitIgnore instance
    ///
    /// # Arguments
    ///
    /// * `start_path` - The path to start searching for .gitignore files
    pub fn new(start_path: &Path) -> Self {
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
        let mut ignore_patterns = Vec::new();
        for path in gitignore_paths.into_iter().rev() {
            let gitignore_content =
                fs::read_to_string(&path).expect("Failed to read .gitignore file");
            ignore_patterns.extend(
                gitignore_content
                    .lines()
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(Pattern::new),
            );
        }

        GitIgnore {
            ignore_patterns,
            root_path: start_path.to_path_buf(),
        }
    }

    /// Checks if a given path should be ignored
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check
    pub fn ignores(&self, path: &Path) -> bool {
        let relative_path = path.strip_prefix(&self.root_path).unwrap_or(path);
        let is_dir = path.is_dir();

        let mut ignored = false;
        for ancestor in relative_path.ancestors() {
            for pattern in &self.ignore_patterns {
                if pattern.matches(ancestor, true) {
                    ignored = !pattern.is_negation;
                }
            }
        }

        if !ignored {
            for pattern in &self.ignore_patterns {
                if pattern.matches(relative_path, is_dir) {
                    ignored = !pattern.is_negation;
                }
            }
        }

        ignored
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

        let gitignore = GitIgnore::new(temp_dir.path());
        fs::create_dir(temp_dir.path().join("test_dir")).unwrap();
        assert!(gitignore.ignores(&temp_dir.path().join("file.txt")));
        assert!(!gitignore.ignores(&temp_dir.path().join("important.txt")));
        assert!(gitignore.ignores(&temp_dir.path().join("test_dir")));
        assert!(!gitignore.ignores(&temp_dir.path().join("file.rs")));
    }

    #[test]
    fn test_directory_only_pattern() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "logs/\n");

        let gitignore = GitIgnore::new(temp_dir.path());

        // Create the "logs" directory
        fs::create_dir(temp_dir.path().join("logs")).unwrap();

        assert!(gitignore.ignores(&temp_dir.path().join("logs")));
        assert!(!gitignore.ignores(&temp_dir.path().join("logs.txt")));
    }

    #[test]
    fn test_nested_gitignore() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "*.log\n");

        let nested_dir = temp_dir.path().join("nested");
        fs::create_dir(&nested_dir).unwrap();
        fs::write(nested_dir.join(".gitignore"), "!important.log\n").unwrap();

        let gitignore = GitIgnore::new(&nested_dir);

        assert!(gitignore.ignores(&nested_dir.join("test.log")));
        assert!(!gitignore.ignores(&nested_dir.join("important.log")));
    }

    #[test]
    fn test_complex_patterns() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "**/*.bak\n**/build/\n!src/**/*.bak\n");

        let gitignore = GitIgnore::new(temp_dir.path());

        // Create the "build" directory
        fs::create_dir(temp_dir.path().join("build")).unwrap();
        fs::create_dir_all(temp_dir.path().join("subdir").join("build")).unwrap();

        assert!(gitignore.ignores(&temp_dir.path().join("file.bak")));
        assert!(gitignore.ignores(&temp_dir.path().join("subdir").join("file.bak")));
        assert!(gitignore.ignores(&temp_dir.path().join("build")));
        assert!(gitignore.ignores(&temp_dir.path().join("subdir").join("build")));
        assert!(!gitignore.ignores(&temp_dir.path().join("src").join("file.bak")));
        assert!(!gitignore.ignores(&temp_dir.path().join("src").join("subdir").join("file.bak")));
        assert!(gitignore.ignores(
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

        let gitignore = GitIgnore::new(temp_dir.path());

        // Create the directory structure
        fs::create_dir_all(temp_dir.path().join("dist")).unwrap();
        fs::create_dir_all(
            temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/dist"),
        )
        .unwrap();

        // Test a file that should not be ignored
        assert!(!gitignore.ignores(
            &temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/src/index.html")
        ));

        // Test various paths
        assert!(gitignore.ignores(&temp_dir.path().join("dist")));
        assert!(gitignore.ignores(
            &temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/dist")
        ));
        assert!(gitignore.ignores(
            &temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/dist/index.html")
        ));
        assert!(gitignore.ignores(
            &temp_dir
                .path()
                .join("products/platform.wonop.com/app/frontend/dist/assets/logo.png")
        ));
    }

    #[test]
    fn test_nonexistent_paths() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "*.txt\ntest_dir/\n");

        let gitignore = GitIgnore::new(temp_dir.path());
        fs::create_dir_all(temp_dir.path().join("test_dir")).unwrap();
        // Test nonexistent paths
        assert!(gitignore.ignores(&temp_dir.path().join("nonexistent.txt")));
        assert!(gitignore.ignores(&temp_dir.path().join("test_dir")));
        assert!(!gitignore.ignores(&temp_dir.path().join("nonexistent.rs")));
    }

    #[test]
    fn test_subdirectory_ignore() {
        let temp_dir = TempDir::new().unwrap();
        create_gitignore(&temp_dir, "ignored_dir/\n");

        let gitignore = GitIgnore::new(temp_dir.path());

        fs::create_dir_all(temp_dir.path().join("ignored_dir/subdir")).unwrap();
        fs::write(
            temp_dir.path().join("ignored_dir/subdir/file.txt"),
            "content",
        )
        .unwrap();

        assert!(gitignore.ignores(&temp_dir.path().join("ignored_dir")));
        assert!(gitignore.ignores(&temp_dir.path().join("ignored_dir/subdir")));
        assert!(gitignore.ignores(&temp_dir.path().join("ignored_dir/subdir/file.txt")));
    }
}
