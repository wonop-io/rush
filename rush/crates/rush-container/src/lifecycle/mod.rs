//! Container lifecycle management
//!
//! This module provides components for managing the lifecycle of containers,
//! including graceful shutdown.

pub mod shutdown;

// Re-export main types
pub use shutdown::{ShutdownConfig, ShutdownManager, ShutdownStrategy};
