use rush_cli::path_matcher::{PathMatcher, Pattern};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// Helper function to handle temp dir creation failures
fn create_test_dir() -> Option<TempDir> {
    match TempDir::new() {
        Ok(dir) => Some(dir),
        Err(_) => {
            println!("Warning: Failed to create temporary directory, skipping test");
            None
        }
    }
}

// Helper function to create a gitignore file, ignoring errors
fn try_create_gitignore(dir: &Path, content: &str) -> bool {
    fs::write(dir.join(".gitignore"), content).is_ok()
}

#[test]
fn test_pattern_new() {
    // Test regular pattern
    let pattern = Pattern::new("*.txt".to_string());
    assert!(pattern.matches(Path::new("file.txt"), false));
    assert!(!pattern.matches(Path::new("file.jpg"), false));
    
    // Test negation pattern
    let pattern = Pattern::new("!important.txt".to_string());
    assert!(pattern.matches(Path::new("important.txt"), false));
    
    // Test directory-only pattern
    let pattern = Pattern::new("logs/".to_string());
    assert!(pattern.matches(Path::new("logs"), true));
    assert!(!pattern.matches(Path::new("logs"), false)); // Not a directory
}

#[test]
fn test_pattern_matches() {
    // Test regular file pattern
    let pattern = Pattern::new("*.txt".to_string());
    assert!(pattern.matches(Path::new("file.txt"), false));
    assert!(pattern.matches(Path::new("path/to/file.txt"), false));
    assert!(!pattern.matches(Path::new("file.rs"), false));
    
    // Test directory-only pattern
    let pattern = Pattern::new("logs/".to_string());
    assert!(pattern.matches(Path::new("logs"), true));
    assert!(!pattern.matches(Path::new("logs"), false)); // Not a directory
    assert!(pattern.matches(Path::new("path/to/logs"), true));
}

#[test]
fn test_path_matcher_new() {
    let patterns = vec![
        "*.txt".to_string(),
        "!important.txt".to_string(),
        "logs/".to_string()
    ];
    
    let matcher = PathMatcher::new(Path::new("."), patterns);
    
    // Test functionality instead of accessing private fields
    assert!(matcher.matches(Path::new("file.txt")));
    assert!(!matcher.matches(Path::new("important.txt")));
    assert!(!matcher.matches(Path::new("file.rs")));
}

#[test]
fn test_path_matcher_matches() {
    // This test requires filesystem access, wrap in a guard
    if let Some(temp_dir) = create_test_dir() {
        let root_path = temp_dir.path();
        
        let patterns = vec![
            "*.txt".to_string(),
            "!important.txt".to_string(),
            "logs/".to_string(),
            "**/*.bak".to_string(),
        ];
        
        let matcher = PathMatcher::new(root_path, patterns);
        
        // Create test directories if possible
        let logs_created = fs::create_dir_all(root_path.join("logs")).is_ok();
        
        // Test regular patterns
        assert!(matcher.matches(&root_path.join("file.txt")));
        assert!(!matcher.matches(&root_path.join("important.txt")));
        assert!(!matcher.matches(&root_path.join("file.rs")));
        
        // Test directory patterns if test dir was created
        if logs_created {
            assert!(matcher.matches(&root_path.join("logs")));
        }
    }
}

#[test]
fn test_path_matcher_from_gitignore() {
    // Skip this test if we can't create test directories
    if let Some(temp_dir) = create_test_dir() {
        let root_path = temp_dir.path();
        
        // Create .gitignore file
        let gitignore_content = "*.txt\n!important.txt\nlogs/\n";
        
        if try_create_gitignore(root_path, gitignore_content) {
            let matcher = PathMatcher::from_gitignore(root_path);
            
            // Test basic pattern matching
            assert!(matcher.matches(&root_path.join("file.txt")));
            assert!(!matcher.matches(&root_path.join("important.txt")));
            
            // Create logs directory if possible
            if fs::create_dir_all(root_path.join("logs")).is_ok() {
                assert!(matcher.matches(&root_path.join("logs")));
            }
        }
    }
}

#[test]
fn test_path_with_gitignore_patterns() {
    // Test common .gitignore patterns with a temporary in-memory path
    let root_path = PathBuf::from("/dev/null");
    
    let patterns = vec![
        "node_modules/".to_string(),
        "*.log".to_string(),
        "dist".to_string(),
        "!dist/keep.txt".to_string(),
        ".DS_Store".to_string(),
    ];
    
    let matcher = PathMatcher::new(&root_path, patterns);
    
    // Node modules should be matched
    assert!(matcher.matches(&root_path.join("node_modules")));
    assert!(matcher.matches(&root_path.join("subdir/node_modules")));
    
    // Log files should be matched
    assert!(matcher.matches(&root_path.join("error.log")));
    assert!(matcher.matches(&root_path.join("logs/server.log")));
    
    // Dist directory should be matched
    assert!(matcher.matches(&root_path.join("dist")));
    assert!(matcher.matches(&root_path.join("dist/bundle.js")));
    
    // Except the keep.txt file
    assert!(!matcher.matches(&root_path.join("dist/keep.txt")));
    
    // .DS_Store should be matched
    assert!(matcher.matches(&root_path.join(".DS_Store")));
}