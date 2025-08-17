//! Docker-based local service implementation
//!
//! This module provides a LocalService implementation for Docker containers.

use async_trait::async_trait;
use log::{debug, info, warn};
use rush_core::docker::{ContainerStatus, DockerClient};
use rush_core::error::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::LocalServiceConfig;
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
    data_dir: std::path::PathBuf,
}

impl DockerLocalService {
    /// Create a new Docker-based local service
    pub fn new(
        name: String,
        service_type: LocalServiceType,
        docker_client: Arc<dyn DockerClient>,
        config: LocalServiceConfig,
        network_name: String,
        data_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            name,
            service_type,
            docker_client,
            container_id: None,
            config,
            network_name,
            data_dir,
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
        info!("Starting Docker local service: {}", self.name);
        
        // Check if container already exists
        let container_name = self.get_container_name();
        match self.docker_client.get_container_by_name(&container_name).await {
            Ok(existing_id) => {
                info!("Found existing container for {}, removing it", self.name);
                let _ = self.docker_client.stop_container(&existing_id).await;
                let _ = self.docker_client.remove_container(&existing_id).await;
            }
            Err(_) => {
                debug!("No existing container found for {}", self.name);
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
        debug!("Pulling image {} if needed", image);
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
        info!("Docker local service {} started successfully", self.name);
        
        // Start following container logs
        let docker_client = self.docker_client.clone();
        let name = self.name.clone();
        let container_id_clone = container_id.clone();
        tokio::spawn(async move {
            // Follow container logs continuously
            let _ = docker_client.follow_container_logs(
                &container_id_clone,
                name,
                "cyan"  // Use cyan color for local services
            ).await;
        });
        
        // Run initialization scripts if any
        if !self.config.init_scripts.is_empty() {
            info!("Running initialization scripts for {}", self.name);
            for script in &self.config.init_scripts {
                debug!("Running init script: {}", script);
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
        if let Some(container_id) = &self.container_id {
            info!("Stopping Docker local service: {}", self.name);
            
            self.docker_client
                .stop_container(container_id)
                .await
                .map_err(|e| Error::Docker(format!(
                    "Failed to stop {}: {}",
                    self.name, e
                )))?;
            
            // Only remove if not persisting data
            if !self.config.persist_data {
                self.docker_client
                    .remove_container(container_id)
                    .await
                    .map_err(|e| Error::Docker(format!(
                        "Failed to remove {}: {}",
                        self.name, e
                    )))?;
            }
            
            self.container_id = None;
            info!("Docker local service {} stopped", self.name);
        }
        
        Ok(())
    }
    
    async fn is_healthy(&self) -> Result<bool> {
        self.check_container_health().await
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
}