//! Container service management
//!
//! This module provides functionality for managing container services, including
//! configuration, launch parameters, and runtime state.

use crate::build::ServiceSpec;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

/// Represents runtime configuration for a container service
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Service name
    pub name: String,
    /// Docker image for the service
    pub image: String,
    /// Host address
    pub host: String,
    /// Port number
    pub port: u16,
    /// Container port
    pub target_port: u16,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Secret environment variables
    pub secrets: HashMap<String, String>,
    /// Volume mappings (host_path -> container_path)
    pub volumes: HashMap<String, String>,
    /// Optional mount point for ingress/routing
    pub mount_point: Option<String>,
    /// Domain for the service
    pub domain: String,
}

/// Represents a running container service
#[derive(Debug, Clone, Serialize)]
pub struct ContainerService {
    /// Container ID
    pub id: String,
    /// Service name
    pub name: String,
    /// Docker image
    pub image: String,
    /// Host address
    pub host: String,
    /// Docker host address
    pub docker_host: String,
    /// Port number
    pub port: u16,
    /// Container port
    pub target_port: u16,
    /// Domain for the service
    pub domain: String,
    /// Optional mount point for ingress/routing
    pub mount_point: Option<String>,
}

impl ContainerService {
    /// Creates a new ContainerService instance
    ///
    /// # Arguments
    ///
    /// * `id` - Container ID
    /// * `config` - Service configuration
    ///
    /// # Returns
    ///
    /// A new ContainerService instance
    pub fn from_config(id: String, config: &ServiceConfig) -> Self {
        Self {
            id,
            name: config.name.clone(),
            image: config.image.clone(),
            host: config.host.clone(),
            port: config.port,
            target_port: config.target_port,
            domain: config.domain.clone(),
            mount_point: config.mount_point.clone(),
            docker_host: "TODO".to_string(),
        }
    }

    /// Gets the address for the service
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Gets the URL for the service
    pub fn url(&self) -> String {
        if let Some(mount_point) = &self.mount_point {
            format!("http://{}:{}{}", self.host, self.port, mount_point)
        } else {
            format!("http://{}:{}", self.host, self.port)
        }
    }
}

/// A collection of container services
pub type ServiceCollection = HashMap<String, Vec<Arc<ContainerService>>>;

/// A collection of services organized by domain
pub type ServicesSpec = HashMap<String, Vec<ServiceSpec>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_creation() {
        let mut env = HashMap::new();
        env.insert("ENV1".to_string(), "value1".to_string());

        let mut secrets = HashMap::new();
        secrets.insert("SECRET1".to_string(), "secret_value".to_string());

        let config = ServiceConfig {
            name: "test-service".to_string(),
            image: "test-image:latest".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            target_port: 3000,
            environment: env,
            secrets,
            volumes: HashMap::new(),
            mount_point: Some("/api".to_string()),
            domain: "test.example.com".to_string(),
        };

        let service = ContainerService::from_config("container123".to_string(), &config);

        assert_eq!(service.name, "test-service");
        assert_eq!(service.image, "test-image:latest");
        assert_eq!(service.port, 8080);
        assert_eq!(service.address(), "localhost:8080");
        assert_eq!(service.url(), "http://localhost:8080/api");
    }

    #[test]
    fn test_service_without_mount_point() {
        let config = ServiceConfig {
            name: "api".to_string(),
            image: "api:latest".to_string(),
            host: "localhost".to_string(),
            port: 9000,
            target_port: 8080,
            environment: HashMap::new(),
            secrets: HashMap::new(),
            volumes: HashMap::new(),
            mount_point: None,
            domain: "api.example.com".to_string(),
        };

        let service = ContainerService::from_config("container456".to_string(), &config);

        assert_eq!(service.url(), "http://localhost:9000");
    }
}
