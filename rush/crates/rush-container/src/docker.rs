//! Docker client interface for container management
//!
//! This module provides abstractions for interacting with Docker to create,
//! manage, and monitor containers.

use rush_core::error::{Error, Result};
use rush_output::{OutputDirector, OutputSource, OutputStream};
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
            .map_err(|e| Error::Docker(format!("Failed to create network: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to create Docker network: {}", stderr);
            return Err(Error::Docker(format!("Network creation failed: {stderr}")));
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
            .map_err(|e| Error::Docker(format!("Failed to delete network: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't treat this as an error if the network doesn't exist
            if stderr.contains("No such network") {
                debug!("Network {} already removed", name);
                return Ok(());
            }
            error!("Failed to delete Docker network: {}", stderr);
            return Err(Error::Docker(format!("Network deletion failed: {stderr}")));
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
            .map_err(|e| Error::Docker(format!("Failed to check network: {e}")))?;

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
            .map_err(|e| Error::Docker(format!("Failed to pull image: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to pull Docker image: {}", stderr);
            return Err(Error::Docker(format!("Image pull failed: {stderr}")));
        }

        debug!("Successfully pulled Docker image: {}", image);
        Ok(())
    }

    /// Builds a Docker image from a Dockerfile and context directory.
    ///
    /// This method:
    /// 1. Changes to the context directory using the Directory RAII guard
    /// 2. Calculates the relative path from context to Dockerfile
    /// 3. Executes docker build with the specified tag
    /// 4. Automatically restores the original directory when done
    ///
    /// # Arguments
    ///
    /// * `tag` - The tag to apply to the built image (e.g., "myapp:latest")
    /// * `dockerfile` - Path to the Dockerfile (can be absolute or relative)
    /// * `context` - The build context directory containing files for the Docker build
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the build succeeds
    /// * `Err(Error::Docker)` if the build fails with details about the failure
    ///
    /// # Important
    ///
    /// Uses the Directory RAII guard to safely change directories. The original
    /// directory is always restored, even if the build fails or panics.
    async fn build_image(&self, tag: &str, dockerfile: &str, context: &str) -> Result<()> {
        use std::path::Path;

        trace!("Building Docker image: {}", tag);

        // Convert paths to PathBuf for manipulation
        let context_path = Path::new(context);
        let dockerfile_path = Path::new(dockerfile);

        // Calculate relative path from context to Dockerfile
        let dockerfile_relative = if dockerfile_path.is_absolute() && context_path.is_absolute() {
            // Try to make dockerfile relative to context
            dockerfile_path
                .strip_prefix(context_path)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|_| {
                    // If not a child of context, use absolute path
                    dockerfile_path.to_path_buf()
                })
        } else {
            dockerfile_path.to_path_buf()
        };

        let dockerfile_arg = dockerfile_relative.to_string_lossy();

        info!("Docker build: Running from directory '{}'", context);
        info!(
            "Docker build command: docker build --tag {} --file {} .",
            tag, dockerfile_arg
        );

        // Use the Directory guard to change to the build context directory
        let _dir_guard = rush_utils::Directory::chdir(context);

        let output = Command::new(&self.docker_path)
            .args([
                "build",
                "--platform",
                "linux/amd64", // Always build for x86_64
                "--tag",
                tag,
                "--file",
                dockerfile_arg.as_ref(),
                ".",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to execute docker build: {e}")))?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            error!("\n=== Docker Build Failed ===");
            error!("Image tag: {}", tag);
            error!("Working directory: {}", context);
            error!("Dockerfile (relative): {}", dockerfile_arg);
            error!("Dockerfile (absolute): {}", dockerfile);
            error!("Exit code: {:?}", output.status.code());

            if !stdout.is_empty() {
                error!("\n=== Build Output ===");
                for line in stdout.lines() {
                    error!("  {}", line);
                }
            }

            if !stderr.is_empty() {
                error!("\n=== Error Output ===");
                for line in stderr.lines() {
                    error!("  {}", line);
                }
            }

            error!("\n=== Troubleshooting ===");
            error!("1. Check if the Dockerfile exists at: {}", dockerfile);
            error!("2. Verify the build context directory: {}", context);
            error!("3. Ensure Docker daemon is running: docker ps");
            error!("4. Check Docker disk space: docker system df");
            error!(
                "5. Try building manually: cd {} && docker build --tag {} --file {} .",
                context, tag, dockerfile_arg
            );

            // Create a more informative error message
            let error_summary = if stderr.contains("no such file or directory") {
                "Dockerfile or build context not found"
            } else if stderr.contains("permission denied") {
                "Permission denied - check Docker permissions"
            } else if stderr.contains("no space left") {
                "No space left on device - clean up Docker images/containers"
            } else if stderr.contains("network") {
                "Network error - check internet connection for base image pulls"
            } else {
                "Build failed - see detailed output above"
            };

            return Err(Error::Docker(format!(
                "Docker build failed for {tag}: {error_summary}"
            )));
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

        let mut args = vec![
            "run",
            "-d",
            "--platform",
            "linux/amd64", // Always run as x86_64
            "--name",
            name,
            "--network",
            network,
        ];

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
        println!("RUNNING: {}", args.join(" "));
        let output = Command::new(&self.docker_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to run container: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to run Docker container: {}", stderr);
            return Err(Error::Docker(format!("Container run failed: {stderr}")));
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
            .map_err(|e| Error::Docker(format!("Failed to stop container: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't treat as error if container is already stopped
            if stderr.contains("No such container") {
                debug!("Container {} already stopped", container_id);
                return Ok(());
            }
            error!("Failed to stop Docker container: {}", stderr);
            return Err(Error::Docker(format!("Container stop failed: {stderr}")));
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
            .map_err(|e| Error::Docker(format!("Failed to remove container: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't treat as error if container is already removed
            if stderr.contains("No such container") {
                debug!("Container {} already removed", container_id);
                return Ok(());
            }
            error!("Failed to remove Docker container: {}", stderr);
            return Err(Error::Docker(format!("Container removal failed: {stderr}")));
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
            .map_err(|e| Error::Docker(format!("Failed to inspect container: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No such container") {
                warn!("Container {} not found", container_id);
                return Ok(ContainerStatus::Unknown);
            }
            error!("Failed to inspect Docker container: {}", stderr);
            return Err(Error::Docker(format!(
                "Container inspection failed: {stderr}"
            )));
        }

        let status_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let parts: Vec<&str> = status_str.split(',').collect();

        match parts.first() {
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
                &format!("name={name}"),
                "--format",
                "{{.Names}}",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to list containers: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to check Docker container existence: {}", stderr);
            return Err(Error::Docker(format!("Container check failed: {stderr}")));
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
            .map_err(|e| Error::Docker(format!("Failed to get container logs: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to get Docker container logs: {}", stderr);
            return Err(Error::Docker(format!(
                "Container logs retrieval failed: {stderr}"
            )));
        }

        let logs = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(logs)
    }

    async fn follow_container_logs(
        &self,
        container_id: &str,
        label: String,
        color: &str,
    ) -> Result<()> {
        trace!("Following logs for Docker container: {}", container_id);

        use colored::Colorize;

        // Format label with padding (similar to old implementation)
        let formatted_label = format!("{label:15}").color(color).bold();

        // Use docker logs -f to follow the container logs
        let mut child = Command::new(&self.docker_path)
            .args(["logs", "-f", "--tail", "10", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Docker(format!("Failed to follow container logs: {e}")))?;

        // Get the streams
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Spawn tasks to handle stdout and stderr
        if let Some(stdout) = stdout {
            let label_clone = formatted_label.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            // Print with formatted label prefix
                            print!("{label_clone} | {line}");
                        }
                        Err(e) => {
                            error!("Error reading stdout: {}", e);
                            break;
                        }
                    }
                }
            });
        }

        if let Some(stderr) = stderr {
            let label_clone = formatted_label;
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            // Print with formatted label prefix to stderr
                            eprint!("{label_clone} | {line}");
                        }
                        Err(e) => {
                            error!("Error reading stderr: {}", e);
                            break;
                        }
                    }
                }
            });
        }

        Ok(())
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
            .map_err(|e| Error::Docker(format!("Failed to send signal to container: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't treat as error if container is already gone
            if stderr.contains("No such container") {
                debug!("Container {} not found when sending signal", container_id);
                return Ok(());
            }
            error!("Failed to send signal to Docker container: {}", stderr);
            return Err(Error::Docker(format!("Container signal failed: {stderr}")));
        }

        debug!(
            "Successfully sent signal {} to Docker container: {}",
            signal, container_id
        );
        Ok(())
    }
    
    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
        trace!("Executing command in container {}: {:?}", container_id, command);
        
        let mut args = vec!["exec", container_id];
        args.extend(command);
        
        let output = Command::new(&self.docker_path)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to exec in container: {e}")))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Docker(format!("Command failed: {}", stderr)));
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    
    async fn get_container_by_name(&self, name: &str) -> Result<String> {
        trace!("Getting container by name: {}", name);
        
        let output = Command::new(&self.docker_path)
            .args(["ps", "-aq", "--filter", &format!("name={}", name)])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to get container by name: {e}")))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Docker(format!("Failed to get container: {}", stderr)));
        }
        
        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if container_id.is_empty() {
            return Err(Error::Docker(format!("Container '{}' not found", name)));
        }
        
        Ok(container_id)
    }
}

impl DockerCliClient {
    /// Follows the logs from a container using an OutputDirector (specific to DockerCliClient)
    pub async fn follow_logs_with_director<T: OutputDirector>(
        &self,
        container_id: &str,
        source: OutputSource,
        director: &mut T,
    ) -> Result<()> {
        trace!(
            "Following logs for Docker container with director: {}",
            container_id
        );

        // Use docker logs -f to follow the container logs
        let mut child = Command::new(&self.docker_path)
            .args(["logs", "-f", "--tail", "10", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Docker(format!("Failed to follow container logs: {e}")))?;

        // Get the streams
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Create channels to communicate with the director
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(OutputSource, OutputStream)>(100);

        // Handle stdout
        if let Some(stdout) = stdout {
            let source_clone = source.clone();
            let tx_stdout = tx.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let output_data = line.as_bytes().to_vec();
                            let stream = OutputStream::stdout(output_data);
                            if let Err(_) = tx_stdout.send((source_clone.clone(), stream)).await {
                                break; // Receiver dropped
                            }
                        }
                        Err(e) => {
                            error!("Error reading stdout: {}", e);
                            break;
                        }
                    }
                }
            });
        }

        // Handle stderr
        if let Some(stderr) = stderr {
            let source_clone = source.clone();
            let tx_stderr = tx.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let output_data = line.as_bytes().to_vec();
                            let stream = OutputStream::stderr(output_data);
                            if let Err(_) = tx_stderr.send((source_clone.clone(), stream)).await {
                                break; // Receiver dropped
                            }
                        }
                        Err(e) => {
                            error!("Error reading stderr: {}", e);
                            break;
                        }
                    }
                }
            });
        }

        // Drop the original sender so the receiver will eventually close
        drop(tx);

        // Process messages from the streams and send them to the director
        while let Some((msg_source, stream)) = rx.recv().await {
            if let Err(e) = director.write_output(&msg_source, &stream).await {
                error!("Error writing to output director: {}", e);
                break;
            }
        }

        // Wait for child process to complete
        let _ = child.wait().await;

        // Flush any remaining output
        director.flush().await?;

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
    
    /// Returns the service name
    pub fn name(&self) -> Option<String> {
        Some(self.config.name.clone())
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

// Type alias for backward compatibility with old code
pub type DockerImage = crate::image_builder::ImageBuilder;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_docker_service_stop() {
        // Create a mock using a simple test double approach
        struct MockDockerClient {
            stop_container_called: std::sync::Mutex<bool>,
        }

        impl fmt::Debug for MockDockerClient {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct("MockDockerClient").finish()
            }
        }

        #[async_trait::async_trait]
        impl DockerClient for MockDockerClient {
            async fn create_network(&self, _name: &str) -> Result<()> {
                unimplemented!()
            }
            async fn delete_network(&self, _name: &str) -> Result<()> {
                unimplemented!()
            }
            async fn network_exists(&self, _name: &str) -> Result<bool> {
                unimplemented!()
            }
            async fn pull_image(&self, _image: &str) -> Result<()> {
                unimplemented!()
            }
            async fn build_image(
                &self,
                _tag: &str,
                _dockerfile: &str,
                _context: &str,
            ) -> Result<()> {
                unimplemented!()
            }
            async fn run_container(
                &self,
                _image: &str,
                _name: &str,
                _network: &str,
                _env_vars: &[String],
                _ports: &[String],
                _volumes: &[String],
            ) -> Result<String> {
                unimplemented!()
            }
            async fn stop_container(&self, container_id: &str) -> Result<()> {
                assert_eq!(container_id, "test-container");
                let mut called = self.stop_container_called.lock().unwrap();
                *called = true;
                Ok(())
            }
            async fn remove_container(&self, _container_id: &str) -> Result<()> {
                unimplemented!()
            }
            async fn container_status(&self, _container_id: &str) -> Result<ContainerStatus> {
                unimplemented!()
            }
            async fn container_exists(&self, _name: &str) -> Result<bool> {
                unimplemented!()
            }
            async fn container_logs(&self, _container_id: &str, _lines: usize) -> Result<String> {
                unimplemented!()
            }
            async fn follow_container_logs(
                &self,
                _container_id: &str,
                _label: String,
                _color: &str,
            ) -> Result<()> {
                unimplemented!()
            }
            async fn send_signal_to_container(
                &self,
                _container_id: &str,
                _signal: i32,
            ) -> Result<()> {
                unimplemented!()
            }
            async fn exec_in_container(&self, _container_id: &str, _command: &[&str]) -> Result<String> {
                unimplemented!()
            }
            async fn get_container_by_name(&self, _name: &str) -> Result<String> {
                unimplemented!()
            }
        }

        let mock = Arc::new(MockDockerClient {
            stop_container_called: std::sync::Mutex::new(false),
        });

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
            client: mock.clone(),
        };

        let result = service.stop().await;
        assert!(result.is_ok());
        assert!(*mock.stop_container_called.lock().unwrap());
    }

    #[tokio::test]
    async fn test_docker_service_status() {
        // Create a mock using a simple test double approach
        struct MockDockerClient {
            container_status_called: std::sync::Mutex<bool>,
        }

        impl fmt::Debug for MockDockerClient {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct("MockDockerClient").finish()
            }
        }

        #[async_trait::async_trait]
        impl DockerClient for MockDockerClient {
            async fn create_network(&self, _name: &str) -> Result<()> {
                unimplemented!()
            }
            async fn delete_network(&self, _name: &str) -> Result<()> {
                unimplemented!()
            }
            async fn network_exists(&self, _name: &str) -> Result<bool> {
                unimplemented!()
            }
            async fn pull_image(&self, _image: &str) -> Result<()> {
                unimplemented!()
            }
            async fn build_image(
                &self,
                _tag: &str,
                _dockerfile: &str,
                _context: &str,
            ) -> Result<()> {
                unimplemented!()
            }
            async fn run_container(
                &self,
                _image: &str,
                _name: &str,
                _network: &str,
                _env_vars: &[String],
                _ports: &[String],
                _volumes: &[String],
            ) -> Result<String> {
                unimplemented!()
            }
            async fn stop_container(&self, _container_id: &str) -> Result<()> {
                unimplemented!()
            }
            async fn remove_container(&self, _container_id: &str) -> Result<()> {
                unimplemented!()
            }
            async fn container_status(&self, container_id: &str) -> Result<ContainerStatus> {
                assert_eq!(container_id, "test-container");
                let mut called = self.container_status_called.lock().unwrap();
                *called = true;
                Ok(ContainerStatus::Running)
            }
            async fn container_exists(&self, _name: &str) -> Result<bool> {
                unimplemented!()
            }
            async fn container_logs(&self, _container_id: &str, _lines: usize) -> Result<String> {
                unimplemented!()
            }
            async fn follow_container_logs(
                &self,
                _container_id: &str,
                _label: String,
                _color: &str,
            ) -> Result<()> {
                unimplemented!()
            }
            async fn send_signal_to_container(
                &self,
                _container_id: &str,
                _signal: i32,
            ) -> Result<()> {
                unimplemented!()
            }
            async fn exec_in_container(&self, _container_id: &str, _command: &[&str]) -> Result<String> {
                unimplemented!()
            }
            async fn get_container_by_name(&self, _name: &str) -> Result<String> {
                unimplemented!()
            }
        }

        let mock = Arc::new(MockDockerClient {
            container_status_called: std::sync::Mutex::new(false),
        });

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
            client: mock.clone(),
        };

        let result = service.status().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ContainerStatus::Running);
        assert!(*mock.container_status_called.lock().unwrap());
    }
}
