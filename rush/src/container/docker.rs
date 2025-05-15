//! Docker client interface for container management
//!
//! This module provides abstractions for interacting with Docker to create,
//! manage, and monitor containers.

use crate::error::{Error, Result};
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::fmt;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;

/// Defines operations that can be performed on Docker containers
#[async_trait::async_trait]
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

    /// Sends a signal to a container
    async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()>;
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

/// Health status of a container
#[derive(Debug, Clone, PartialEq)]
pub struct HealthStatus {
    /// Whether the container is healthy according to health checks
    healthy: bool,
    /// Detailed status information
    status_info: String,
}

impl HealthStatus {
    /// Creates a new health status
    pub fn new(healthy: bool, status_info: String) -> Self {
        Self {
            healthy,
            status_info,
        }
    }

    /// Returns whether the container is healthy
    pub fn is_healthy(&self) -> bool {
        self.healthy
    }

    /// Returns detailed status information
    pub fn status(&self) -> &str {
        &self.status_info
    }
}

/// Implementation of DockerClient using the docker CLI
#[derive(Debug)]
pub struct DockerCliClient {
    /// Path to the docker executable
    docker_path: String,
}

impl DockerCliClient {
    /// Creates a new DockerCliClient
    pub fn new(docker_path: String) -> Self {
        Self { docker_path }
    }
}

