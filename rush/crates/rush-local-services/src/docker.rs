//! Docker client trait for local services
//!
//! This module defines a minimal Docker client interface to avoid
//! circular dependencies with rush-container.

use rush_core::error::Result;
use std::fmt;

/// Minimal Docker client trait for local services
#[async_trait::async_trait]
pub trait DockerClient: Send + Sync + fmt::Debug {
    /// Runs a container with the specified configuration
    async fn run_container(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
    ) -> Result<String>; // Returns container ID

    /// Stops a running container
    async fn stop_container(&self, container_id: &str) -> Result<()>;

    /// Removes a container
    async fn remove_container(&self, container_id: &str) -> Result<()>;

    /// Gets the status of a container
    async fn container_status(&self, container_id: &str) -> Result<ContainerStatus>;

    /// Gets the logs from a container
    async fn container_logs(&self, container_id: &str, lines: usize) -> Result<String>;

    /// Execute a command in a running container
    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String>;

    /// Get container by name
    async fn get_container_by_name(&self, name: &str) -> Result<String>;
}

/// Status of a Docker container
#[derive(Debug, Clone, PartialEq)]
pub enum ContainerStatus {
    /// Container is running
    Running,
    /// Container has exited with a status code
    Exited(i32),
    /// Container status couldn't be determined
    Unknown,
}
