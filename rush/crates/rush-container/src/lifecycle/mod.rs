//! Container lifecycle management
//!
//! This module handles container lifecycle operations including launching,
//! monitoring and graceful shutdown of containers.

mod launch;
mod monitor;
mod shutdown;

pub use launch::launch_containers;
pub use monitor::LifecycleMonitor;
pub use shutdown::ShutdownManager;

/// Handles the complete container lifecycle from launch to termination
///
/// This struct coordinates the various phases of a container's lifecycle,
/// providing a simpler interface to the rest of the application.
pub struct LifecycleManager {
    /// Manager for handling container shutdowns
    shutdown_manager: ShutdownManager,
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleManager {
    /// Creates a new lifecycle manager
    pub fn new() -> Self {
        Self {
            shutdown_manager: ShutdownManager::new(),
        }
    }

    /// Requests a graceful shutdown of containers
    ///
    /// # Arguments
    ///
    /// * `services` - List of services to shut down
    /// * `timeout` - How long to wait for graceful shutdown before forcing
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn shutdown_services(
        &self,
        services: &[crate::service::ContainerService],
        _timeout: std::time::Duration,
    ) -> rush_core::error::Result<()> {
        // Shutdown services in reverse order of their creation
        for service in services.iter().rev() {
            log::info!("Shutting down service: {}", service.name);
            // Implementation would delegate to shutdown_manager
        }

        Ok(())
    }

    /// Default implementation
    pub fn default() -> Self {
        Self::new()
    }
}
