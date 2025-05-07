use crate::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a gitignore file
    fn create_gitignore(dir: &Path, content: &str) {
        fs::write(dir.join(".gitignore"), content).unwrap();
    }

    #[test]
    fn test_pattern_constructor() {
        // Test basic pattern
        let pattern = path_matcher::Pattern::new("*.txt".to_string());
        assert!(!pattern.is_negation);
        assert!(!pattern.is_directory_only);
        
        // Test negation pattern
        let pattern = path_matcher::Pattern::new("!important.txt".to_string());
        assert!(pattern.is_negation);
        assert!(!pattern.is_directory_only);
        
        // Test directory pattern
        let pattern = path_matcher::Pattern::new("node_modules/".to_string());
        assert!(!pattern.is_negation);
        assert!(pattern.is_directory_only);
        
        // Test negated directory pattern
        let pattern = path_matcher::Pattern::new("!node_modules/".to_string());
        assert!(pattern.is_negation);
        assert!(pattern.is_directory_only);
    }

    #[test]
    fn test_pattern_matching() {
        // Test file pattern
        let pattern = path_matcher::Pattern::new("*.txt".to_string());
        assert!(pattern.matches(Path::new("file.txt"), false));
        assert!(pattern.matches(Path::new("path/to/file.txt"), false));
        assert!(!pattern.matches(Path::new("file.rs"), false));
        
        // Test directory pattern
        let pattern = path_matcher::Pattern::new("node_modules/".to_string());
        assert!(pattern.matches(Path::new("node_modules"), true));
        assert!(!pattern.matches(Path::new("node_modules"), false)); // Not a directory
        assert!(pattern.matches(Path::new("path/to/node_modules"), true));
        
        // Test absolute path pattern
        let pattern = path_matcher::Pattern::new("/specific/path".to_string());
        assert!(pattern.matches(Path::new("specific/path"), false));
        assert!(!pattern.matches(Path::new("different/path"), false));
    }

    #[test]
    fn test_path_matcher_custom_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let patterns = vec![
            "*.log".to_string(),
            "!important.log".to_string(),
            "node_modules/".to_string(),
        ];
        
        let matcher = path_matcher::PathMatcher::new(temp_dir.path(), patterns);
        
        // Create test directory structure
        fs::create_dir_all(temp_dir.path().join("node_modules")).unwrap();
        fs::write(temp_dir.path().join("test.log"), "test").unwrap();
        fs::write(temp_dir.path().join("important.log"), "important").unwrap();
        
        // Test matching
        assert!(matcher.matches(&temp_dir.path().join("test.log")));
        assert!(!matcher.matches(&temp_dir.path().join("important.log")));
        assert!(matcher.matches(&temp_dir.path().join("node_modules")));
        assert!(!matcher.matches(&temp_dir.path().join("test.txt")));
    }

    #[test]
    fn test_path_matcher_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create test directory structure with nested .gitignore files
        create_gitignore(temp_dir.path(), "*.log\n!important.log\n");
        
        let nested_dir = temp_dir.path().join("src");
        fs::create_dir_all(&nested_dir).unwrap();
        create_gitignore(&nested_dir, "*.tmp\n*.bak\n");
        
        let matcher = path_matcher::PathMatcher::from_gitignore(temp_dir.path());
        
        // Files in root dir
        assert!(matcher.matches(&temp_dir.path().join("test.log")));
        assert!(!matcher.matches(&temp_dir.path().join("important.log")));
        
        // Files in nested dir
        assert!(matcher.matches(&nested_dir.join("test.log")));
        assert!(!matcher.matches(&nested_dir.join("important.log")));
        assert!(matcher.matches(&nested_dir.join("file.tmp")));
        assert!(matcher.matches(&nested_dir.join("file.bak")));
        assert!(!matcher.matches(&nested_dir.join("file.txt")));
    }

    #[test]
    fn test_path_matcher_complex_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let patterns = vec![
            "**/node_modules/**".to_string(),
            "**/.git/**".to_string(),
            "*.min.js".to_string(),
            "!vendor/**/*.min.js".to_string(),
        ];
        
        let matcher = path_matcher::PathMatcher::new(temp_dir.path(), patterns);
        
        // Create test directory structure
        fs::create_dir_all(temp_dir.path().join("node_modules/package")).unwrap();
        fs::create_dir_all(temp_dir.path().join(".git/objects")).unwrap();
        fs::create_dir_all(temp_dir.path().join("vendor/js")).unwrap();
        fs::create_dir_all(temp_dir.path().join("src/js")).unwrap();
        fs::write(temp_dir.path().join("src/js/app.min.js"), "content").unwrap();
        fs::write(temp_dir.path().join("vendor/js/lib.min.js"), "content").unwrap();
        
        // Test matching
        assert!(matcher.matches(&temp_dir.path().join("node_modules")));
        assert!(matcher.matches(&temp_dir.path().join("node_modules/package")));
        assert!(matcher.matches(&temp_dir.path().join("node_modules/package/file.js")));
        assert!(matcher.matches(&temp_dir.path().join(".git/objects/12/345")));
        assert!(matcher.matches(&temp_dir.path().join("src/js/app.min.js")));
        assert!(!matcher.matches(&temp_dir.path().join("vendor/js/lib.min.js")));
        assert!(!matcher.matches(&temp_dir.path().join("src/js/app.js")));
    }
}