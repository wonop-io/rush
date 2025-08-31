//! Docker-based local service implementation
//!
//! This module provides a LocalService implementation for Docker containers.

use async_trait::async_trait;
use log::warn;
use rush_docker::{ContainerStatus, DockerClient};
use rush_core::error::{Error, Result};
use rush_output::simple::Sink;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::config::LocalServiceConfig;
use crate::output::ServiceOutput;
use crate::r#trait::LocalService;
use crate::types::LocalServiceType;

/// Docker-based implementation of LocalService
pub struct DockerLocalService {
    /// Service name
    name: String,
    
    /// Service type
    service_type: LocalServiceType,
    
    /// Docker client
    docker_client: Arc<dyn DockerClient>,
    
    /// Container ID when running
    container_id: Option<String>,
    
    /// Service configuration
    config: LocalServiceConfig,
    
    /// Network to connect to
    network_name: String,
    
    /// Data directory for persistence
    _data_dir: std::path::PathBuf,
    
    /// Output handler
    output: ServiceOutput,
}

impl DockerLocalService {
    /// Follow container logs and send to output sink
    async fn follow_logs_to_output(
        container_id: String,
        output: ServiceOutput,
    ) {
        // Use docker logs -f to follow the container logs
        let mut child = match Command::new("docker")
            .args(["logs", "-f", "--tail", "0", &container_id])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                log::error!("Failed to follow container logs: {}", e);
                return;
            }
        };

        // Handle stdout
        if let Some(stdout) = child.stdout.take() {
            let output_clone = output.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            // Remove trailing newline
                            let clean_line = line.trim_end().to_string();
                            if !clean_line.is_empty() {
                                output_clone.info(clean_line).await;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Handle stderr
        if let Some(stderr) = child.stderr.take() {
            let output_clone = output.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            // Remove trailing newline
                            let clean_line = line.trim_end().to_string();
                            if !clean_line.is_empty() {
                                output_clone.error(clean_line).await;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    }
    
    /// Create a new Docker-based local service
    pub fn new(
        name: String,
        service_type: LocalServiceType,
        docker_client: Arc<dyn DockerClient>,
        config: LocalServiceConfig,
        network_name: String,
        data_dir: std::path::PathBuf,
    ) -> Self {
        let output = ServiceOutput::new(name.clone());
        Self {
            name,
            service_type,
            docker_client,
            container_id: None,
            config,
            network_name,
            _data_dir: data_dir,
            output,
        }
    }
    
    /// Get the Docker image for this service
    fn get_image(&self) -> String {
        self.config
            .image
            .clone()
            .unwrap_or_else(|| self.service_type.default_image())
    }
    
    /// Get the container name
    fn get_container_name(&self) -> String {
        format!("rush-local-{}", self.name)
    }
    
    /// Check container health
    async fn check_container_health(&self) -> Result<bool> {
        if let Some(container_id) = &self.container_id {
            match self.docker_client.container_status(container_id).await {
                Ok(ContainerStatus::Running) => {
                    // If there's a health check, run it
                    if let Some(health_check) = &self.config.health_check {
                        self.run_health_check(container_id, health_check).await
                    } else {
                        Ok(true)
                    }
                }
                Ok(ContainerStatus::Exited(code)) => {
                    warn!("Container {} exited with code {}", self.name, code);
                    Ok(false)
                }
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }
    
    /// Run health check command
    async fn run_health_check(&self, container_id: &str, health_check: &str) -> Result<bool> {
        let command: Vec<&str> = health_check.split_whitespace().collect();
        
        match self.docker_client.exec_in_container(container_id, &command).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    
    /// Generate connection string based on service type
    fn generate_connection_string(&self) -> Option<String> {
        match &self.service_type {
            LocalServiceType::PostgreSQL => {
                let default_user = "postgres".to_string();
                let default_password = "postgres".to_string();
                let default_db = "postgres".to_string();
                
                let user = self.config.env.get("POSTGRES_USER")
                    .or_else(|| self.config.env.get("POSTGRESQL_USER"))
                    .unwrap_or(&default_user);
                let password = self.config.env.get("POSTGRES_PASSWORD")
                    .or_else(|| self.config.env.get("POSTGRESQL_PASSWORD"))
                    .unwrap_or(&default_password);
                let db = self.config.env.get("POSTGRES_DB")
                    .or_else(|| self.config.env.get("POSTGRESQL_DATABASE"))
                    .unwrap_or(&default_db);
                let port = self.config.ports.first()
                    .map(|p| p.host_port)
                    .unwrap_or(5432);
                
                Some(format!(
                    "postgres://{}:{}@{}:{}/{}",
                    user, password, self.get_container_name(), port, db
                ))
            }
            LocalServiceType::Redis => {
                let port = self.config.ports.first()
                    .map(|p| p.host_port)
                    .unwrap_or(6379);
                Some(format!("redis://{}:{}", self.get_container_name(), port))
            }
            LocalServiceType::MinIO => {
                let port = self.config.ports.first()
                    .map(|p| p.host_port)
                    .unwrap_or(9000);
                Some(format!("http://{}:{}", self.get_container_name(), port))
            }
            _ => None,
        }
    }
}

#[async_trait]
impl LocalService for DockerLocalService {
    async fn start(&mut self) -> Result<()> {
        self.output.info(format!("Starting Docker local service: {}", self.name)).await;
        
        // Check if container already exists
        let container_name = self.get_container_name();
        match self.docker_client.get_container_by_name(&container_name).await {
            Ok(existing_id) => {
                self.output.info(format!("Found existing container for {}, removing it", self.name)).await;
                let _ = self.docker_client.stop_container(&existing_id).await;
                let _ = self.docker_client.remove_container(&existing_id).await;
            }
            Err(_) => {
                // No existing container found
            }
        }
        
        // Prepare environment variables
        let mut env_vars: Vec<String> = self.config.env
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        
        // Add default environment variables for known service types
        match &self.service_type {
            LocalServiceType::PostgreSQL => {
                if !self.config.env.contains_key("POSTGRES_HOST_AUTH_METHOD") {
                    env_vars.push("POSTGRES_HOST_AUTH_METHOD=trust".to_string());
                }
            }
            _ => {}
        }
        
        // Prepare port mappings
        let ports: Vec<String> = self.config.ports
            .iter()
            .map(|p| p.to_docker_format())
            .collect();
        
        // Prepare volume mappings
        let volumes: Vec<String> = if self.config.persist_data {
            self.config.volumes
                .iter()
                .map(|v| v.to_docker_format())
                .collect()
        } else {
            vec![]
        };
        
        // Get the image
        let image = self.get_image();
        
        // Pull the image if needed
        self.output.info(format!("Pulling image {} if needed", image)).await;
        self.docker_client.pull_image(&image).await?;
        
        // Prepare command if specified
        let command = self.config.command.as_ref().map(|cmd| {
            cmd.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>()
        });
        
        // Run the container
        let container_id = if let Some(cmd) = command {
            self.docker_client
                .run_container_with_command(
                    &image,
                    &container_name,
                    &self.network_name,
                    &env_vars,
                    &ports,
                    &volumes,
                    Some(&cmd),
                )
                .await?
        } else {
            self.docker_client
                .run_container(
                    &image,
                    &container_name,
                    &self.network_name,
                    &env_vars,
                    &ports,
                    &volumes,
                )
                .await?
        };
        
        self.container_id = Some(container_id.clone());
        self.output.info(format!("Docker local service {} started successfully", self.name)).await;
        
        // Start following container logs if we have an output sink
        if self.output.has_sink() {
            let container_id_clone = container_id.clone();
            let output_clone = self.output.clone();
            
            tokio::spawn(async move {
                Self::follow_logs_to_output(
                    container_id_clone,
                    output_clone,
                ).await;
            });
        }
        
        // Run initialization scripts if any
        if !self.config.init_scripts.is_empty() {
            self.output.info(format!("Running initialization scripts for {}", self.name)).await;
            for script in &self.config.init_scripts {
                self.output.info(format!("Running init script: {}", script)).await;
                let command: Vec<&str> = script.split_whitespace().collect();
                if let Some(container_id) = &self.container_id {
                    self.docker_client
                        .exec_in_container(container_id, &command)
                        .await
                        .map_err(|e| Error::Docker(format!(
                            "Failed to run init script for {}: {}",
                            self.name, e
                        )))?;
                }
            }
        }
        
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<()> {
        self.output.info(format!("Stopping Docker local service: {}", self.name)).await;
        
        // Always try to stop by name to ensure we clean up properly
        // This handles cases where container_id might not be set correctly
        let container_name = self.get_container_name();
        
        // First try to stop using the container ID if we have it
        if let Some(container_id) = &self.container_id {
            let _ = self.docker_client.stop_container(container_id).await;
        }
        
        // Also try to stop by name to ensure cleanup
        // This catches containers that might have been started but not tracked properly
        match self.docker_client.get_container_by_name(&container_name).await {
            Ok(existing_id) => {
                self.output.info(format!("Stopping container {} by name", container_name)).await;
                
                // Stop the container
                if let Err(e) = self.docker_client.stop_container(&existing_id).await {
                    self.output.error(format!("Failed to stop container {}: {}", container_name, e)).await;
                }
                
                // Always remove the container on shutdown (even if persist_data is true)
                // When the program restarts, it will create a new container
                self.output.info(format!("Removing container {}", container_name)).await;
                if let Err(e) = self.docker_client.remove_container(&existing_id).await {
                    self.output.error(format!("Failed to remove container {}: {}", container_name, e)).await;
                }
            }
            Err(_) => {
                // Container doesn't exist or already stopped
                self.output.info(format!("Container {} not found or already stopped", container_name)).await;
            }
        }
        
        self.container_id = None;
        self.output.info(format!("Docker local service {} stopped", self.name)).await;
        
        Ok(())
    }
    
    async fn is_healthy(&self) -> Result<bool> {
        self.check_container_health().await
    }
    
    async fn run_post_startup_tasks(&mut self) -> Result<()> {
        if self.config.post_startup_tasks.is_empty() {
            return Ok(());
        }
        
        let container_id = match &self.container_id {
            Some(id) => id.clone(),
            None => {
                return Err(Error::Docker(format!(
                    "Cannot run post-startup tasks for {}: container not running",
                    self.name
                )));
            }
        };
        
        self.output.info(format!("Running post-startup tasks for {}", self.name)).await;
        
        for task in &self.config.post_startup_tasks.clone() {
            self.output.info(format!("⏺ Executing: {}", task)).await;
            
            // Split the command into parts for execution
            // Use shell -c to handle complex commands with pipes, redirects, etc.
            let command = vec!["sh", "-c", task];
            
            match self.docker_client.exec_in_container(&container_id, &command).await {
                Ok(output) => {
                    // Log the output if any
                    if !output.is_empty() {
                        for line in output.lines() {
                            self.output.info(format!("  ⎿  {}", line)).await;
                        }
                    }
                }
                Err(e) => {
                    // Check if the error is because the resource already exists
                    // For S3 buckets, we use "|| true" in the command to ignore errors
                    // But log as info for other types of errors (they may be expected)
                    self.output.info(format!("  ⎿  Task failed (may be expected): {}", e)).await;
                }
            }
        }
        
        self.output.info(format!("Post-startup tasks completed for {}", self.name)).await;
        Ok(())
    }
    
    async fn generated_env_vars(&self) -> Result<HashMap<String, String>> {
        let mut vars = HashMap::new();
        
        // Add connection string if applicable
        if let Some(conn_str) = self.generate_connection_string() {
            let key = format!("{}_{}_URL", 
                self.name.to_uppercase().replace('-', "_"),
                self.service_type.env_var_suffix()
            );
            vars.insert(key, conn_str);
        }
        
        // Add service-specific environment variables
        match &self.service_type {
            LocalServiceType::MinIO => {
                if let Some(access_key) = self.config.env.get("MINIO_ROOT_USER") {
                    vars.insert(
                        format!("{}_S3_ACCESS_KEY", self.name.to_uppercase()),
                        access_key.clone()
                    );
                }
                if let Some(secret_key) = self.config.env.get("MINIO_ROOT_PASSWORD") {
                    vars.insert(
                        format!("{}_S3_SECRET_KEY", self.name.to_uppercase()),
                        secret_key.clone()
                    );
                }
            }
            _ => {}
        }
        
        Ok(vars)
    }
    
    async fn generated_env_secrets(&self) -> Result<HashMap<String, String>> {
        // Most Docker services don't generate secrets
        // This would be overridden by specific implementations if needed
        Ok(HashMap::new())
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn service_type(&self) -> LocalServiceType {
        self.service_type.clone()
    }
    
    fn is_running(&self) -> bool {
        self.container_id.is_some()
    }
    
    fn set_output_sink(&mut self, sink: Arc<Mutex<Box<dyn Sink>>>) {
        self.output.set_sink(sink);
    }
}