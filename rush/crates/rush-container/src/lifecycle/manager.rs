//! Container lifecycle manager
//!
//! This module manages the lifecycle of containers including starting,
//! stopping, and coordinating container operations.

use crate::{
    docker::{DockerClient, DockerService, DockerServiceConfig},
    events::{Event, EventBus, ContainerEvent},
    reactor::state::{SharedReactorState, ReactorPhase},
    service::ContainerService,
};
use rush_core::error::Result;
use rush_security::Vault;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use log::{debug, error, info, warn};
use tokio::sync::broadcast;

/// Configuration for the lifecycle manager
#[derive(Debug, Clone)]
pub struct LifecycleConfig {
    /// Product name
    pub product_name: String,
    /// Environment (dev, staging, prod)
    pub environment: String,
    /// Docker network name
    pub network_name: String,
    /// Components to redirect (skip starting)
    pub redirected_components: HashMap<String, (String, u16)>,
    /// Whether to inject stripe secrets
    pub inject_stripe_secrets: bool,
    /// Max retries for container operations
    pub max_retries: u32,
    /// Retry delay
    pub retry_delay: Duration,
    /// Whether to automatically restart failed containers
    pub auto_restart: bool,
    /// Whether to enable health checks
    pub enable_health_checks: bool,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Maximum restart attempts before giving up
    pub max_restart_attempts: u32,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            product_name: String::new(),
            environment: "dev".to_string(),
            network_name: "rush-network".to_string(),
            redirected_components: HashMap::new(),
            inject_stripe_secrets: false,
            max_retries: 3,
            retry_delay: Duration::from_secs(1),
            auto_restart: true,
            enable_health_checks: false,
            health_check_interval: Duration::from_secs(30),
            max_restart_attempts: 3,
        }
    }
}

/// Manages container lifecycle operations
pub struct LifecycleManager {
    config: LifecycleConfig,
    docker_client: Arc<dyn DockerClient>,
    vault: Arc<Mutex<dyn Vault + Send>>,
    event_bus: EventBus,
    state: SharedReactorState,
    shutdown_sender: broadcast::Sender<()>,
}

impl LifecycleManager {
    /// Create a new lifecycle manager
    pub fn new(
        config: LifecycleConfig,
        docker_client: Arc<dyn DockerClient>,
        vault: Arc<Mutex<dyn Vault + Send>>,
        event_bus: EventBus,
        state: SharedReactorState,
    ) -> Self {
        let (shutdown_sender, _) = broadcast::channel(8);
        
        Self {
            config,
            docker_client,
            vault,
            event_bus,
            state,
            shutdown_sender,
        }
    }

    /// Start the lifecycle manager
    pub async fn start(&self) -> Result<()> {
        info!("Starting lifecycle manager");
        Ok(())
    }

    /// Stop a specific component
    pub async fn stop_component(&self, component_name: &str) -> Result<()> {
        info!("Stopping component: {}", component_name);
        
        // Update state
        {
            let mut state = self.state.write().await;
            state.mark_component_stopped(component_name);
        }
        
        // Publish event
        if let Err(e) = self.event_bus.publish(Event::new(
            "lifecycle",
            ContainerEvent::ContainerStopped {
                component: component_name.to_string(),
                container_id: "unknown".to_string(),
                exit_code: Some(0),
                reason: crate::events::StopReason::Shutdown,
            },
        )).await {
            debug!("Failed to publish container stopped event: {}", e);
        }
        
        Ok(())
    }

    /// Start a specific component
    pub async fn start_component(&self, component_name: &str) -> Result<()> {
        info!("Starting component: {}", component_name);
        
        // Update state
        {
            let mut state = self.state.write().await;
            state.mark_component_running(component_name, format!("container_{}", component_name));
        }
        
        // Publish event
        if let Err(e) = self.event_bus.publish(Event::new(
            "lifecycle",
            ContainerEvent::ContainerStarted {
                component: component_name.to_string(),
                container_id: format!("container_{}", component_name),
                timestamp: std::time::Instant::now(),
            },
        )).await {
            debug!("Failed to publish container started event: {}", e);
        }
        
        Ok(())
    }

