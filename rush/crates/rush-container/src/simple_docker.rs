//! Simplified Docker implementation using direct subprocess execution
//!
//! This module provides a dramatically simplified approach to Docker container
//! management by running containers as subprocesses with interactive mode (-it)
//! and streaming output directly, eliminating the need for complex abstractions,
//! polling, and docker logs.

use rush_core::error::{Error, Result};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;
use log::{debug, error, info};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::simple_output::{OutputLine, SinkExt};
use rush_output::simple::Sink;

/// Options for running a container
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Container name
    pub name: String,
    /// Docker image
    pub image: String,
    /// Network to connect to
    pub network: Option<String>,
    /// Environment variables (KEY=VALUE format)
    pub env_vars: Vec<String>,
    /// Port mappings (HOST:CONTAINER format)
    pub ports: Vec<String>,
    /// Volume mounts (HOST:CONTAINER format)
    pub volumes: Vec<String>,
    /// Additional docker run arguments
    pub extra_args: Vec<String>,
    /// Working directory
    pub workdir: Option<String>,
    /// Command to run (overrides image CMD)
    pub command: Option<Vec<String>>,
    /// Run detached (for services that don't need output streaming)
    pub detached: bool,
}

impl RunOptions {
    /// Convert options to docker command arguments
    pub fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // Use -d (detached) and -t (allocate pseudo-TTY) for proper terminal emulation
        // This provides TTY without requiring stdin to be a TTY
        args.push("-d".to_string());
        args.push("-t".to_string());

        // Add platform for consistency
        args.push("--platform".to_string());
        args.push("linux/amd64".to_string());

        // Container name
        args.push("--name".to_string());
        args.push(self.name.clone());

        // Network
        if let Some(network) = &self.network {
            args.push("--network".to_string());
            args.push(network.clone());
        }

        // Environment variables
        for env in &self.env_vars {
            args.push("-e".to_string());
            args.push(env.clone());
        }

        // Port mappings
        for port in &self.ports {
            args.push("-p".to_string());
            args.push(port.clone());
        }

        // Volume mounts
        for volume in &self.volumes {
            args.push("-v".to_string());
            args.push(volume.clone());
        }

        // Working directory
        if let Some(workdir) = &self.workdir {
            args.push("-w".to_string());
            args.push(workdir.clone());
        }

        // Extra arguments
        args.extend(self.extra_args.clone());

        args
    }
}

/// Simple Docker container manager using docker CLI
pub struct SimpleDocker {
    /// Output sink for container logs
    output_sink: Option<Arc<Mutex<Box<dyn Sink>>>>,
    /// Docker command path (usually "docker")
    docker_cmd: String,
}

impl SimpleDocker {
    /// Create a new SimpleDocker instance
    pub fn new() -> Self {
        Self {
            output_sink: None,
            docker_cmd: "docker".to_string(),
        }
    }

    /// Create with a specific docker command path
    pub fn with_docker_cmd(docker_cmd: String) -> Self {
        Self {
            output_sink: None,
            docker_cmd,
        }
    }

    /// Set the output sink for container logs
    pub fn set_output_sink(&mut self, sink: Arc<Mutex<Box<dyn Sink>>>) {
        self.output_sink = Some(sink);
    }

