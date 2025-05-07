// Re-export the test_utils module at the integration test level
pub use crate::test_utils::*;

// Additional helper functions specific to integration tests
use std::env;
use std::process::Command;
use std::path::Path;

/// Check if docker is available for testing
pub fn docker_available() -> bool {
    Command::new("docker")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Set up the environment for integration tests
pub fn setup_integration_test_env() {
    // Set test mode
    env::set_var("RUSH_TEST_MODE", "true");
    env::set_var("RUSH_INTEGRATION_TEST", "true");
}

/// Clean up after integration tests
pub fn cleanup_integration_test_env() {
    env::remove_var("RUSH_TEST_MODE");
    env::remove_var("RUSH_INTEGRATION_TEST");
}

/// Runs a command in the given directory and returns the output
pub fn run_command_in_dir(dir: &Path, command: &str, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new(command)
        .args(args)
        .current_dir(dir)
        .output()?;
    
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Command failed: {} {}\nError: {}",
                command,
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            ),
        ))
    }
}