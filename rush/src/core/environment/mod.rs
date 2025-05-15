//! Environment setup and variable management
//!
//! This module provides functionality for setting up the environment for Rush CLI
//! and managing environment variables across different deployment environments.

mod setup;
mod variables;

pub use setup::{load_environment_variables, setup_environment};
pub use variables::{Variables, VariablesFile};
