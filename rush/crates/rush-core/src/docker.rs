//! Docker client interface for container management
//!
//! This module provides abstractions for interacting with Docker to create,
//! manage, and monitor containers.

use crate::error::Result;
use async_trait::async_trait;
use std::fmt;

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

/// Defines operations that can be performed on Docker containers
#[async_trait]
pub trait DockerClient: Send + Sync + fmt::Debug {
    /// Creates a Docker network
    async fn create_network(&self, name: &str) -> Result<()>;

    /// Deletes a Docker network
    async fn delete_network(&self, name: &str) -> Result<()>;

    /// Checks if a Docker network exists
    async fn network_exists(&self, name: &str) -> Result<bool>;

    /// Pulls a Docker image
    async fn pull_image(&self, image: &str) -> Result<()>;

    /// Builds a Docker image
    async fn build_image(&self, tag: &str, dockerfile: &str, context: &str) -> Result<()>;

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

    /// Runs a container with the specified configuration and optional command
    async fn run_container_with_command(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
        command: Option<&[String]>,
    ) -> Result<String>; // Returns container ID

    /// Stops a running container
    async fn stop_container(&self, container_id: &str) -> Result<()>;

    /// Removes a container
    async fn remove_container(&self, container_id: &str) -> Result<()>;

    /// Gets the status of a container
    async fn container_status(&self, container_id: &str) -> Result<ContainerStatus>;

    /// Checks if a container exists
    async fn container_exists(&self, name: &str) -> Result<bool>;

    /// Gets the logs from a container
    async fn container_logs(&self, container_id: &str, lines: usize) -> Result<String>;

    /// Follows the logs from a container with formatted output
    async fn follow_container_logs(
        &self,
        container_id: &str,
        label: String,
        color: &str,
    ) -> Result<()>;

    /// Sends a signal to a container
    async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()>;

    /// Execute a command in a running container
    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String>;

    /// Get container by name
    async fn get_container_by_name(&self, name: &str) -> Result<String>;
}
