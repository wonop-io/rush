//! Command-line interface for Rush CLI
//!
//! This module contains the CLI argument parsing and command execution logic.

mod args;
mod commands;
pub mod context;
mod context_builder;
mod execute;
mod init;

pub use args::{
    parse_args, parse_redirected_components, parse_silenced_components, CommandArgs, CommonCliArgs,
    DeployArgs, DescribeCommand,
};
pub use commands::{
    execute_apply, execute_build, execute_deploy, execute_describe, execute_dev, execute_install,
    execute_secrets, execute_unapply, execute_uninstall, execute_validate, RolloutCommand,
    VaultCommand,
};
pub use context_builder::create_context;
pub use context_builder::setup_logging;
pub use execute::execute_command;
pub use init::init_application;
