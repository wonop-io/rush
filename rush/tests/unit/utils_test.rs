use crate::*;
use std::env;
use std::path::Path;
use tempfile::TempDir;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directory_chdir() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_string_lossy().to_string();
        let original_dir = env::current_dir().unwrap();
        
        // Test changing directory
        {
            let _dir_guard = utils::Directory::chdir(&temp_path);
            assert_eq!(env::current_dir().unwrap(), temp_dir.path());
        }
        
        // Test that original directory is restored
        assert_eq!(env::current_dir().unwrap(), original_dir);
    }

    #[test]
    fn test_directory_chpath() {
        let temp_dir = TempDir::new().unwrap();
        let original_dir = env::current_dir().unwrap();
        
        // Test changing directory using Path
        {
            let _dir_guard = utils::Directory::chpath(temp_dir.path());
            assert_eq!(env::current_dir().unwrap(), temp_dir.path());
        }
        
        // Test that original directory is restored
        assert_eq!(env::current_dir().unwrap(), original_dir);
    }

    #[test]
    fn test_which_found() {
        // This should work on most systems that have 'ls' or 'dir'
        #[cfg(unix)]
        let tool = "ls";
        #[cfg(windows)]
        let tool = "cmd";
        
        let result = utils::which(tool);
        assert!(result.is_some());
    }

    #[test]
    fn test_which_not_found() {
        // A tool that should not exist on most systems
        let result = utils::which("non_existent_tool_xyz_123");
        assert!(result.is_none());
    }

    #[test]
    fn test_first_which() {
        #[cfg(unix)]
        {
            // At least one of these should exist on Unix
            let candidates = vec!["ls", "cp", "mv"];
            let result = utils::first_which(candidates);
            assert!(result.is_some());
        }
        
        #[cfg(windows)]
        {
            // At least one of these should exist on Windows
            let candidates = vec!["cmd", "dir", "echo"];
            let result = utils::first_which(candidates);
            assert!(result.is_some());
        }
        
        // None of these should exist
        let candidates = vec!["xyz_123", "abc_456", "non_existent_cmd"];
        let result = utils::first_which(candidates);
        assert!(result.is_none());
    }
}