    /// Run a container with pseudo-TTY and follow logs
    pub async fn run_interactive(
        &self,
        options: RunOptions,
    ) -> Result<String> {
        info!("Starting container {} with image {}", options.name, options.image);

        // First, run the container with -d -t (detached with TTY)
        let mut cmd = Command::new(&self.docker_cmd);
        cmd.arg("run");

        // Add all the options as arguments
        for arg in options.to_args() {
            cmd.arg(arg);
        }

        // Add the image
        cmd.arg(&options.image);

        // Add command if specified
        if let Some(command) = &options.command {
            for arg in command {
                cmd.arg(arg);
            }
        }

        debug!("Executing: {:?}", cmd);

        // Run the container and capture the container ID
        let output = cmd.output().await
            .map_err(|e| Error::Docker(format!("Failed to start container {}: {}", options.name, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Docker(format!("Failed to start container {}: {}", options.name, stderr)));
        }

        // The container ID is in stdout
        let container_id = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        info!("Container {} started with ID: {}", options.name, container_id);

        // Now follow the logs using docker logs --follow
        // This gives us the TTY output with colors preserved
        if let Some(sink) = &self.output_sink {
            let container_name = options.name.clone();
            let sink = sink.clone();
            let docker_cmd = self.docker_cmd.clone();

            tokio::spawn(async move {
                // Give container a moment to start producing output
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                // Follow logs using docker logs --follow
                let mut cmd = Command::new(&docker_cmd);
                cmd.args(&["logs", "--follow", &container_name]);
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());

                if let Ok(mut child) = cmd.spawn() {
                    // Stream stdout
                    if let Some(stdout) = child.stdout.take() {
                        let component_name = container_name.clone();
                        let sink_clone = sink.clone();

                        tokio::spawn(async move {
                            let reader = BufReader::new(stdout);
                            let mut lines = reader.lines();

                            while let Ok(Some(line)) = lines.next_line().await {
                                let output_line = OutputLine {
                                    component: component_name.clone(),
                                    line,
                                    is_error: false,
                                };

                                let _ = sink_clone.lock().await.write_output_line(output_line).await;
                            }
                        });
                    }

                    // Stream stderr
                    if let Some(stderr) = child.stderr.take() {
                        let component_name = container_name.clone();
                        let sink_clone = sink;

                        tokio::spawn(async move {
                            let reader = BufReader::new(stderr);
                            let mut lines = reader.lines();

                            while let Ok(Some(line)) = lines.next_line().await {
                                let output_line = OutputLine {
                                    component: component_name.clone(),
                                    line,
                                    is_error: true,
                                };

                                let _ = sink_clone.lock().await.write_output_line(output_line).await;
                            }
                        });
                    }
                }
            });
        }

        Ok(container_id)
    }

