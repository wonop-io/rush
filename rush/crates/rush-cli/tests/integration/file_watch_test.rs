//! Integration tests for file watching and rebuild functionality

use rush_container::watcher::{setup_file_watcher, ChangeProcessor, WatcherConfig};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

#[tokio::test]
async fn test_file_change_detection() {
    // Create a temporary directory for testing
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.rs");
    
    // Write initial content
    fs::write(&test_file, "// Initial content").unwrap();
    
    // Set up file watcher
    let config = WatcherConfig {
        root_dir: temp_dir.path().to_path_buf(),
        watch_paths: vec![],
        debounce_ms: 100,
        use_gitignore: false,
    };
    
    let (_watcher, processor) = setup_file_watcher(config).unwrap();
    
    // Wait a bit for watcher to initialize
    sleep(Duration::from_millis(200)).await;
    
    // Modify the file
    fs::write(&test_file, "// Modified content").unwrap();
    
    // Wait for the change to be detected
    sleep(Duration::from_millis(500)).await;
    
    // Process pending changes
    let changed_files = processor.process_pending_changes().await.unwrap();
    
    // Verify the change was detected
    assert!(!changed_files.is_empty(), "Should detect file changes");
    assert!(changed_files.iter().any(|p| p.ends_with("test.rs")), 
            "Should detect test.rs change");
}

#[tokio::test]
async fn test_multiple_file_changes() {
    // Create a temporary directory for testing
    let temp_dir = TempDir::new().unwrap();
    let test_file1 = temp_dir.path().join("file1.rs");
    let test_file2 = temp_dir.path().join("file2.rs");
    
    // Write initial content
    fs::write(&test_file1, "// File 1").unwrap();
    fs::write(&test_file2, "// File 2").unwrap();
    
    // Set up file watcher
    let config = WatcherConfig {
        root_dir: temp_dir.path().to_path_buf(),
        watch_paths: vec![],
        debounce_ms: 100,
        use_gitignore: false,
    };
    
    let (_watcher, processor) = setup_file_watcher(config).unwrap();
    
    // Wait for initialization
    sleep(Duration::from_millis(200)).await;
    
    // Modify both files
    fs::write(&test_file1, "// Modified 1").unwrap();
    fs::write(&test_file2, "// Modified 2").unwrap();
    
    // Wait for changes to be detected
    sleep(Duration::from_millis(500)).await;
    
    // Process pending changes
    let changed_files = processor.process_pending_changes().await.unwrap();
    
    // Verify both changes were detected
    assert!(changed_files.len() >= 2, "Should detect multiple file changes");
    assert!(changed_files.iter().any(|p| p.ends_with("file1.rs")), 
            "Should detect file1.rs change");
    assert!(changed_files.iter().any(|p| p.ends_with("file2.rs")), 
            "Should detect file2.rs change");
}

#[tokio::test]
async fn test_gitignore_filtering() {
    // Create a temporary directory for testing
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.rs");
    let ignored_file = temp_dir.path().join("target").join("debug").join("build.rs");
    
    // Create gitignore file
    fs::write(temp_dir.path().join(".gitignore"), "target/\n").unwrap();
    
    // Create the target directory
    fs::create_dir_all(temp_dir.path().join("target").join("debug")).unwrap();
    
    // Write files
    fs::write(&test_file, "// Test file").unwrap();
    fs::write(&ignored_file, "// Ignored file").unwrap();
    
    // Set up file watcher with gitignore enabled
    let config = WatcherConfig {
        root_dir: temp_dir.path().to_path_buf(),
        watch_paths: vec![],
        debounce_ms: 100,
        use_gitignore: true,
    };
    
    let (_watcher, processor) = setup_file_watcher(config).unwrap();
    
    // Wait for initialization
    sleep(Duration::from_millis(200)).await;
    
    // Modify both files
    fs::write(&test_file, "// Modified test").unwrap();
    fs::write(&ignored_file, "// Modified ignored").unwrap();
    
    // Wait for changes
    sleep(Duration::from_millis(500)).await;
    
    // Process pending changes
    let changed_files = processor.process_pending_changes().await.unwrap();
    
    // Verify only non-ignored file was detected
    assert!(changed_files.iter().any(|p| p.ends_with("test.rs")), 
            "Should detect test.rs change");
    assert!(!changed_files.iter().any(|p| p.to_string_lossy().contains("target")), 
            "Should not detect changes in target directory");
}

#[tokio::test]
async fn test_debounce_behavior() {
    // Create a temporary directory for testing
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.rs");
    
    // Write initial content
    fs::write(&test_file, "// Initial").unwrap();
    
    // Set up file watcher with longer debounce
    let config = WatcherConfig {
        root_dir: temp_dir.path().to_path_buf(),
        watch_paths: vec![],
        debounce_ms: 300,
        use_gitignore: false,
    };
    
    let (_watcher, processor) = setup_file_watcher(config).unwrap();
    
    // Wait for initialization
    sleep(Duration::from_millis(200)).await;
    
    // Make rapid changes
    for i in 0..5 {
        fs::write(&test_file, format!("// Change {}", i)).unwrap();
        sleep(Duration::from_millis(50)).await;
    }
    
    // Process changes - should batch them
    let changed_files = processor.process_pending_changes().await.unwrap();
    
    // Should detect the file change, but only once due to debouncing
    assert!(!changed_files.is_empty(), "Should detect changes");
    let test_file_changes = changed_files.iter()
        .filter(|p| p.ends_with("test.rs"))
        .count();
    
    // Due to debouncing, we should see the file only once or a few times, not 5
    assert!(test_file_changes <= 2, 
            "Debouncing should reduce number of reported changes: got {}", test_file_changes);
}