    /// Stop the lifecycle manager
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping lifecycle manager");
        Ok(())
    }

    /// Start services
    pub async fn start_services(
        &self,
        services: Vec<ContainerService>,
        component_specs: &[rush_build::ComponentBuildSpec],
        built_images: &HashMap<String, String>,
    ) -> Result<Vec<DockerService>> {
        info!("Starting {} services", services.len());
        
        // Update state
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::Starting)?;
        }
        
        // Publish event
        if let Err(e) = self.event_bus.publish(Event::new(
            "lifecycle",
            ContainerEvent::NetworkReady {
                network_name: self.config.network_name.clone(),
            },
        )).await {
            debug!("Failed to publish network ready event: {}", e);
        }
        
        let mut running_services = Vec::new();
        
        for service in services {
            // Check if this service should be redirected
            if self.config.redirected_components.contains_key(&service.name) {
                info!("Skipping {} (redirected to external service)", service.name);
                continue;
            }
            
            // Check if it's a local service (they're started separately)
            let component_spec = component_specs
                .iter()
                .find(|spec| spec.component_name == service.name);
            
            if let Some(spec) = component_spec {
                if matches!(spec.build_type, rush_build::BuildType::LocalService { .. }) {
                    debug!("Skipping local service {} (managed separately)", service.name);
                    continue;
                }
            }
            
            // Start the service
            match self.start_service(
                &service,
                component_specs,
                built_images,
            ).await {
                Ok(docker_service) => {
                    info!("Successfully started {}", service.name);
                    
                    // Update state
                    {
                        let mut state = self.state.write().await;
                        state.mark_component_running(
                            &service.name,
                            docker_service.id().to_string(),
                        );
                    }
                    
                    // Publish event
                    if let Err(e) = self.event_bus.publish(Event::new(
                        "lifecycle",
                        ContainerEvent::ContainerStarted {
                            component: service.name.clone(),
                            container_id: docker_service.id().to_string(),
                            timestamp: Instant::now(),
                        },
                    )).await {
                        debug!("Failed to publish container started event: {}", e);
                    }
                    
                    running_services.push(docker_service);
                }
                Err(e) => {
                    error!("Failed to start {}: {}", service.name, e);
                    
                    // Update state
                    {
                        let mut state = self.state.write().await;
                        state.record_component_error(&service.name, e.to_string());
                    }
                    
                    // Publish error event
                    if let Err(pub_err) = self.event_bus.publish(Event::error(
                        "lifecycle",
                        format!("Failed to start {}: {}", service.name, e),
                        true,
                    )).await {
                        debug!("Failed to publish error event: {}", pub_err);
                    }
                    
                    // Continue with other services
                }
            }
        }
        
        // Update state to running
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::Running)?;
            state.set_running_services(running_services.clone());
        }
        
        info!("Started {} services successfully", running_services.len());
        Ok(running_services)
    }

    /// Start a single service
    async fn start_service(
        &self,
        service: &ContainerService,
        component_specs: &[rush_build::ComponentBuildSpec],
        built_images: &HashMap<String, String>,
    ) -> Result<DockerService> {
        debug!("Starting service: {}", service.name);
        
        // Load secrets from vault
        let secrets = {
            let vault_guard = self.vault.lock().unwrap();
            vault_guard
                .get(
                    &self.config.product_name,
                    &service.name,
                    &self.config.environment,
                )
                .await
                .unwrap_or_default()
        };
        
        // Get component spec
        let component_spec = component_specs
            .iter()
            .find(|spec| spec.component_name == service.name);
        
        // Build environment variables
        let mut env_vars = HashMap::new();
        
        // Add environment from spec
        if let Some(spec) = component_spec {
            // Add dotenv variables
            for (key, value) in &spec.dotenv {
                env_vars.insert(key.clone(), value.clone());
            }
            
            // Add env variables from YAML
            if let Some(env) = &spec.env {
                for (key, value) in env {
                    env_vars.insert(key.clone(), value.clone());
                }
            }
        }
        
        // Add secrets
        for (key, value) in secrets {
            env_vars.insert(key, value);
        }
        
        // Get the actual image name to use
        let image_name = built_images
            .get(&service.name)
            .cloned()
            .unwrap_or_else(|| service.image.clone());
        
        // Create Docker service config
        let docker_config = DockerServiceConfig {
            name: format!("{}-{}", self.config.product_name, service.name),
            image: image_name,
            network: self.config.network_name.clone(),
            env_vars,
            ports: vec![format!("{}:{}", service.port, service.target_port)],
            volumes: vec![],
        };
        
        // Create and start the container
        let mut retries = 0;
        let container_id = loop {
            // Create the container
            match self.docker_client.run_container(
                &docker_config.image,
                &docker_config.name,
                &docker_config.network,
                &docker_config.env_vars.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>(),
                &docker_config.ports,
                &docker_config.volumes,
            ).await {
                Ok(id) => break id,
                Err(e) if retries < self.config.max_retries => {
                    warn!(
                        "Failed to start {} (attempt {}/{}): {}",
                        service.name,
                        retries + 1,
                        self.config.max_retries,
                        e
                    );
                    retries += 1;
                    tokio::time::sleep(self.config.retry_delay).await;
                }
                Err(e) => {
                    error!("Failed to start {} after {} retries", service.name, self.config.max_retries);
                    return Err(e);
                }
            }
        };
        
        // Create the DockerService struct
        let docker_service = DockerService::new(
            container_id,
            docker_config,
            self.docker_client.clone(),
        );
        
        Ok(docker_service)
    }

    /// Stop all services
    pub async fn stop_services(&self, services: &[DockerService]) -> Result<()> {
        info!("Stopping {} services", services.len());
        
        // Update state
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::ShuttingDown)?;
        }
        
        // Broadcast shutdown signal
        let _ = self.shutdown_sender.send(());
        
        // Publish shutdown event
        if let Err(e) = self.event_bus.publish(Event::new(
            "lifecycle",
            ContainerEvent::ShutdownInitiated {
                reason: crate::events::ShutdownReason::UserRequested,
            },
        )).await {
            debug!("Failed to publish shutdown event: {}", e);
        }
        
        // Stop each service
        for service in services {
            match self.stop_service(service).await {
                Ok(_) => {
                    debug!("Successfully stopped container {}", service.id());
                    
                    // Update state
                    {
                        let state_read = self.state.read().await;
                        // Find component name from container ID
                        let component_name = state_read.components().values()
                            .find(|c| c.container_id.as_deref() == Some(service.id()))
                            .map(|c| c.name.clone());
                        drop(state_read);
                        
                        if let Some(name) = component_name {
                            let mut state_write = self.state.write().await;
                            state_write.mark_component_stopped(&name);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to stop container {}: {}", service.id(), e);
                    // Continue with other services
                }
            }
        }
        
        info!("All services stopped");
        Ok(())
    }

    /// Stop a single service
    async fn stop_service(&self, service: &DockerService) -> Result<()> {
        let mut retries = 0;
        
        while retries < self.config.max_retries {
            // Try to stop gracefully
            let stop_result = service.stop().await;
            let remove_result = service.remove().await;
            
            if stop_result.is_ok() && remove_result.is_ok() {
                // Publish event
                // Get service name from config
                let service_name = service.name().unwrap_or_else(|| "unknown".to_string());
                if let Err(e) = self.event_bus.publish(Event::new(
                    "lifecycle",
                    ContainerEvent::ContainerStopped {
                        component: service_name,
                        container_id: service.id().to_string(),
                        exit_code: Some(0),
                        reason: crate::events::StopReason::Shutdown,
                    },
                )).await {
                    debug!("Failed to publish container stopped event: {}", e);
                }
                
                return Ok(());
            }
            
            retries += 1;
            if retries < self.config.max_retries {
                warn!(
                    "Failed to stop container {} (attempt {}/{}), retrying...",
                    service.id(),
                    retries,
                    self.config.max_retries,
                );
                tokio::time::sleep(self.config.retry_delay).await;
            }
        }
        
        error!(
            "Failed to stop container {} after {} retries",
            service.id(),
            self.config.max_retries
        );
        
        Err(rush_core::error::Error::Docker(
            format!("Failed to stop container {}", service.id())
        ))
    }

    /// Restart a service
    pub async fn restart_service(
        &self,
        service: &DockerService,
        component_specs: &[rush_build::ComponentBuildSpec],
        built_images: &HashMap<String, String>,
    ) -> Result<DockerService> {
        let service_name = service.name().unwrap_or_else(|| "unknown".to_string());
        info!("Restarting service: {}", service_name);
        
        // Stop the service first
        self.stop_service(service).await?;
        
        // Create a ContainerService from the DockerService
        // Parse port from the first port mapping if available
        let (port, target_port) = if let Some(port_mapping) = service.config.ports.first() {
            let parts: Vec<&str> = port_mapping.split(':').collect();
            let port = parts.get(0).and_then(|p| p.parse().ok()).unwrap_or(0);
            let target_port = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(port);
            (port, target_port)
        } else {
            (0, 0)
        };
        
        let container_service = ContainerService {
            id: String::new(), // Will be assigned when started
            name: service_name.clone(),
            image: service.config.image.clone(),
            host: "localhost".to_string(),
            docker_host: "localhost".to_string(),
            port,
            target_port,
            domain: format!("{}.local", service_name),
            mount_point: None,
        };
        
        // Start it again
        self.start_service(&container_service, component_specs, built_images).await
    }

    /// Get shutdown sender for coordinated shutdown
    pub fn shutdown_sender(&self) -> broadcast::Sender<()> {
        self.shutdown_sender.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lifecycle_config_default() {
        let config = LifecycleConfig::default();
        assert_eq!(config.environment, "dev");
        assert_eq!(config.network_name, "rush-network");
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay, Duration::from_secs(1));
    }
}