// src/utils/path_matcher/mod.rs
mod functions;
mod matcher;
mod types;

pub use functions::{find_gitignore_files, parse_gitignore_files};
pub use matcher::PathMatcher;
pub use types::Pattern;

// Re-export for backward compatibility
use glob::Pattern as GlobPattern;
use std::fs;
use std::path::{Path, PathBuf};

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
