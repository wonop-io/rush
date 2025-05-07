use rush_cli::toolchain::ToolchainContext;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

// Create a git repo with a simple commit history for testing
fn setup_git_repo(dir: &Path) {
    // Initialize git repo
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .unwrap();
    
    // Configure git user for commits
    let _ = Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir)
        .output()
        .unwrap();
    
    let _ = Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .output()
        .unwrap();
    
    // Create a test file and commit it
    fs::write(dir.join("test.txt"), "initial content").unwrap();
    
    let _ = Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir)
        .output()
        .unwrap();
    
    let _ = Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(dir)
        .output()
        .unwrap();
}

#[test]
fn test_toolchain_context_creation() {
    // Test that we can create a basic toolchain context
    let context = ToolchainContext::new();
    
    // The git executable should be available
    assert!(!context.git().is_empty());
}

#[test]
fn test_git_folder_hash() {
    // Skip if git is not available
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    
    let temp_dir = TempDir::new().unwrap();
    setup_git_repo(temp_dir.path());
    
    // Change to the git repo directory for the test
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp_dir.path()).unwrap();
    
    let context = ToolchainContext::new();
    let hash_result = context.get_git_folder_hash(".");
    
    // Restore original directory
    env::set_current_dir(original_dir).unwrap();
    
    // We should get a hash, not an error
    assert!(hash_result.is_ok());
    
    // The hash should be a valid git hash (40 hex chars) or "precommit"
    let hash = hash_result.unwrap();
    if hash != "precommit" {
        assert_eq!(hash.len(), 40);
        // Check that it's a hex string
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}

#[test]
fn test_git_wip() {
    // Skip if git is not available
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    
    let temp_dir = TempDir::new().unwrap();
    setup_git_repo(temp_dir.path());
    
    // Change to the git repo directory for the test
    let original_dir = env::current_dir().unwrap();
    env::set_current_dir(temp_dir.path()).unwrap();
    
    let context = ToolchainContext::new();
    
    // Initially there should be no changes
    let wip_result = context.get_git_wip(".");
    assert!(wip_result.is_ok());
    assert_eq!(wip_result.unwrap(), "");
    
    // Make a change to the file
    fs::write(temp_dir.path().join("test.txt"), "modified content").unwrap();
    
    // Now there should be a WIP hash
    let wip_result = context.get_git_wip(".");
    assert!(wip_result.is_ok());
    let wip = wip_result.unwrap();
    assert!(!wip.is_empty());
    assert!(wip.starts_with("-wip-"));
    
    // Restore original directory
    env::set_current_dir(original_dir).unwrap();
}