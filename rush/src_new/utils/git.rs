use std::path::Path;
use std::process::Command;
use std::str;

/// Checks if a path is inside a git repository
pub fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Gets the current git branch name
pub fn get_current_branch(path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(path)
        .output()
        .ok()?;

    if output.status.success() {
        let branch = str::from_utf8(&output.stdout).ok()?;
        Some(branch.trim().to_string())
    } else {
        None
    }
}

/// Gets the latest commit hash
pub fn get_latest_commit(path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(path)
        .output()
        .ok()?;

    if output.status.success() {
        let commit = str::from_utf8(&output.stdout).ok()?;
        Some(commit.trim().to_string())
    } else {
        None
    }
}

/// Checks if the working directory is clean (no uncommitted changes)
pub fn is_working_dir_clean(path: &Path) -> bool {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(path)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let status = str::from_utf8(&output.stdout).unwrap_or("");
            status.trim().is_empty()
        }
        _ => false,
    }
}
