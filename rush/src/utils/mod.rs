//! Utility functions for file system, process management, Docker, and other operations.
//!
//! This module provides a set of utilities that are used throughout the Rush CLI.

mod command_runner;
mod directory;
mod docker_cross;
mod fs;
mod git;
mod path;
mod path_matcher;
mod process;
mod template;
mod version;

// Public exports from utility modules
pub use self::command_runner::{run_command, run_command_in_window};
pub use self::directory::Directory;
pub use self::docker_cross::DockerCrossCompileGuard;
pub use self::fs::*;
pub use self::git::*;
pub use self::path::*;
pub use self::path_matcher::*;
pub use self::process::*;
pub use self::template::*;
pub use self::version::check_version;

// Common helper functions
/// Finds the first available executable from a list of candidates
pub fn first_which(candidates: Vec<&str>) -> Option<String> {
    for candidate in &candidates {
        if let Some(path) = which(candidate) {
            return Some(path);
        }
    }
    None
}

/// Locates an executable in PATH
pub fn which(tool: &str) -> Option<String> {
    let home_var = crate::constants::HOME_VAR;
    let expanded_tool = if tool.starts_with("$HOME/") || tool.starts_with("~/") {
        let home = std::env::var(home_var).ok()?;
        let path = if tool.starts_with("$HOME/") {
            tool.replacen("$HOME/", &format!("{home}/"), 1)
        } else {
            tool.replacen("~/", &format!("{home}/"), 1)
        };
        path
    } else {
        tool.to_string()
    };

    if std::path::Path::new(&expanded_tool).exists() {
        return Some(expanded_tool);
    }

    let output = std::process::Command::new("which")
        .arg(tool)
        .output()
        .ok()?;

    if output.status.success() {
        let path = std::str::from_utf8(&output.stdout).ok()?.trim().to_string();
        if !path.is_empty() {
            return Some(path);
        }
    }

    None
}

/// Resolves a tool path within a toolchain directory
pub fn resolve_toolchain_path(path: &str, tool: &str) -> Option<String> {
    let dir_path = std::path::Path::new(path);
    if !dir_path.exists() || !dir_path.is_dir() {
        return None;
    }

    if let Ok(entries) = std::fs::read_dir(dir_path) {
        for entry in entries.filter_map(Result::ok) {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            if file_name_str.contains(tool) {
                return Some(entry.path().to_string_lossy().into_owned());
            }
        }
    }

    None
}
