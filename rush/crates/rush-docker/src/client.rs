//! Docker client implementations

use crate::{ContainerStatus, DockerClient};
use async_trait::async_trait;
use rush_core::{Error, Result};
use std::process::Stdio;
use tokio::process::Command;
use log::{debug, error, info, warn};
use tracing::instrument;

/// Docker executor that implements DockerClient using command-line interface
#[derive(Debug, Clone)]
pub struct DockerExecutor {
    /// Whether to use sudo for docker commands
    use_sudo: bool,
    /// Default timeout for operations in seconds
    timeout: u64,
}

impl Default for DockerExecutor {
    fn default() -> Self {
        Self {
            use_sudo: false,
            timeout: 300,
        }
    }
}

impl DockerExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sudo(mut self) -> Self {
        self.use_sudo = true;
        self
    }

    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = timeout;
        self
    }

    /// Get container exit code (helper method, not part of trait)
    #[instrument(level = "debug", skip(self), fields(container_id = %container_id))]
    async fn get_container_exit_code(&self, container_id: &str) -> Result<Option<i32>> {
        let args = vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{.State.ExitCode}}".to_string(),
            container_id.to_string(),
        ];

        match self.execute(args).await {
            Ok(output) => {
                let code = output.trim().parse::<i32>().ok();
                Ok(code)
            }
            Err(_) => Ok(None),
        }
    }

    /// Execute a docker command with arguments
    #[instrument(level = "debug", skip(self), fields(command = args.get(0).map(|s| s.as_str()).unwrap_or("unknown"), args_count = args.len()))]
    async fn execute(&self, args: Vec<String>) -> Result<String> {
        let _total_start = std::time::Instant::now();
        let docker_cmd = args.get(0).map(|s| s.as_str()).unwrap_or("unknown");

        let program = if self.use_sudo { "sudo" } else { "docker" };
        let mut cmd = Command::new(program);

        if self.use_sudo {
            cmd.arg("docker");
        }

        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Executing: {} {}", program, args.join(" "));

        let exec_start = std::time::Instant::now();
        let output = cmd
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to execute docker command: {e}")))?;

        // Record Docker command execution time
        // Note: We can't access rush_container from here, so we'll just log the timing
        let duration = exec_start.elapsed();
        debug!("Docker command '{}' took {:?}", docker_cmd, duration);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Check for common errors
            if stderr.contains("No such container") || stdout.contains("No such container") {
                return Err(Error::ContainerNotFound("Container not found".to_string()));
            }
            if stderr.contains("No such image") || stdout.contains("No such image") {
                return Err(Error::Docker("Image not found".to_string()));
            }

            error!("Docker command failed: {}", stderr);
            return Err(Error::Docker(format!("Docker command failed: {stderr}")));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[async_trait]
impl DockerClient for DockerExecutor {
    // Network operations
    #[instrument(level = "info", skip(self), fields(network_name = %name))]
    async fn create_network(&self, name: &str) -> Result<()> {
        let args = vec![
            "network".to_string(),
            "create".to_string(),
            name.to_string(),
        ];
        self.execute(args).await?;
        info!("Created Docker network: {}", name);
        Ok(())
    }

    #[instrument(level = "info", skip(self), fields(network_name = %name))]
    async fn delete_network(&self, name: &str) -> Result<()> {
        let args = vec!["network".to_string(), "rm".to_string(), name.to_string()];
        self.execute(args).await?;
        info!("Deleted Docker network: {}", name);
        Ok(())
    }

    #[instrument(level = "debug", skip(self), fields(network_name = %name))]
    async fn network_exists(&self, name: &str) -> Result<bool> {
        let args = vec![
            "network".to_string(),
            "ls".to_string(),
            "--format".to_string(),
            "{{.Name}}".to_string(),
        ];
        let output = self.execute(args).await?;
        let exists = output.lines().any(|line| line.trim() == name);
        warn!("network_exists check: searching for '{}', found: {}", name, exists);
        warn!("Available networks: {}", output.lines().collect::<Vec<_>>().join(", "));
        Ok(exists)
    }

    // Image operations
    #[instrument(level = "info", skip(self), fields(image = %image))]
    async fn pull_image(&self, image: &str) -> Result<()> {
        let args = vec!["pull".to_string(), image.to_string()];
        self.execute(args).await?;
        info!("Pulled Docker image: {}", image);
        Ok(())
    }

    #[instrument(level = "info", skip(self), fields(tag = %tag, dockerfile = %dockerfile, context = %context))]
    async fn build_image(&self, tag: &str, dockerfile: &str, context: &str) -> Result<()> {
        let mut args = vec!["build".to_string()];

        args.push("--tag".to_string());
        args.push(tag.to_string());

        args.push("--file".to_string());
        args.push(dockerfile.to_string());

        args.push(context.to_string());

        info!("Docker build command: docker {}", args.join(" "));
        debug!("Building from context: {}", context);
        
        let output = self.execute(args).await?;
        
        // Log the build output for debugging
        if !output.trim().is_empty() {
            debug!("Docker build output:\n{}", output);
        }
        
        info!("Built Docker image: {}", tag);
        Ok(())
    }

    // Container operations
    #[instrument(level = "info", skip(self, env_vars, ports, volumes), fields(image = %image, container_name = %name, network = %network))]
    async fn run_container(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
    ) -> Result<String> {
        let mut args = vec!["run".to_string()];

        args.push("-d".to_string()); // detach by default
        args.push("--name".to_string());
        args.push(name.to_string());
        args.push("--network".to_string());
        args.push(network.to_string());

        for env_var in env_vars {
            args.push("-e".to_string());
            args.push(env_var.clone());
        }

        for port in ports {
            args.push("-p".to_string());
            args.push(port.clone());
        }

        for volume in volumes {
            args.push("-v".to_string());
            args.push(volume.clone());
        }

        args.push(image.to_string());

        let output = self.execute(args).await?;
        let container_id = output.trim().to_string();
        info!(
            "Started container {} with ID: {}",
            name, container_id
        );
        Ok(container_id)
    }

    #[instrument(level = "info", skip(self, env_vars, ports, volumes, command), fields(image = %image, container_name = %name, network = %network))]
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
        let mut args = vec!["run".to_string()];

        args.push("-d".to_string()); // detach by default
        args.push("--name".to_string());
        args.push(name.to_string());
        args.push("--network".to_string());
        args.push(network.to_string());

        for env_var in env_vars {
            args.push("-e".to_string());
            args.push(env_var.clone());
        }

        for port in ports {
            args.push("-p".to_string());
            args.push(port.clone());
        }

        for volume in volumes {
            args.push("-v".to_string());
            args.push(volume.clone());
        }

        args.push(image.to_string());

        if let Some(cmd) = command {
            args.extend(cmd.iter().cloned());
        }

        let output = self.execute(args).await?;
        let container_id = output.trim().to_string();
        info!(
            "Started container {} with ID: {}",
            name, container_id
        );
        Ok(container_id)
    }

    #[instrument(level = "info", skip(self), fields(container_id = %container_id))]
    async fn stop_container(&self, container_id: &str) -> Result<()> {
        let args = vec!["stop".to_string(), container_id.to_string()];
        self.execute(args).await?;
        info!("Stopped container: {}", container_id);
        Ok(())
    }

    #[instrument(level = "info", skip(self), fields(container_id = %container_id))]
    async fn kill_container(&self, container_id: &str) -> Result<()> {
        let args = vec!["kill".to_string(), container_id.to_string()];
        match self.execute(args).await {
            Ok(_) => {
                info!("Killed container: {}", container_id);
                Ok(())
            }
            Err(e) => {
                // Don't fail if container doesn't exist or is already stopped
                if e.to_string().contains("No such container") || e.to_string().contains("is not running") {
                    info!("Container {} already stopped or doesn't exist", container_id);
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    #[instrument(level = "info", skip(self), fields(container_id = %container_id))]
    async fn remove_container(&self, container_id: &str) -> Result<()> {
        let args = vec!["rm".to_string(), "-f".to_string(), container_id.to_string()];
        self.execute(args).await?;
        info!("Removed container: {}", container_id);
        Ok(())
    }

    #[instrument(level = "debug", skip(self), fields(container_id = %container_id))]
    async fn container_status(&self, container_id: &str) -> Result<ContainerStatus> {
        let args = vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{.State.Status}}".to_string(),
            container_id.to_string(),
        ];

        match self.execute(args).await {
            Ok(output) => {
                let status = output.trim();
                match status {
                    "running" => Ok(ContainerStatus::Running),
                    "exited" => {
                        // Get exit code
                        if let Ok(Some(code)) = self.get_container_exit_code(container_id).await {
                            Ok(ContainerStatus::Exited(code))
                        } else {
                            Ok(ContainerStatus::Unknown)
                        }
                    }
                    _ => Ok(ContainerStatus::Unknown),
                }
            }
            Err(_) => Ok(ContainerStatus::Unknown),
        }
    }

    async fn container_exists(&self, name: &str) -> Result<bool> {
        let args = vec![
            "ps".to_string(),
            "-a".to_string(),
            "--filter".to_string(),
            format!("name={}", name),
            "--format".to_string(),
            "{{.ID}}".to_string(),
        ];

        let output = self.execute(args).await?;
        Ok(!output.trim().is_empty())
    }

    async fn container_logs(&self, container_id: &str, lines: usize) -> Result<String> {
        let mut args = vec!["logs".to_string()];
        
        if lines > 0 {
            args.push("--tail".to_string());
            args.push(lines.to_string());
        }

        args.push(container_id.to_string());

        self.execute(args).await
    }

    async fn follow_container_logs(
        &self,
        container_id: &str,
        _label: String,
        _color: &str,
    ) -> Result<()> {
        // This would need a more complex implementation with streaming
        // For now, just get the logs without following
        let args = vec!["logs".to_string(), container_id.to_string()];
        self.execute(args).await?;
        Ok(())
    }

    async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()> {
        let args = vec![
            "kill".to_string(),
            "-s".to_string(),
            signal.to_string(),
            container_id.to_string(),
        ];
        self.execute(args).await?;
        Ok(())
    }

    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
        let mut args = vec!["exec".to_string(), container_id.to_string()];
        args.extend(command.iter().map(|s| s.to_string()));

        self.execute(args).await
    }

    async fn get_container_by_name(&self, name: &str) -> Result<String> {
        let args = vec![
            "ps".to_string(),
            "-a".to_string(),
            "--filter".to_string(),
            format!("name={}", name),
            "--format".to_string(),
            "{{.ID}}".to_string(),
        ];

        let output = self.execute(args).await?;
        let id = output.trim();

        if id.is_empty() {
            Err(Error::Docker(format!("Container {} not found", name)))
        } else {
            Ok(id.to_string())
        }
    }
    
    async fn push_image(&self, image: &str) -> Result<()> {
        info!("Pushing Docker image: {}", image);
        let args = vec!["push".to_string(), image.to_string()];

        // Docker push can take a while, especially for large images
        // The output will show progress
        let output = self.execute(args).await?;

        // Log the output for debugging
        debug!("Docker push output: {}", output);

        info!("Successfully pushed Docker image: {}", image);
        Ok(())
    }

    #[instrument(level = "debug", skip(self), fields(image = %image))]
    async fn image_exists(&self, image: &str) -> Result<bool> {
        let args = vec![
            "image".to_string(),
            "inspect".to_string(),
            image.to_string(),
        ];

        match self.execute(args).await {
            Ok(_) => {
                debug!("Image {} exists", image);
                Ok(true)
            }
            Err(e) => {
                // Check if the error is because the image doesn't exist
                if e.to_string().contains("Image not found") {
                    debug!("Image {} does not exist", image);
                    Ok(false)
                } else {
                    // Some other error occurred
                    Err(e)
                }
            }
        }
    }
}