    /// Stop a container using docker stop
    pub async fn stop(&self, name: &str) -> Result<()> {
        info!("Stopping container {}", name);

        // Use docker stop to stop the container
        let output = Command::new(&self.docker_cmd)
            .args(&["stop", name])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to stop container {}: {}", name, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("No such container") {
                return Err(Error::Docker(format!("Failed to stop container {}: {}", name, stderr)));
            }
        }

        info!("Container {} stopped", name);
        Ok(())
    }

    /// Remove a container (force remove)
    pub async fn remove(&self, name: &str) -> Result<()> {
        info!("Removing container {}", name);

        // Force remove the container
        let output = Command::new(&self.docker_cmd)
            .args(&["rm", "-f", name])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to remove container {}: {}", name, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("No such container") {
                return Err(Error::Docker(format!("Failed to remove container {}: {}", name, stderr)));
            }
        }

        info!("Container {} removed", name);
        Ok(())
    }

    /// List running containers (simple docker ps)
    pub async fn list(&self) -> Result<Vec<String>> {
        let output = Command::new(&self.docker_cmd)
            .args(&["ps", "--format", "{{.Names}}"])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to list containers: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Docker(format!("Failed to list containers: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let containers: Vec<String> = stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(containers)
    }

    /// Check if a container exists
    pub async fn exists(&self, name: &str) -> Result<bool> {
        let output = Command::new(&self.docker_cmd)
            .args(&["ps", "-a", "--filter", &format!("name={}", name), "--format", "{{.Names}}"])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to check container {}: {}", name, e)))?;

        if !output.status.success() {
            return Ok(false);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().any(|line| line.trim() == name))
    }

    /// Get container ID by name
    pub async fn get_container_id(&self, name: &str) -> Result<String> {
        let output = Command::new(&self.docker_cmd)
            .args(&["ps", "-aqf", &format!("name={}", name)])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to get container ID for {}: {}", name, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Docker(format!("Failed to get container ID for {}: {}", name, stderr)));
        }

        let id = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Docker(format!("Container {} not found", name)))?;

        Ok(id)
    }

    /// Stop all containers managed by this instance
    pub async fn stop_all(&self) -> Result<()> {
        // Get list of running containers
        let containers = self.list().await?;

        info!("Stopping {} containers", containers.len());

        for name in containers {
            if let Err(e) = self.stop(&name).await {
                error!("Failed to stop container {}: {}", name, e);
            }
        }

        Ok(())
    }

    /// Clean shutdown of all containers
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down SimpleDocker");

        // Stop all containers
        self.stop_all().await?;

        Ok(())
    }

    /// Run a quick command in a container and return output
    pub async fn run_command(&self, image: &str, command: Vec<String>) -> Result<String> {
        let mut cmd = Command::new(&self.docker_cmd);
        cmd.arg("run")
           .arg("--rm")  // Remove after exit
           .arg(image);

        for arg in command {
            cmd.arg(arg);
        }

        let output = cmd.output().await
            .map_err(|e| Error::Docker(format!("Failed to run command in {}: {}", image, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Docker(format!("Command failed in {}: {}", image, stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Pull an image
    pub async fn pull_image(&self, image: &str) -> Result<()> {
        info!("Pulling image {}", image);

        let output = Command::new(&self.docker_cmd)
            .args(&["pull", image])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to pull image {}: {}", image, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Docker(format!("Failed to pull image {}: {}", image, stderr)));
        }

        info!("Successfully pulled image {}", image);
        Ok(())
    }

    /// Create a network
    pub async fn create_network(&self, name: &str) -> Result<()> {
        info!("Creating network {}", name);

        let output = Command::new(&self.docker_cmd)
            .args(&["network", "create", name])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to create network {}: {}", name, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore if network already exists
            if !stderr.contains("already exists") {
                return Err(Error::Docker(format!("Failed to create network {}: {}", name, stderr)));
            }
            debug!("Network {} already exists", name);
        } else {
            info!("Created network {}", name);
        }

        Ok(())
    }

    /// Remove a network
    pub async fn remove_network(&self, name: &str) -> Result<()> {
        info!("Removing network {}", name);

        let output = Command::new(&self.docker_cmd)
            .args(&["network", "rm", name])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to remove network {}: {}", name, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore if network doesn't exist
            if !stderr.contains("not found") {
                return Err(Error::Docker(format!("Failed to remove network {}: {}", name, stderr)));
            }
            debug!("Network {} not found", name);
        } else {
            info!("Removed network {}", name);
        }

        Ok(())
    }
}

impl Default for SimpleDocker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_options_to_args() {
        let options = RunOptions {
            name: "test-container".to_string(),
            image: "alpine:latest".to_string(),
            network: Some("test-net".to_string()),
            env_vars: vec!["FOO=bar".to_string(), "BAZ=qux".to_string()],
            ports: vec!["8080:80".to_string()],
            volumes: vec!["/tmp:/data".to_string()],
            workdir: Some("/app".to_string()),
            command: None,
            detached: false,
            extra_args: vec!["--rm".to_string()],
        };

        let args = options.to_args();

        assert!(args.contains(&"-it".to_string()));
        assert!(args.contains(&"--name".to_string()));
        assert!(args.contains(&"test-container".to_string()));
        assert!(args.contains(&"--network".to_string()));
        assert!(args.contains(&"test-net".to_string()));
        assert!(args.contains(&"-e".to_string()));
        assert!(args.contains(&"FOO=bar".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"8080:80".to_string()));
        assert!(args.contains(&"-v".to_string()));
        assert!(args.contains(&"/tmp:/data".to_string()));
        assert!(args.contains(&"-w".to_string()));
        assert!(args.contains(&"/app".to_string()));
        assert!(args.contains(&"--rm".to_string()));
    }

    #[test]
    fn test_detached_mode() {
        let options = RunOptions {
            name: "test".to_string(),
            image: "alpine".to_string(),
            detached: true,
            ..Default::default()
        };

        let args = options.to_args();
        assert!(args.contains(&"-d".to_string()));
        assert!(!args.contains(&"-it".to_string()));
    }
}