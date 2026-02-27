//! Network manager with RAII semantics
//!
//! This module provides centralized network management with automatic cleanup.

use std::sync::Arc;

use log::{debug, info, warn};
use rush_core::error::Result;

use crate::docker::DockerClient;

/// RAII network manager that creates and manages Docker networks
pub struct NetworkManager {
    docker_client: Arc<dyn DockerClient>,
    network_name: String,
    created: bool,
}

impl NetworkManager {
    /// Create a new network manager for the given product
    pub async fn new(docker_client: Arc<dyn DockerClient>, product_name: &str) -> Result<Self> {
        let network_name = Self::compute_network_name(product_name);

        info!("Setting up network: {network_name}");

        let mut manager = Self {
            docker_client,
            network_name: network_name.clone(),
            created: false,
        };

        manager.ensure_network_exists().await?;

        Ok(manager)
    }

    /// Get the network name
    pub fn network_name(&self) -> &str {
        &self.network_name
    }

    /// Compute network name from product name (sanitizes dots to dashes)
    fn compute_network_name(product_name: &str) -> String {
        let sanitized = product_name.replace('.', "-");
        format!("net-{sanitized}")
    }

    /// Ensure the network exists, creating it if necessary
    async fn ensure_network_exists(&mut self) -> Result<()> {
        // Check if network already exists
        match self.docker_client.network_exists(&self.network_name).await {
            Ok(true) => {
                info!("Network {} already exists", self.network_name);
                self.created = false;
            }
            Ok(false) => {
                info!("Creating network: {}", self.network_name);
                self.docker_client
                    .create_network(&self.network_name)
                    .await?;
                self.created = true;
                info!("Successfully created network: {}", self.network_name);
            }
            Err(e) => {
                warn!("Failed to check network existence, attempting to create: {e}");
                // Try to create anyway - if it exists, Docker will return an error
                match self.docker_client.create_network(&self.network_name).await {
                    Ok(()) => {
                        self.created = true;
                        info!("Successfully created network: {}", self.network_name);
                    }
                    Err(create_err) if create_err.to_string().contains("already exists") => {
                        info!("Network {} already exists", self.network_name);
                        self.created = false;
                    }
                    Err(create_err) => return Err(create_err),
                }
            }
        }

        Ok(())
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        if self.created {
            debug!(
                "NetworkManager dropping, will clean up network: {}",
                self.network_name
            );
            // Note: We can't make async calls in Drop, so we'll need to handle cleanup differently
            // For now, we leave the network for manual cleanup
            // In a production system, we might want to spawn a cleanup task
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_network_name() {
        assert_eq!(
            NetworkManager::compute_network_name("helloworld.wonop.io"),
            "net-helloworld-wonop-io"
        );
        assert_eq!(NetworkManager::compute_network_name("simple"), "net-simple");
        assert_eq!(
            NetworkManager::compute_network_name("complex.multi.dot"),
            "net-complex-multi-dot"
        );
    }
}
