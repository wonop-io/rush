//! Rush Core - Foundation types and utilities
//!
//! This crate provides the core types, traits, and utilities used across
//! all other Rush crates.

pub mod config_loader;
pub mod constants;
pub mod dotenv;
pub mod error;
pub mod error_context;
pub mod service_constants;
pub mod shutdown;
pub mod types;

// Re-export commonly used items
pub use constants::*;
pub use error::{Error, Result};
pub use error_context::{ErrorContext, OptionContext};
pub use shutdown::{global_shutdown, ShutdownCoordinator, ShutdownReason};

/// Rush version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
