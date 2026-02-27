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

// Core utilities
// Primary command interface - use these
pub use command::{
    get_command_output, run_command_with_label, CommandConfig, CommandOutput, CommandRunner,
};
pub use directory::Directory;
pub use docker_cross::DockerCrossCompileGuard;
pub use fs::{find_project_root, read_to_string};
pub use path::expand_path;
pub use path_matcher::{PathMatcher, Pattern};
pub use template::TEMPLATES;

// Re-export utility functions from mod.rs
pub use self::r#mod::{first_which, resolve_toolchain_path, which};
