//! Docker client implementation and utilities
//!
//! This module provides Docker client implementations and service abstractions.

use std::collections::HashMap;
use std::fmt;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info, trace, warn};
use rush_core::error::{Error, Result};
// Re-export the DockerClient trait and ContainerStatus from rush-docker
pub use rush_docker::{ContainerStatus, DockerClient};
use rush_output::{OutputDirector, OutputSource, OutputStream};
use tokio::process::Command;

// Include the enhanced Docker modules
pub mod client_wrapper;
pub mod connection_pool;
pub mod log_streamer;
pub mod metrics;

// Re-export commonly used types from submodules
pub use client_wrapper::{DockerClientWrapper, DockerStats, DockerWrapperConfig};
pub use connection_pool::{ConnectionPool, PoolConfig, PooledDockerClient};
pub use log_streamer::{LogEntry, LogLevel, LogStream, LogStreamConfig, LogStreamManager};
pub use metrics::{MetricsCollector, MetricsReport, OperationType};

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
    /// Target platform for builds and runs (e.g., "linux/amd64" or "linux/arm64")
    platform: String,
}

impl DockerCliClient {
    /// Creates a new DockerCliClient with native platform
    pub fn new(docker_path: String) -> Self {
        Self {
            docker_path,
            platform: rush_core::constants::docker_platform_native().to_string(),
        }
    }

    /// Creates a new DockerCliClient with a specific platform
    pub fn with_platform(docker_path: String, platform: &str) -> Self {
        Self {
            docker_path,
            platform: platform.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl DockerClient for DockerCliClient {
    async fn create_network(&self, name: &str) -> Result<()> {
        trace!("Creating Docker network: {name}");

        let output = Command::new(&self.docker_path)
            .args(["network", "create", "-d", "bridge", name])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to create network: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to create Docker network: {stderr}");
            return Err(Error::Docker(format!("Network creation failed: {stderr}")));
        }

        debug!("Successfully created Docker network: {name}");
        Ok(())
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        trace!("Deleting Docker network: {name}");

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
                debug!("Network {name} already removed");
                return Ok(());
            }
            error!("Failed to delete Docker network: {stderr}");
            return Err(Error::Docker(format!("Network deletion failed: {stderr}")));
        }

        debug!("Successfully deleted Docker network: {name}");
        Ok(())
    }

    async fn network_exists(&self, name: &str) -> Result<bool> {
        trace!("Checking if Docker network exists: {name}");

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
        trace!("Pulling Docker image: {image}");

        let output = Command::new(&self.docker_path)
            .args(["pull", image])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to pull image: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to pull Docker image: {stderr}");
            return Err(Error::Docker(format!("Image pull failed: {stderr}")));
        }

        debug!("Successfully pulled Docker image: {image}");
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

        trace!("Building Docker image: {tag}");
        let build_start = std::time::Instant::now();

        // Convert paths to PathBuf for manipulation
        let _path_calc_start = std::time::Instant::now();
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

        info!("Docker build: Running from directory '{context}'");
        info!("Docker build command: docker build --tag {tag} --file {dockerfile_arg} .");

        // Use the Directory guard to change to the build context directory
        let _dir_guard = rush_utils::Directory::chdir(context);

