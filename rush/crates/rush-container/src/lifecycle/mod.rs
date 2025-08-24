//! Container lifecycle management
//!
//! This module provides components for managing the lifecycle of containers,
//! including starting, stopping, monitoring, and graceful shutdown.

pub mod manager;
pub mod monitor;
pub mod shutdown;

// Re-export main types
pub use manager::{LifecycleManager, LifecycleConfig};
pub use monitor::{HealthMonitor, HealthCheckConfig, HealthStatus};
pub use shutdown::{ShutdownManager, ShutdownConfig, ShutdownStrategy};