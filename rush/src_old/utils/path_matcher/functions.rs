use super::types::Pattern;
use std::fs;
use std::path::{Path, PathBuf};

/// Finds all .gitignore files starting from the given path and walking up the directory tree
///
/// # Arguments
///
/// * `start_path` - The path to start searching for .gitignore files
pub fn find_gitignore_files(start_path: &Path) -> Vec<PathBuf> {
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

    gitignore_paths
}

/// Parses .gitignore files and returns patterns
///
/// # Arguments
///
/// * `gitignore_paths` - Paths to .gitignore files
pub fn parse_gitignore_files(gitignore_paths: &[PathBuf]) -> Vec<Pattern> {
    let mut match_patterns = Vec::new();
    for path in gitignore_paths.iter().rev() {
        let gitignore_content = fs::read_to_string(path).expect("Failed to read .gitignore file");
        match_patterns.extend(
            gitignore_content
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(Pattern::new),
        );
    }
    match_patterns
}
