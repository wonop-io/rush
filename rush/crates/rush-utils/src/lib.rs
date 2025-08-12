//! Rush Utils - General utilities and helpers

pub mod command_runner;
pub mod directory;
pub mod docker_cross;
pub mod fs;
pub mod git;
pub mod path;
pub mod path_matcher;
pub mod process;
pub mod template;
pub mod version;

pub use directory::Directory;
pub use docker_cross::DockerCrossCompileGuard;
pub use path::expand_path;
pub use path_matcher::{PathMatcher, Pattern};

/// Run a command with proper error handling
pub async fn run_command(
    name: &str,
    command: &str,
    args: Vec<&str>,
) -> Result<String, anyhow::Error> {
    command_runner::run_command(name, command, args).await
        .map_err(|e| anyhow::anyhow!("Command failed: {}", e))
}
