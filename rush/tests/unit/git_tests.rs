use rush_cli::git::{is_git_repo, get_current_branch, get_latest_commit, is_working_dir_clean};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::io::Read;
use tempfile::TempDir;

fn init_git_repo(dir: &Path) -> Result<(), std::io::Error> {
    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()?;
    
    // Configure git user for commits
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir)
        .output()?;
    
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir)
        .output()?;
    
    Ok(())
}

fn create_commit(dir: &Path, filename: &str, content: &str) -> Result<(), std::io::Error> {
    // Create a file
    let file_path = dir.join(filename);
    let mut file = File::create(&file_path)?;
    file.write_all(content.as_bytes())?;
    
    // Stage the file
    Command::new("git")
        .args(["add", filename])
        .current_dir(dir)
        .output()?;
    
    // Create a commit
    Command::new("git")
        .args(["commit", "-m", &format!("Add {}", filename)])
        .current_dir(dir)
        .output()?;
    
    Ok(())
}

#[test]
fn test_is_git_repo() {
    let temp_dir = TempDir::new().unwrap();
    
    // Initially not a git repo
    assert!(!is_git_repo(temp_dir.path()));
    
    // Initialize git repo
    init_git_repo(temp_dir.path()).unwrap();
    
    // Now it should be a git repo
    assert!(is_git_repo(temp_dir.path()));
}

#[test]
fn test_get_current_branch() {
    let temp_dir = TempDir::new().unwrap();
    
    // Initialize git repo
    init_git_repo(temp_dir.path()).unwrap();
    
    // Create a commit so we have a branch
    create_commit(temp_dir.path(), "test.txt", "test content").unwrap();
    
    // Master/main branch should be detected
    let branch = get_current_branch(temp_dir.path());
    assert!(branch.is_some());
    
    // Branch name depends on git config, could be "master" or "main"
    let branch_name = branch.unwrap();
    assert!(branch_name == "master" || branch_name == "main");
    
    // Test with invalid path
    let invalid_dir = temp_dir.path().join("nonexistent");
    let branch = get_current_branch(&invalid_dir);
    assert!(branch.is_none());
}

#[test]
fn test_get_latest_commit() {
    let temp_dir = TempDir::new().unwrap();
    
    // Initialize git repo
    init_git_repo(temp_dir.path()).unwrap();
    
    // Create a commit
    create_commit(temp_dir.path(), "test.txt", "test content").unwrap();
    
    // Get latest commit
    let commit = get_latest_commit(temp_dir.path());
    assert!(commit.is_some());
    
    // Should be a valid git hash (40 hex characters)
    let commit_hash = commit.unwrap();
    assert_eq!(commit_hash.len(), 40);
    assert!(commit_hash.chars().all(|c| c.is_ascii_hexdigit()));
    
    // Test with invalid path
    let invalid_dir = temp_dir.path().join("nonexistent");
    let commit = get_latest_commit(&invalid_dir);
    assert!(commit.is_none());
    
    // Create another commit and verify hash changes
    create_commit(temp_dir.path(), "another.txt", "more content").unwrap();
    let new_commit = get_latest_commit(temp_dir.path()).unwrap();
    assert_ne!(commit_hash, new_commit);
}

#[test]
fn test_is_working_dir_clean() {
    let temp_dir = TempDir::new().unwrap();
    
    // Initialize git repo
    init_git_repo(temp_dir.path()).unwrap();
    
    // Create a commit
    create_commit(temp_dir.path(), "test.txt", "test content").unwrap();
    
    // Initially clean
    assert!(is_working_dir_clean(temp_dir.path()));
    
    // Create an unstaged file
    let file_path = temp_dir.path().join("unstaged.txt");
    let mut file = File::create(&file_path).unwrap();
    file.write_all(b"unstaged content").unwrap();
    
    // Now it should not be clean
    assert!(!is_working_dir_clean(temp_dir.path()));
    
    // Stage the file and check
    Command::new("git")
        .args(["add", "unstaged.txt"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    
    // Working dir is still not clean (staged but uncommitted changes)
    assert!(!is_working_dir_clean(temp_dir.path()));
    
    // Commit the file
    Command::new("git")
        .args(["commit", "-m", "Add unstaged.txt"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    
    // Working dir should be clean again
    assert!(is_working_dir_clean(temp_dir.path()));
    
    // Test with invalid path
    let invalid_dir = temp_dir.path().join("nonexistent");
    assert!(!is_working_dir_clean(&invalid_dir));
    
    // Test with invalid git command output
    // Create a file that will cause git status to fail
    let git_dir = temp_dir.path().join(".git");
    let config_path = git_dir.join("config");
    let mut file = File::open(&config_path).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    
    // Corrupt the git config
    let mut file = File::create(&config_path).unwrap();
    file.write_all(b"invalid git config").unwrap();
    
    // Now git status should fail, expect false
    assert!(!is_working_dir_clean(temp_dir.path()));
}