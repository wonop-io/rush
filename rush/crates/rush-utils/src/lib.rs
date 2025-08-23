//! Rush Utils - General utilities and helpers

pub mod command;
pub mod directory;
pub mod docker_cross;
pub mod fs;
pub mod git;
mod r#mod;
pub mod path;
pub mod path_matcher;
pub mod process;
pub mod template;
pub mod version;

// Deprecated - use command module instead
#[deprecated(note = "Use command module instead")]
pub mod command_runner;

// Core utilities
pub use directory::Directory;
pub use docker_cross::DockerCrossCompileGuard;
pub use fs::{find_project_root, read_to_string};
pub use path::expand_path;
pub use path_matcher::{PathMatcher, Pattern};
pub use template::TEMPLATES;

// Re-export utility functions from mod.rs
pub use self::r#mod::{first_which, resolve_toolchain_path, which};

// Primary command interface - use these
pub use command::{
    CommandConfig, CommandOutput, CommandRunner, 
    get_command_output, run_command_with_label
};

// Deprecated - for backward compatibility only
#[deprecated(note = "Use CommandRunner::run() instead")]
pub use command_runner::run_command_in_window;

/// Run a command with proper error handling
/// Deprecated: Use CommandRunner::run() with CommandConfig instead
#[deprecated(note = "Use CommandRunner::run() with CommandConfig instead")]
pub async fn run_command(
    name: &str,
    command: &str,
    args: Vec<&str>,
) -> Result<String, anyhow::Error> {
    let config = CommandConfig::new(command)
        .args(args)
        .capture(true);
    
    let output = CommandRunner::run(config).await
        .map_err(|e| anyhow::anyhow!("Command failed: {}", e))?;
    
    Ok(output.stdout)
}
