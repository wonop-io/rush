//! Container reactor module
//!
//! This module provides the container lifecycle management and orchestration
//! through an event-driven reactor pattern.

pub mod config;
pub mod errors;
pub mod state;

// Re-export main types
pub use config::ContainerReactorConfig;
pub use errors::{ReactorError, ReactorResult};
pub use state::{ReactorPhase, ReactorState, SharedReactorState, ComponentState, StateError};

// Include the main reactor implementation
mod core;
pub use core::ContainerReactor;