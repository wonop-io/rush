//! Rush Core - Foundation types and utilities
//!
//! This crate provides the core types, traits, and utilities used across
//! all other Rush crates.

pub mod constants;
pub mod dotenv;
pub mod error;
pub mod service_constants;
pub mod shutdown;
pub mod types;

// Re-export commonly used items
pub use constants::*;
pub use error::{Error, Result};
pub use shutdown::{global_shutdown, ShutdownCoordinator, ShutdownReason};

/// Rush version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
