use rush_cli::utils::{Directory, DockerCrossCompileGuard, first_which, which, resolve_toolchain_path};
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[test]
fn test_directory_chdir() {
    let temp_dir = TempDir::new().unwrap();
    let original_dir = env::current_dir().unwrap();
    
    // Using scope to ensure Directory is dropped
    {
        let _dir_guard = Directory::chdir(temp_dir.path().to_str().unwrap());
        // On macOS, /var might be a symlink to /private/var
        let current = env::current_dir().unwrap();
        assert!(current.to_string_lossy().contains(temp_dir.path().file_name().unwrap().to_string_lossy().as_ref()));
    }
    
    // After the guard is dropped, we should be back to the original directory
    assert_eq!(env::current_dir().unwrap(), original_dir);
}

#[test]
fn test_directory_chpath() {
    let temp_dir = TempDir::new().unwrap();
    let original_dir = env::current_dir().unwrap();
    
    // Using scope to ensure Directory is dropped
    {
        let _dir_guard = Directory::chpath(temp_dir.path());
        // On macOS, /var might be a symlink to /private/var
        let current = env::current_dir().unwrap();
        assert!(current.to_string_lossy().contains(temp_dir.path().file_name().unwrap().to_string_lossy().as_ref()));
    }
    
    // After the guard is dropped, we should be back to the original directory
    assert_eq!(env::current_dir().unwrap(), original_dir);
}

#[test]
fn test_docker_cross_compile_guard() {
    let target = "linux/amd64";

    // Save original env vars if they exist
    let original_cross_container_opts = env::var("CROSS_CONTAINER_OPTS").ok();
    let original_docker_default_platform = env::var("DOCKER_DEFAULT_PLATFORM").ok();
    
    // Create the guard
    {
        let guard = DockerCrossCompileGuard::new(target);
        
        // Check that the environment variables are set correctly
        assert_eq!(env::var("CROSS_CONTAINER_OPTS").unwrap(), format!("--platform {}", target));
        assert_eq!(env::var("DOCKER_DEFAULT_PLATFORM").unwrap(), target);
        assert_eq!(guard.target(), target);
    }
    
    // After guard is dropped, env vars should be restored
    match original_cross_container_opts {
        Some(val) => assert_eq!(env::var("CROSS_CONTAINER_OPTS").unwrap(), val),
        None => assert!(env::var("CROSS_CONTAINER_OPTS").is_err()),
    }
    
    match original_docker_default_platform {
        Some(val) => assert_eq!(env::var("DOCKER_DEFAULT_PLATFORM").unwrap(), val),
        None => assert!(env::var("DOCKER_DEFAULT_PLATFORM").is_err()),
    }
}

#[test]
fn test_which_with_existing_command() {
    // This test assumes 'ls' is available on the test system
    let result = which("ls");
    assert!(result.is_some());
    
    // Path should point to an executable file
    let path = PathBuf::from(result.unwrap());
    assert!(path.exists());
}

#[test]
fn test_which_with_nonexistent_command() {
    // Using a command that is very unlikely to exist
    let result = which("this_command_should_not_exist_anywhere");
    assert!(result.is_none());
}

#[test]
fn test_first_which_with_multiple_candidates() {
    // First candidate doesn't exist, second one should (assuming 'ls' exists)
    let result = first_which(vec!["nonexistent_command", "ls"]);
    assert!(result.is_some());
    
    // Path should point to an executable file
    let path = PathBuf::from(result.unwrap());
    assert!(path.exists());
}

#[test]
fn test_first_which_with_no_matches() {
    let result = first_which(vec!["nonexistent_command1", "nonexistent_command2"]);
    assert!(result.is_none());
}

#[test]
fn test_resolve_toolchain_path() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a mock toolchain file
    let tool_name = "mock_tool";
    let mock_tool_path = temp_dir.path().join(format!("rust-{}", tool_name));
    File::create(&mock_tool_path).unwrap();
    
    // Test resolving the tool
    let result = resolve_toolchain_path(temp_dir.path().to_str().unwrap(), tool_name);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), mock_tool_path.to_string_lossy().into_owned());
}

#[test]
fn test_resolve_toolchain_path_nonexistent_tool() {
    let temp_dir = TempDir::new().unwrap();
    
    let result = resolve_toolchain_path(temp_dir.path().to_str().unwrap(), "nonexistent_tool");
    assert!(result.is_none());
}

#[test]
fn test_resolve_toolchain_path_nonexistent_directory() {
    let result = resolve_toolchain_path("/path/that/does/not/exist", "tool");
    assert!(result.is_none());
}