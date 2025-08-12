//! Rush Utils - General utilities and helpers

pub mod command_runner;
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

pub use directory::Directory;
pub use docker_cross::DockerCrossCompileGuard;
pub use fs::{find_project_root, read_to_string};
pub use path::expand_path;
pub use path_matcher::{PathMatcher, Pattern};
pub use template::TEMPLATES;

// Re-export utility functions from mod.rs
pub use self::r#mod::{first_which, resolve_toolchain_path, which};

// Re-export command runner functions
pub use command_runner::run_command_in_window;

/// Run a command with proper error handling
pub async fn run_command(
    name: &str,
    command: &str,
    args: Vec<&str>,
) -> Result<String, anyhow::Error> {
    command_runner::run_command(name, command, args).await
        .map_err(|e| anyhow::anyhow!("Command failed: {}", e))
}
