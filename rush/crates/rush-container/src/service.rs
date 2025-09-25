//! Container service management
//!
//! This module provides functionality for managing container services, including
//! configuration, launch parameters, and runtime state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub use rush_build::ServiceSpec;
use serde::Serialize;

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

/// RAII wrapper for ContainerService that ensures cleanup on drop
pub struct ManagedContainerService {
    inner: ContainerService,
    cleanup_on_drop: Arc<AtomicBool>,
    pub(crate) docker_client: Option<Arc<dyn crate::docker::DockerClient>>,
}

impl ManagedContainerService {
    /// Create a new managed container service
    pub fn new(service: ContainerService) -> Self {
        Self {
            inner: service,
            cleanup_on_drop: Arc::new(AtomicBool::new(true)),
            docker_client: None,
        }
    }

    /// Create with a docker client for cleanup
    pub fn with_docker_client(
        service: ContainerService,
        docker_client: Arc<dyn crate::docker::DockerClient>,
    ) -> Self {
        Self {
            inner: service,
            cleanup_on_drop: Arc::new(AtomicBool::new(true)),
            docker_client: Some(docker_client),
        }
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

    /// Get the inner ContainerService
    pub fn inner(&self) -> &ContainerService {
        &self.inner
    }

    /// Get container ID
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    /// Get service name
    pub fn name(&self) -> &str {
        &self.inner.name
    }
}

impl Drop for ManagedContainerService {
    fn drop(&mut self) {
        if self.cleanup_on_drop.load(Ordering::SeqCst) {
            if let Some(docker_client) = &self.docker_client {
                let container_id = self.inner.id.clone();
                let client = docker_client.clone();

                // Spawn cleanup task if runtime is available
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        log::warn!("RAII cleanup: Force stopping container {}", container_id);

                        // Try graceful stop first (1 second timeout)
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(1),
                            client.stop_container(&container_id),
                        )
                        .await
                        {
                            Ok(Ok(_)) => log::debug!(
                                "Container {} stopped gracefully during cleanup",
                                container_id
                            ),
                            _ => {
                                // Force kill if graceful fails
                                match client.kill_container(&container_id).await {
                                    Ok(_) => log::warn!(
                                        "Container {} force killed during cleanup",
                                        container_id
                                    ),
                                    Err(e) => log::error!(
                                        "Failed to kill container {} during cleanup: {}",
                                        container_id,
                                        e
                                    ),
                                }
                            }
                        }

                        // Always try to remove
                        match client.remove_container(&container_id).await {
                            Ok(_) => {
                                log::debug!("Container {} removed during cleanup", container_id)
                            }
                            Err(e) => log::debug!(
                                "Failed to remove container {} during cleanup: {}",
                                container_id,
                                e
                            ),
                        }
                    });
                } else {
                    // If no runtime available, try to create one for cleanup
                    if let Ok(rt) = tokio::runtime::Runtime::new() {
                        let container_id = self.inner.id.clone();
                        let client = client.clone();

                        rt.block_on(async move {
                            log::warn!(
                                "RAII cleanup (new runtime): Force stopping container {}",
                                container_id
                            );

                            // Best effort cleanup
                            let _ = client.kill_container(&container_id).await;
                            let _ = client.remove_container(&container_id).await;
                        });
                    }
                }
            }
        }
    }
}

impl std::ops::Deref for ManagedContainerService {
    type Target = ContainerService;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for ManagedContainerService {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Clone for ManagedContainerService {
    fn clone(&self) -> Self {
        // When cloning, disable cleanup on the clone by default
        // to avoid double cleanup

        Self {
            inner: self.inner.clone(),
            cleanup_on_drop: Arc::new(AtomicBool::new(false)),
            docker_client: self.docker_client.clone(),
        }
    }
}