#[async_trait::async_trait]
impl DockerClient for DockerCliClient {
    async fn create_network(&self, name: &str) -> Result<()> {
        trace!("Creating Docker network: {}", name);

        let output = Command::new(&self.docker_path)
            .args(["network", "create", "-d", "bridge", name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to create network: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to create Docker network: {}", stderr);
            return Err(Error::Docker(format!(
                "Network creation failed: {}",
                stderr
            )));
        }

        debug!("Successfully created Docker network: {}", name);
        Ok(())
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        trace!("Deleting Docker network: {}", name);

        let output = Command::new(&self.docker_path)
            .args(["network", "rm", name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to delete network: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't treat this as an error if the network doesn't exist
            if stderr.contains("No such network") {
                debug!("Network {} already removed", name);
                return Ok(());
            }
            error!("Failed to delete Docker network: {}", stderr);
            return Err(Error::Docker(format!(
                "Network deletion failed: {}",
                stderr
            )));
        }

        debug!("Successfully deleted Docker network: {}", name);
        Ok(())
    }

    async fn network_exists(&self, name: &str) -> Result<bool> {
        trace!("Checking if Docker network exists: {}", name);

        let output = Command::new(&self.docker_path)
            .args(["network", "inspect", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|e| Error::Docker(format!("Failed to check network: {}", e)))?;

        Ok(output.success())
    }

    async fn pull_image(&self, image: &str) -> Result<()> {
        trace!("Pulling Docker image: {}", image);

        let output = Command::new(&self.docker_path)
            .args(["pull", image])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to pull image: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to pull Docker image: {}", stderr);
            return Err(Error::Docker(format!("Image pull failed: {}", stderr)));
        }

        debug!("Successfully pulled Docker image: {}", image);
        Ok(())
    }

    async fn build_image(&self, tag: &str, dockerfile: &str, context: &str) -> Result<()> {
        trace!("Building Docker image: {}", tag);

        let output = Command::new(&self.docker_path)
            .args(["build", "--tag", tag, "--file", dockerfile, context])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to build image: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to build Docker image: {}", stderr);
            return Err(Error::Docker(format!("Image build failed: {}", stderr)));
        }

        debug!("Successfully built Docker image: {}", tag);
        Ok(())
    }

    async fn run_container(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
    ) -> Result<String> {
        trace!("Running Docker container {} from image {}", name, image);

        let mut args = vec!["run", "-d", "--name", name, "--network", network];

        // Add environment variables
        for env in env_vars {
            args.push("-e");
            args.push(env);
        }

        // Add port mappings
        for port in ports {
            args.push("-p");
            args.push(port);
        }

        // Add volume mappings
        for volume in volumes {
            args.push("-v");
            args.push(volume);
        }

        // Add image name at the end
        args.push(image);

        let output = Command::new(&self.docker_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to run container: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to run Docker container: {}", stderr);
            return Err(Error::Docker(format!("Container run failed: {}", stderr)));
        }

        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info!(
            "Successfully started Docker container: {} (ID: {})",
            name, container_id
        );
        Ok(container_id)
    }

    async fn stop_container(&self, container_id: &str) -> Result<()> {
        trace!("Stopping Docker container: {}", container_id);

        let output = Command::new(&self.docker_path)
            .args(["stop", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to stop container: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't treat as error if container is already stopped
            if stderr.contains("No such container") {
                debug!("Container {} already stopped", container_id);
                return Ok(());
            }
            error!("Failed to stop Docker container: {}", stderr);
            return Err(Error::Docker(format!("Container stop failed: {}", stderr)));
        }

        debug!("Successfully stopped Docker container: {}", container_id);
        Ok(())
    }

    async fn remove_container(&self, container_id: &str) -> Result<()> {
        trace!("Removing Docker container: {}", container_id);

        let output = Command::new(&self.docker_path)
            .args(["rm", "-f", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to remove container: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't treat as error if container is already removed
            if stderr.contains("No such container") {
                debug!("Container {} already removed", container_id);
                return Ok(());
            }
            error!("Failed to remove Docker container: {}", stderr);
            return Err(Error::Docker(format!(
                "Container removal failed: {}",
                stderr
            )));
        }

        debug!("Successfully removed Docker container: {}", container_id);
        Ok(())
    }

    async fn container_status(&self, container_id: &str) -> Result<ContainerStatus> {
        trace!("Getting status for Docker container: {}", container_id);

        let output = Command::new(&self.docker_path)
            .args([
                "inspect",
                "--format={{.State.Status}},{{.State.ExitCode}}",
                container_id,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to inspect container: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such container") {
                warn!("Container {} not found", container_id);
                return Ok(ContainerStatus::Unknown);
            }
            error!("Failed to inspect Docker container: {}", stderr);
            return Err(Error::Docker(format!(
                "Container inspection failed: {}",
                stderr
            )));
        }

        let status_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let parts: Vec<&str> = status_str.split(',').collect();

        match parts.get(0) {
            Some(&"running") => Ok(ContainerStatus::Running),
            Some(&"exited") => {
                if let Some(exit_code) = parts.get(1) {
                    if let Ok(code) = exit_code.parse::<i32>() {
                        return Ok(ContainerStatus::Exited(code));
                    }
                }
                Ok(ContainerStatus::Exited(0))
            }
            _ => Ok(ContainerStatus::Unknown),
        }
    }

    async fn container_exists(&self, name: &str) -> Result<bool> {
        trace!("Checking if Docker container exists: {}", name);

        let output = Command::new(&self.docker_path)
            .args([
                "ps",
                "-a",
                "--filter",
                &format!("name={}", name),
                "--format",
                "{{.Names}}",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to list containers: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to check Docker container existence: {}", stderr);
            return Err(Error::Docker(format!("Container check failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(!stdout.is_empty())
    }

    async fn container_logs(&self, container_id: &str, lines: usize) -> Result<String> {
        trace!("Getting logs for Docker container: {}", container_id);

        let output = Command::new(&self.docker_path)
            .args(["logs", "--tail", &lines.to_string(), container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to get container logs: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to get Docker container logs: {}", stderr);
            return Err(Error::Docker(format!(
                "Container logs retrieval failed: {}",
                stderr
            )));
        }

        let logs = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(logs)
    }

    async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()> {
        trace!(
            "Sending signal {} to Docker container: {}",
            signal,
            container_id
        );

        let output = Command::new(&self.docker_path)
            .args(["kill", "--signal", &signal.to_string(), container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to send signal to container: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't treat as error if container is already gone
            if stderr.contains("No such container") {
                debug!("Container {} not found when sending signal", container_id);
                return Ok(());
            }
            error!("Failed to send signal to Docker container: {}", stderr);
            return Err(Error::Docker(format!(
                "Container signal failed: {}",
                stderr
            )));
        }

        debug!(
            "Successfully sent signal {} to Docker container: {}",
            signal, container_id
        );
        Ok(())
    }
}

/// Configuration for a Docker service
#[derive(Debug, Clone)]
pub struct DockerServiceConfig {
    /// Name of the service
    pub name: String,
    /// Docker image to use
    pub image: String,
    /// Network to connect to
    pub network: String,
    /// Environment variables
    pub env_vars: HashMap<String, String>,
    /// Port mappings (host:container)
    pub ports: Vec<String>,
    /// Volume mappings (host:container)
    pub volumes: Vec<String>,
}

/// Represents a running Docker service
#[derive(Clone)]
pub struct DockerService {
    /// Container ID
    pub id: String,
    /// Configuration used to create the service
    pub config: DockerServiceConfig,
    /// Client for interacting with Docker
    client: Arc<dyn DockerClient>,
}

impl fmt::Debug for DockerService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DockerService")
            .field("id", &self.id)
            .field("config", &self.config)
            .field("client", &"<DockerClient>")
            .finish()
    }
}

impl DockerService {
    /// Creates a new DockerService
    pub fn new(id: String, config: DockerServiceConfig, client: Arc<dyn DockerClient>) -> Self {
        Self { id, config, client }
    }

    /// Stops the service
    pub async fn stop(&self) -> Result<()> {
        self.client.stop_container(&self.id).await
    }

    /// Removes the service
    pub async fn remove(&self) -> Result<()> {
        self.client.remove_container(&self.id).await
    }

    /// Gets the service status
    pub async fn status(&self) -> Result<ContainerStatus> {
        self.client.container_status(&self.id).await
    }

    /// Gets the service logs
    pub async fn logs(&self, lines: usize) -> Result<String> {
        self.client.container_logs(&self.id, lines).await
    }

    /// Returns the container ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Sends a signal to the container
    pub async fn send_signal(&mut self, signal: i32) -> Result<()> {
        self.client.send_signal_to_container(&self.id, signal).await
    }

    /// Checks if the service is running
    pub async fn is_running(&self) -> Result<bool> {
        match self.client.container_status(&self.id).await? {
            ContainerStatus::Running => Ok(true),
            _ => Ok(false),
        }
    }

    /// Gets the exit code of the container if it has exited
    pub async fn exit_code(&self) -> Result<Option<i32>> {
        match self.client.container_status(&self.id).await? {
            ContainerStatus::Exited(code) => Ok(Some(code)),
            _ => Ok(None),
        }
    }

    /// Gets the health status of the container
    pub fn health_status(&self) -> Option<HealthStatus> {
        // This would normally query Docker for health check status
        // Simplified implementation for now
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;
    use mockall::*;

    mock! {
        DockerClient {}

        #[async_trait]
        impl DockerClient for DockerClient {
            async fn create_network(&self, name: &str) -> Result<()>;
            async fn delete_network(&self, name: &str) -> Result<()>;
            async fn network_exists(&self, name: &str) -> Result<bool>;
            async fn pull_image(&self, image: &str) -> Result<()>;
            async fn build_image(&self, tag: &str, dockerfile: &str, context: &str) -> Result<()>;
            async fn run_container(
                &self,
                image: &str,
                name: &str,
                network: &str,
                env_vars: &[String],
                ports: &[String],
                volumes: &[String],
            ) -> Result<String>;
            async fn stop_container(&self, container_id: &str) -> Result<()>;
            async fn remove_container(&self, container_id: &str) -> Result<()>;
            async fn container_status(&self, container_id: &str) -> Result<ContainerStatus>;
            async fn container_exists(&self, name: &str) -> Result<bool>;
            async fn container_logs(&self, container_id: &str, lines: usize) -> Result<String>;
            async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()>;
        }
    }

    #[tokio::test]
    async fn test_docker_service_stop() {
        let mut mock = MockDockerClient::new();
        mock.expect_stop_container()
            .with(eq("test-container"))
            .times(1)
            .returning(|_| Ok(()));

        let config = DockerServiceConfig {
            name: "test".to_string(),
            image: "nginx".to_string(),
            network: "test-network".to_string(),
            env_vars: HashMap::new(),
            ports: vec![],
            volumes: vec![],
        };

        let service = DockerService {
            id: "test-container".to_string(),
            config,
            client: Arc::new(mock),
        };

        let result = service.stop().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_docker_service_status() {
        let mut mock = MockDockerClient::new();
        mock.expect_container_status()
            .with(eq("test-container"))
            .times(1)
            .returning(|_| Ok(ContainerStatus::Running));

        let config = DockerServiceConfig {
            name: "test".to_string(),
            image: "nginx".to_string(),
            network: "test-network".to_string(),
            env_vars: HashMap::new(),
            ports: vec![],
            volumes: vec![],
        };

        let service = DockerService {
            id: "test-container".to_string(),
            config,
            client: Arc::new(mock),
        };

        let result = service.status().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ContainerStatus::Running);
    }
}