        let output = Command::new(&self.docker_path)
            .args([
                "build",
                "--platform",
                &self.platform,
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
            error!("Image tag: {tag}");
            error!("Working directory: {context}");
            error!("Dockerfile (relative): {dockerfile_arg}");
            error!("Dockerfile (absolute): {dockerfile}");
            error!("Exit code: {:?}", output.status.code());

            if !stdout.is_empty() {
                error!("\n=== Build Output ===");
                for line in stdout.lines() {
                    error!("  {line}");
                }
            }

            if !stderr.is_empty() {
                error!("\n=== Error Output ===");
                for line in stderr.lines() {
                    error!("  {line}");
                }
            }

            error!("\n=== Troubleshooting ===");
            error!("1. Check if the Dockerfile exists at: {dockerfile}");
            error!("2. Verify the build context directory: {context}");
            error!("3. Ensure Docker daemon is running: docker ps");
            error!("4. Check Docker disk space: docker system df");
            error!(
                "5. Try building manually: cd {context} && docker build --tag {tag} --file {dockerfile_arg} ."
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

        debug!("Successfully built Docker image: {tag}");

        crate::profiling::global_tracker()
            .record_with_component("docker_build", "total", build_start.elapsed())
            .await;

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
        self.run_container_with_command(image, name, network, env_vars, ports, volumes, None)
            .await
    }

    async fn run_container_with_command(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
        command: Option<&[String]>,
    ) -> Result<String> {
        trace!("Running Docker container {name} from image {image}");

        let mut args = vec![
            "run",
            "-d",
            "-i", // Keep STDIN open
            "-t", // Allocate a pseudo-TTY to preserve colors
            "--platform",
            &self.platform,
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

        // Add command if specified
        if let Some(cmd) = command {
            for arg in cmd {
                args.push(arg);
            }
        }

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
            error!("Failed to run Docker container: {stderr}");
            return Err(Error::Docker(format!("Container run failed: {stderr}")));
        }

        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info!("Successfully started Docker container: {name} (ID: {container_id})");
        Ok(container_id)
    }

    async fn build_image_with_platform(
        &self,
        tag: &str,
        dockerfile: &str,
        context: &str,
        platform: &str,
    ) -> Result<()> {
        use std::path::Path;

        trace!("Building Docker image: {tag} for platform: {platform}");
        let build_start = std::time::Instant::now();

        let context_path = Path::new(context);
        let dockerfile_path = Path::new(dockerfile);

        let dockerfile_relative = if dockerfile_path.is_absolute() && context_path.is_absolute() {
            dockerfile_path
                .strip_prefix(context_path)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|_| dockerfile_path.to_path_buf())
        } else {
            dockerfile_path.to_path_buf()
        };

        let dockerfile_arg = dockerfile_relative.to_string_lossy();

        info!("Docker build: Running from directory '{context}' for platform '{platform}'");
        info!("Docker build command: docker build --platform {platform} --tag {tag} --file {dockerfile_arg} .");

        let _dir_guard = rush_utils::Directory::chdir(context);

        let output = Command::new(&self.docker_path)
            .args([
                "build",
                "--platform",
                platform,
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
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Docker build failed for {tag}: {stderr}");
            return Err(Error::Docker(format!("Docker build failed for {tag}")));
        }

        debug!("Successfully built Docker image: {tag} for platform: {platform}");

        crate::profiling::global_tracker()
            .record_with_component("docker_build", "total", build_start.elapsed())
            .await;

        Ok(())
    }

    async fn run_container_with_platform(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
        command: Option<&[String]>,
        platform: &str,
    ) -> Result<String> {
        trace!("Running Docker container {name} from image {image} on platform {platform}");

        let mut args = vec![
            "run",
            "-d",
            "-i",
            "-t",
            "--platform",
            platform,
            "--name",
            name,
            "--network",
            network,
        ];

        for env in env_vars {
            args.push("-e");
            args.push(env);
        }

        for port in ports {
            args.push("-p");
            args.push(port);
        }

        for volume in volumes {
            args.push("-v");
            args.push(volume);
        }

        args.push(image);

        if let Some(cmd) = command {
            for arg in cmd {
                args.push(arg);
            }
        }

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
            error!("Failed to run Docker container: {stderr}");
            return Err(Error::Docker(format!("Container run failed: {stderr}")));
        }

        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        info!("Successfully started Docker container: {name} (ID: {container_id}) on platform: {platform}");
        Ok(container_id)
    }

    fn target_platform(&self) -> &str {
        &self.platform
    }

    async fn stop_container(&self, container_id: &str) -> Result<()> {
        trace!("Stopping Docker container: {container_id}");

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
                debug!("Container {container_id} already stopped");
                return Ok(());
            }
            error!("Failed to stop Docker container: {stderr}");
            return Err(Error::Docker(format!("Container stop failed: {stderr}")));
        }

