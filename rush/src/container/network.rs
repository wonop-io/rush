//! Docker network management for containers
//!
//! This module provides utilities for creating, checking, and managing Docker networks
//! that are used to connect containers.

use crate::container::DockerClient;
use crate::error::Result;
use crate::utils::run_command;
use colored::Colorize;
use log::{debug, trace, warn};
use std::process::Command;
use std::sync::Arc;

/// Represents a Docker network configuration
pub struct DockerNetwork {
    /// Name of the Docker network
    name: String,
    /// The Docker executable path
    docker_cmd: String,
}

impl DockerNetwork {
    /// Creates a new DockerNetwork instance
    ///
    /// # Arguments
    ///
    /// * `name` - The name to use for the Docker network
    /// * `docker_cmd` - The path to the Docker executable
    pub fn new(name: &str, docker_cmd: &str) -> Self {
        DockerNetwork {
            name: name.to_string(),
            docker_cmd: docker_cmd.to_string(),
        }
    }

    /// Checks if the network exists
    ///
    /// # Returns
    ///
    /// `true` if the network exists, `false` otherwise
    pub fn exists(&self) -> bool {
        trace!("Checking if Docker network '{}' exists", self.name);

        let output = Command::new(&self.docker_cmd)
            .args(["network", "inspect", &self.name])
            .output();

        match output {
            Ok(output) => {
                debug!("Docker network inspect exit status: {}", output.status);
                output.status.success()
            }
            Err(e) => {
                warn!("Error checking Docker network: {}", e);
                false
            }
        }
    }

    /// Creates the Docker network if it doesn't already exist
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure
    pub async fn create(&self) -> Result<()> {
        if self.exists() {
            trace!(
                "Docker network '{}' already exists. Skipping creation.",
                self.name
            );
            return Ok(());
        }

        trace!("Creating Docker network: {}", self.name);

        run_command(
            &"docker network".white().to_string(),
            &self.docker_cmd,
            vec!["network", "create", "-d", "bridge", &self.name],
        )
        .await?;
        Ok(())
    }

    /// Deletes the Docker network if it exists
    ///
    /// # Returns
    ///
    /// A Result indicating success or failure
    pub async fn delete(&self) -> Result<()> {
        if !self.exists() {
            trace!(
                "Docker network '{}' does not exist. Skipping deletion.",
                self.name
            );
            return Ok(());
        }

        trace!("Deleting Docker network: {}", self.name);

        run_command(
            &"docker network".white().to_string(),
            &self.docker_cmd,
            vec!["network", "rm", &self.name],
        )
        .await?;
        Ok(())
    }

    /// Returns the name of this network
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Sets up a network for container communication
pub async fn setup_network(name: &str, docker_client: &Arc<dyn DockerClient>) -> Result<()> {
    if !docker_client.network_exists(name).await? {
        debug!("Creating Docker network: {}", name);
        docker_client.create_network(name).await?;
    } else {
        debug!("Docker network '{}' already exists", name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_docker_network() {
        let network = DockerNetwork::new("test-network", "/usr/bin/docker");
        assert_eq!(network.name(), "test-network");
    }

    // Integration tests requiring Docker would be here
    // These would typically be marked with #[ignore] to avoid running in CI
    // unless Docker is available
}
