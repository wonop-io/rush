//! Rush Core - Foundation types and utilities
//!
//! This crate provides the core types, traits, and utilities used across
//! all other Rush crates.

pub mod cache;
pub mod config_loader;
pub mod config_repository;
pub mod constants;
pub mod dotenv;
pub mod error;
pub mod error_context;
pub mod events;
pub mod middleware;
pub mod performance;
pub mod plugin;
pub mod service_constants;
pub mod shutdown;
pub mod state_machine;
pub mod types;

// Re-export commonly used items
pub use constants::*;
pub use error::{Error, Result};
pub use error_context::{ErrorContext, OptionContext};
pub use events::{EventBus, EventHandler, SystemEvent, global_event_bus, publish_event};
pub use shutdown::{global_shutdown, ShutdownCoordinator, ShutdownReason};

/// Rush version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