        debug!("Successfully stopped Docker container: {container_id}");
        Ok(())
    }

    async fn kill_container(&self, container_id: &str) -> Result<()> {
        trace!("Force killing Docker container: {container_id}");

        let output = Command::new(&self.docker_path)
            .args(["kill", container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to kill container: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Don't treat as error if container doesn't exist or is already stopped
            if stderr.contains("No such container") || stderr.contains("is not running") {
                debug!("Container {container_id} already stopped or doesn't exist");
                return Ok(());
            }
            error!("Failed to kill Docker container: {stderr}");
            return Err(Error::Docker(format!("Container kill failed: {stderr}")));
        }

        debug!("Successfully killed Docker container: {container_id}");
        Ok(())
    }

    async fn remove_container(&self, container_id: &str) -> Result<()> {
        trace!("Removing Docker container: {container_id}");

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
                debug!("Container {container_id} already removed");
                return Ok(());
            }
            error!("Failed to remove Docker container: {stderr}");
            return Err(Error::Docker(format!("Container removal failed: {stderr}")));
        }

        debug!("Successfully removed Docker container: {container_id}");
        Ok(())
    }

    async fn container_status(&self, container_id: &str) -> Result<ContainerStatus> {
        trace!("Getting status for Docker container: {container_id}");

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
            // Handle various "not found" errors from Docker
            if stderr.contains("No such container")
                || stderr.contains("No such object")
                || stderr.contains("not found")
            {
                debug!("Container {container_id} not found (may have been removed)");
                return Ok(ContainerStatus::Unknown);
            }
            error!("Failed to inspect Docker container: {stderr}");
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
        trace!("Checking if Docker container exists: {name}");

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
            error!("Failed to check Docker container existence: {stderr}");
            return Err(Error::Docker(format!("Container check failed: {stderr}")));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(!stdout.is_empty())
    }

    async fn container_logs(&self, container_id: &str, lines: usize) -> Result<String> {
        trace!("Getting logs for Docker container: {container_id}");

        let output = Command::new(&self.docker_path)
            .args(["logs", "--tail", &lines.to_string(), container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to get container logs: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to get Docker container logs: {stderr}");
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
        trace!("Following logs for Docker container: {container_id}");

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
                            error!("Error reading stdout: {e}");
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
                            error!("Error reading stderr: {e}");
                            break;
                        }
                    }
                }
            });
        }

        Ok(())
    }

    async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()> {
        trace!("Sending signal {signal} to Docker container: {container_id}");

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
                debug!("Container {container_id} not found when sending signal");
                return Ok(());
            }
            error!("Failed to send signal to Docker container: {stderr}");
            return Err(Error::Docker(format!("Container signal failed: {stderr}")));
        }

        debug!("Successfully sent signal {signal} to Docker container: {container_id}");
        Ok(())
    }

    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
        trace!("Executing command in container {container_id}: {command:?}");

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
            return Err(Error::Docker(format!("Command failed: {stderr}")));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn get_container_by_name(&self, name: &str) -> Result<String> {
        trace!("Getting container by name: {name}");

        let output = Command::new(&self.docker_path)
            .args(["ps", "-aq", "--filter", &format!("name={name}")])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to get container by name: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Docker(format!("Failed to get container: {stderr}")));
        }

        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if container_id.is_empty() {
            return Err(Error::Docker(format!("Container '{name}' not found")));
        }

        Ok(container_id)
    }

    async fn push_image(&self, image: &str) -> Result<()> {
        info!("Pushing Docker image: {image}");

        let output = Command::new(&self.docker_path)
            .args(["push", image])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to push image: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Failed to push Docker image: {stderr}");
            return Err(Error::Docker(format!("Image push failed: {stderr}")));
        }

        debug!("Successfully pushed Docker image: {image}");
        Ok(())
    }

    async fn image_exists(&self, image: &str) -> Result<bool> {
        trace!("Checking if Docker image exists: {image}");

        let output = Command::new(&self.docker_path)
            .args(["image", "inspect", image])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to inspect image: {e}")))?;

        // If the command succeeds, the image exists
        if output.status.success() {
            debug!("Image {image} exists");
            Ok(true)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Check if the error is because the image doesn't exist
            if stderr.contains("No such image") || stderr.contains("not found") {
                debug!("Image {image} does not exist");
                Ok(false)
            } else {
                // Some other error occurred
                error!("Failed to check image existence: {stderr}");
                Err(Error::Docker(format!("Image inspection failed: {stderr}")))
            }
        }
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
        trace!("Following logs for Docker container with director: {container_id}");

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
                            if tx_stdout
                                .send((source_clone.clone(), stream))
                                .await
                                .is_err()
                            {
                                break; // Receiver dropped
                            }
                        }
                        Err(e) => {
                            error!("Error reading stdout: {e}");
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
                            if tx_stderr
                                .send((source_clone.clone(), stream))
                                .await
                                .is_err()
                            {
                                break; // Receiver dropped
                            }
                        }
                        Err(e) => {
                            error!("Error reading stderr: {e}");
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
                error!("Error writing to output director: {e}");
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

/// RAII wrapper for Docker containers that ensures cleanup on drop
pub struct ManagedDockerService {
    inner: DockerService,
    cleanup_on_drop: Arc<AtomicBool>,
    docker_client: Arc<dyn DockerClient>,
}

impl ManagedDockerService {
    /// Create a new managed Docker service
    pub fn new(service: DockerService, docker_client: Arc<dyn DockerClient>) -> Self {
        Self {
            inner: service,
            cleanup_on_drop: Arc::new(AtomicBool::new(true)),
            docker_client,
        }
    }

    /// Create from container ID and config
    pub fn from_container(
        id: String,
        config: DockerServiceConfig,
        docker_client: Arc<dyn DockerClient>,
    ) -> Self {
        let service = DockerService::new(id, config, docker_client.clone());
        Self::new(service, docker_client)
    }

    /// Disable automatic cleanup (for graceful shutdown)
    pub fn disable_cleanup(&self) {
        self.cleanup_on_drop.store(false, Ordering::SeqCst);
    }

    /// Enable automatic cleanup
    pub fn enable_cleanup(&self) {
        self.cleanup_on_drop.store(true, Ordering::SeqCst);
    }

    /// Check if cleanup is enabled
    pub fn cleanup_enabled(&self) -> bool {
        self.cleanup_on_drop.load(Ordering::SeqCst)
    }

    /// Get the inner DockerService
    pub fn inner(&self) -> &DockerService {
        &self.inner
    }

    /// Get container ID
    pub fn id(&self) -> &str {
        self.inner.id()
    }

    /// Get service name
    pub fn name(&self) -> Option<String> {
        self.inner.name()
    }
}

impl Drop for ManagedDockerService {
    fn drop(&mut self) {
        if self.cleanup_on_drop.load(Ordering::SeqCst) {
            let container_id = self.inner.id().to_string();
            let docker_client = self.docker_client.clone();

            // Spawn cleanup task if runtime is available
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    warn!("RAII cleanup: Force stopping container {container_id}");

                    // Try graceful stop first (1 second timeout)
                    match tokio::time::timeout(
                        Duration::from_secs(1),
                        docker_client.stop_container(&container_id),
                    )
                    .await
                    {
                        Ok(Ok(_)) => {
                            debug!("Container {container_id} stopped gracefully during cleanup")
                        }
                        _ => {
                            // Force kill if graceful fails
                            match docker_client.kill_container(&container_id).await {
                                Ok(_) => {
                                    warn!("Container {container_id} force killed during cleanup")
                                }
                                Err(e) => error!(
                                    "Failed to kill container {container_id} during cleanup: {e}"
                                ),
                            }
                        }
                    }

                    // Always try to remove
                    match docker_client.remove_container(&container_id).await {
                        Ok(_) => debug!("Container {container_id} removed during cleanup"),
                        Err(e) => {
                            debug!("Failed to remove container {container_id} during cleanup: {e}")
                        }
                    }
                });
            } else {
                // If no runtime available, try to create one for cleanup
                if let Ok(rt) = tokio::runtime::Runtime::new() {
                    let container_id = self.inner.id().to_string();
                    let docker_client = self.docker_client.clone();

                    rt.block_on(async move {
                        warn!(
                            "RAII cleanup (new runtime): Force stopping container {container_id}"
                        );

                        // Best effort cleanup
                        let _ = docker_client.kill_container(&container_id).await;
                        let _ = docker_client.remove_container(&container_id).await;
                    });
                }
            }
        }
    }
}

