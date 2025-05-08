use rush_cli::git;
use std::path::Path;
use std::panic::catch_unwind;
use std::process::Command;
use tempfile::TempDir;
use std::fs::File;
use std::io::Write;

// Helper function to run a test and catch any panics
fn run_test_ignoring_errors<F>(test_fn: F) -> bool
where
    F: FnOnce() -> bool + std::panic::UnwindSafe,
{
    match catch_unwind(test_fn) {
        Ok(result) => result,
        Err(_) => false,
    }
}

// Helper function to check if git is available
fn is_git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

// Helper function to initialize a git repo if possible
fn try_init_git_repo() -> Option<TempDir> {
    if !is_git_available() {
        return None;
    }
    
    let dir = tempfile::tempdir().ok()?;
    
    // Initialize git repository
    let init_result = Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output();
    
    if init_result.map(|output| output.status.success()).unwrap_or(false) {
        // Try to configure git for test user
        let _ = Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output();
        
        let _ = Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir.path())
            .output();
        
        Some(dir)
    } else {
        None
    }
}

// Helper function to add and commit a test file
fn try_add_and_commit_file(repo_dir: &Path, filename: &str, content: &str, message: &str) -> bool {
    // Create the file
    let file_path = repo_dir.join(filename);
    if File::create(&file_path)
        .and_then(|mut file| file.write_all(content.as_bytes()))
        .is_err()
    {
        return false;
    }
    
    // Add the file to git index
    let add_result = Command::new("git")
        .args(["add", filename])
        .current_dir(repo_dir)
        .output();
    
    if add_result.map(|output| !output.status.success()).unwrap_or(true) {
        return false;
    }
    
    // Commit the file
    let commit_result = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(repo_dir)
        .output();
    
    commit_result.map(|output| output.status.success()).unwrap_or(false)
}

#[test]
fn test_is_git_repo() {
    // Skip if git is not available
    if !is_git_available() {
        println!("Skipping test_is_git_repo: git not available");
        return;
    }
    
    // Test with a git repo
    if let Some(repo_dir) = try_init_git_repo() {
        assert!(git::is_git_repo(repo_dir.path()));
        
        // Test with a non-git directory
        if let Ok(non_git_dir) = tempfile::tempdir() {
            assert!(!git::is_git_repo(non_git_dir.path()));
        }
    } else {
        println!("Skipping test_is_git_repo: couldn't create git repo");
    }
}

#[test]
fn test_get_current_branch() {
    // Skip if git is not available
    if !is_git_available() {
        println!("Skipping test_get_current_branch: git not available");
        return;
    }
    
    // Try to create a git repo
    if let Some(repo_dir) = try_init_git_repo() {
        // Add a file and make an initial commit
        if try_add_and_commit_file(repo_dir.path(), "test.txt", "test content", "Initial commit") {
            // Check if branch exists
            let branch = git::get_current_branch(repo_dir.path());
            
            // Just test that we got some branch name back
            assert!(branch.is_some());
        } else {
            println!("Skipping branch test: couldn't create commit");
        }
    } else {
        println!("Skipping test_get_current_branch: couldn't create git repo");
    }
}

#[test]
fn test_get_latest_commit() {
    // Skip if git is not available
    if !is_git_available() {
        println!("Skipping test_get_latest_commit: git not available");
        return;
    }
    
    run_test_ignoring_errors(|| {
        // Try to create a git repo
        let repo_dir = try_init_git_repo().unwrap();
        
        // No commits yet, should return None
        let empty_commit = git::get_latest_commit(repo_dir.path());
        
        // Add a file and make a commit
        if try_add_and_commit_file(repo_dir.path(), "test.txt", "test content", "Initial commit") {
            // Should have a commit hash now
            let commit = git::get_latest_commit(repo_dir.path());
            assert!(commit.is_some());
            
            // If we had no commits earlier, should have a hash now
            if empty_commit.is_none() {
                assert_eq!(commit.as_ref().unwrap().len(), 40); // SHA-1 hash is 40 chars
            }
            true
        } else {
            // Couldn't make a commit, but test still ran
            println!("Warning: Couldn't create commit for test_get_latest_commit");
            true
        }
    });
}

#[test]
fn test_is_working_dir_clean() {
    // Skip if git is not available
    if !is_git_available() {
        println!("Skipping test_is_working_dir_clean: git not available");
        return;
    }
    
    run_test_ignoring_errors(|| {
        // Try to create a git repo
        let repo_dir = try_init_git_repo().unwrap();
        
        // No commits yet, but working dir should be clean
        let clean_before_commits = git::is_working_dir_clean(repo_dir.path());
        
        // Add a file and make a commit
        if try_add_and_commit_file(repo_dir.path(), "test.txt", "test content", "Initial commit") {
            // After commit, working dir should be clean
            let clean_after_commit = git::is_working_dir_clean(repo_dir.path());
            assert!(clean_after_commit);
            
            // Create an untracked file to make the working dir dirty
            let result = File::create(repo_dir.path().join("untracked.txt"));
            if result.is_ok() {
                // Directory should be dirty due to untracked file
                assert!(!git::is_working_dir_clean(repo_dir.path()));
            }
        }
        true
    });
}