impl std::ops::Deref for ManagedDockerService {
    type Target = DockerService;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for ManagedDockerService {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

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
            async fn run_container_with_command(
                &self,
                _image: &str,
                _name: &str,
                _network: &str,
                _env_vars: &[String],
                _ports: &[String],
                _volumes: &[String],
                _command: Option<&[String]>,
            ) -> Result<String> {
                unimplemented!()
            }
            async fn stop_container(&self, container_id: &str) -> Result<()> {
                assert_eq!(container_id, "test-container");
                let mut called = self.stop_container_called.lock().unwrap();
                *called = true;
                Ok(())
            }
            async fn kill_container(&self, _container_id: &str) -> Result<()> {
                unimplemented!()
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
            async fn exec_in_container(
                &self,
                _container_id: &str,
                _command: &[&str],
            ) -> Result<String> {
                unimplemented!()
            }
            async fn get_container_by_name(&self, _name: &str) -> Result<String> {
                unimplemented!()
            }

            async fn push_image(&self, _image: &str) -> Result<()> {
                unimplemented!()
            }

            async fn image_exists(&self, _image: &str) -> Result<bool> {
                Ok(false)
            }
            async fn build_image_with_platform(
                &self,
                _tag: &str,
                _dockerfile: &str,
                _context: &str,
                _platform: &str,
            ) -> Result<()> {
                unimplemented!()
            }
            async fn run_container_with_platform(
                &self,
                _image: &str,
                _name: &str,
                _network: &str,
                _env_vars: &[String],
                _ports: &[String],
                _volumes: &[String],
                _command: Option<&[String]>,
                _platform: &str,
            ) -> Result<String> {
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
            async fn run_container_with_command(
                &self,
                _image: &str,
                _name: &str,
                _network: &str,
                _env_vars: &[String],
                _ports: &[String],
                _volumes: &[String],
                _command: Option<&[String]>,
            ) -> Result<String> {
                unimplemented!()
            }
            async fn stop_container(&self, _container_id: &str) -> Result<()> {
                unimplemented!()
            }
            async fn kill_container(&self, _container_id: &str) -> Result<()> {
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
            async fn exec_in_container(
                &self,
                _container_id: &str,
                _command: &[&str],
            ) -> Result<String> {
                unimplemented!()
            }
            async fn get_container_by_name(&self, _name: &str) -> Result<String> {
                unimplemented!()
            }

            async fn push_image(&self, _image: &str) -> Result<()> {
                unimplemented!()
            }

            async fn image_exists(&self, _image: &str) -> Result<bool> {
                Ok(false)
            }
            async fn build_image_with_platform(
                &self,
                _tag: &str,
                _dockerfile: &str,
                _context: &str,
                _platform: &str,
            ) -> Result<()> {
                unimplemented!()
            }
            async fn run_container_with_platform(
                &self,
                _image: &str,
                _name: &str,
                _network: &str,
                _env_vars: &[String],
                _ports: &[String],
                _volumes: &[String],
                _command: Option<&[String]>,
                _platform: &str,
            ) -> Result<String> {
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
