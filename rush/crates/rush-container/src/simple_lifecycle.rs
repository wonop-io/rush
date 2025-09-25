//! Simplified lifecycle manager using SimpleDocker
//!
//! This module provides a dramatically simplified lifecycle manager that uses
//! the SimpleDocker implementation for container management.
//! Reduces complexity from 1000+ lines to ~400 lines.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use log::{debug, error, info, warn};
use rush_core::error::{Error, Result};
use rush_core::naming::NamingConvention;
use rush_output::simple::Sink;
use rush_security::Vault;
use tokio::sync::broadcast;

use crate::events::{ContainerEvent, Event, EventBus};
use crate::reactor::state::{ReactorPhase, SharedReactorState};
use crate::service::ContainerService;
use crate::simple_docker::{RunOptions, SimpleDocker};

/// Configuration for the simple lifecycle manager
#[derive(Debug, Clone)]
pub struct SimpleLifecycleConfig {
    /// Product name
    pub product_name: String,
    /// Environment (dev, staging, prod)
    pub environment: String,
    /// Docker network name
    pub network_name: String,
    /// Components to redirect (skip starting)
    pub redirected_components: HashMap<String, (String, u16)>,
    /// Whether to automatically restart failed containers (compatibility field)
    pub auto_restart: bool,
    /// Whether to enable health checks (compatibility field)
    pub enable_health_checks: bool,
    /// Health check interval (compatibility field)
    pub health_check_interval: std::time::Duration,
    /// Maximum restart attempts (compatibility field)
    pub max_restart_attempts: u32,
}

impl Default for SimpleLifecycleConfig {
    fn default() -> Self {
        Self {
            product_name: String::new(),
            environment: "dev".to_string(),
            network_name: "rush-network".to_string(),
            redirected_components: HashMap::new(),
            auto_restart: true,
            enable_health_checks: true,
            health_check_interval: std::time::Duration::from_secs(30),
            max_restart_attempts: 3,
        }
    }
}

/// Type alias for the output sink to reduce complexity
type OutputSink = Arc<tokio::sync::RwLock<Option<Arc<tokio::sync::Mutex<Box<dyn Sink>>>>>>;

/// Simplified lifecycle manager using SimpleDocker
pub struct SimpleLifecycleManager {
    config: SimpleLifecycleConfig,
    docker: SimpleDocker,
    vault: Arc<Mutex<dyn Vault + Send>>,
    event_bus: EventBus,
    state: SharedReactorState,
    shutdown_sender: broadcast::Sender<()>,
    /// Output sink for container logs
    output_sink: OutputSink,
}

impl SimpleLifecycleManager {
    /// Create a new simple lifecycle manager
    pub fn new(
        config: SimpleLifecycleConfig,
        vault: Arc<Mutex<dyn Vault + Send>>,
        event_bus: EventBus,
        state: SharedReactorState,
    ) -> Self {
        let (shutdown_sender, _) = broadcast::channel(8);

        Self {
            config,
            docker: SimpleDocker::new(),
            vault,
            event_bus,
            state,
            shutdown_sender,
            output_sink: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Set the output sink for container logs
    pub async fn set_output_sink(&mut self, sink: Arc<tokio::sync::Mutex<Box<dyn Sink>>>) {
        self.docker.set_output_sink(sink.clone());
        let mut output_sink = self.output_sink.write().await;
        *output_sink = Some(sink);
    }

    /// Start services with dependency management (delegates to start_services for now)
    /// Returns `Vec<DockerService>` for compatibility with Reactor
    pub async fn start_services_with_dependencies(
        &self,
        services: Vec<ContainerService>,
        component_specs: &[rush_build::ComponentBuildSpec],
        built_images: &HashMap<String, String>,
    ) -> Result<Vec<crate::docker::DockerService>> {
        // For now, just delegate to start_services
        // In future, could implement dependency ordering here if needed
        let container_names = self
            .start_services(services, component_specs, built_images)
            .await?;

        // Convert container names to DockerService for compatibility
        // This is a temporary shim until we fully migrate away from DockerService
        let docker_services = container_names
            .into_iter()
            .map(|name| {
                crate::docker::DockerService::new(
                    name.clone(),
                    crate::docker::DockerServiceConfig {
                        name: name.clone(),
                        image: String::new(), // Not used by reactor
                        network: self.config.network_name.clone(),
                        env_vars: HashMap::new(),
                        ports: vec![],
                        volumes: vec![],
                    },
                    self.docker_client(),
                )
            })
            .collect();

        Ok(docker_services)
    }

    /// Get a mock docker client for compatibility (public for reactor)
    pub fn docker_client(&self) -> Arc<dyn crate::docker::DockerClient> {
        // Return a simple stub client - this is only used for the DockerService shim
        // which the reactor doesn't actually use for operations
        Arc::new(crate::docker::DockerCliClient::new("docker".to_string()))
    }

    /// Start the lifecycle manager (no-op for simple version)
    pub async fn start(&self) -> Result<()> {
        info!("Starting SimpleLifecycleManager");
        Ok(())
    }

    /// Stop the lifecycle manager (no-op for simple version)
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping SimpleLifecycleManager");
        Ok(())
    }

    /// Create a shutdown manager (returns a stub for compatibility)
    pub fn shutdown_manager(&self) -> crate::lifecycle::shutdown::ShutdownManager {
        // Create a minimal shutdown manager for compatibility
        crate::lifecycle::shutdown::ShutdownManager::new(
            crate::lifecycle::shutdown::ShutdownConfig::default(),
            self.docker_client(),
            self.event_bus.clone(),
            self.state.clone(),
        )
    }

    /// Stop specific services (compatibility method)
    pub async fn stop_services(&self, services: &[crate::docker::DockerService]) -> Result<()> {
        info!("Stopping {} services", services.len());

        for service in services {
            // Extract component name from service
            let container_name = service.name().unwrap_or_else(|| service.id().to_string());

            if self.docker.exists(&container_name).await? {
                self.docker.stop(&container_name).await?;
                self.docker.remove(&container_name).await?;
            }
        }

        Ok(())
    }

    /// Start services (simplified version without dependency management)
    pub async fn start_services(
        &self,
        services: Vec<ContainerService>,
        component_specs: &[rush_build::ComponentBuildSpec],
        built_images: &HashMap<String, String>,
    ) -> Result<Vec<String>> {
        info!("Starting {} services with SimpleDocker", services.len());

        // Create network first
        self.docker
            .create_network(&self.config.network_name)
            .await?;

        // Publish network ready event
        let _ = self
            .event_bus
            .publish(Event::new(
                "lifecycle",
                ContainerEvent::NetworkReady {
                    network_name: self.config.network_name.clone(),
                },
            ))
            .await;

        let mut running_containers = Vec::new();

        for service in services {
            // Check if redirected
            if self
                .config
                .redirected_components
                .contains_key(&service.name)
            {
                info!("Skipping {} (redirected)", service.name);
                continue;
            }

            // Check if local service
            let component_spec = component_specs
                .iter()
                .find(|spec| spec.component_name == service.name);

            if let Some(spec) = component_spec {
                if matches!(spec.build_type, rush_build::BuildType::LocalService { .. }) {
                    debug!("Skipping local service {}", service.name);
                    continue;
                }
            }

            // Start the service
            match self
                .start_service(&service, component_specs, built_images)
                .await
            {
                Ok(container_name) => {
                    info!("Started {} as container {}", service.name, container_name);

                    // Update state
                    {
                        let mut state = self.state.write().await;
                        state.mark_component_running(&service.name, container_name.clone());
                    }

                    // Publish event
                    let _ = self
                        .event_bus
                        .publish(Event::new(
                            "lifecycle",
                            ContainerEvent::ContainerStarted {
                                component: service.name.clone(),
                                container_id: container_name.clone(),
                                timestamp: Instant::now(),
                            },
                        ))
                        .await;

                    running_containers.push(container_name);
                }
                Err(e) => {
                    error!("Failed to start {}: {}", service.name, e);

                    // Update state with error
                    {
                        let mut state = self.state.write().await;
                        state.record_component_error(&service.name, e.to_string());
                    }

                    // Publish error event
                    let _ = self
                        .event_bus
                        .publish(Event::error(
                            "lifecycle",
                            format!("Failed to start {}: {}", service.name, e),
                            true,
                        ))
                        .await;

                    return Err(e);
                }
            }
        }

        info!(
            "Started {} containers successfully",
            running_containers.len()
        );
        Ok(running_containers)
    }

    /// Start a single service using SimpleDocker
    async fn start_service(
        &self,
        service: &ContainerService,
        component_specs: &[rush_build::ComponentBuildSpec],
        built_images: &HashMap<String, String>,
    ) -> Result<String> {
        debug!("Starting service {} with SimpleDocker", service.name);

        // Load secrets from vault
        let secrets = {
            let vault = self.vault.clone();
            let product_name = self.config.product_name.clone();
            let service_name = service.name.clone();
            let environment = self.config.environment.clone();

            async move {
                let vault_guard = vault.lock().unwrap();
                vault_guard
                    .get(&product_name, &service_name, &environment)
                    .await
                    .unwrap_or_default()
            }
            .await
        };

        // Get component spec
        let component_spec = component_specs
            .iter()
            .find(|spec| spec.component_name == service.name);

        // Build environment variables
        let mut env_vars = Vec::new();

        // Add environment from spec
        if let Some(spec) = component_spec {
            // Add dotenv variables
            for (key, value) in &spec.dotenv {
                env_vars.push(format!("{key}={value}"));
            }

            // Add dotenv secrets
            for (key, value) in &spec.dotenv_secrets {
                env_vars.push(format!("{key}={value}"));
            }

            // Add env variables from YAML
            if let Some(env) = &spec.env {
                for (key, value) in env {
                    env_vars.push(format!("{key}={value}"));
                }
            }
        }

        // Add secrets
        for (key, value) in secrets {
            env_vars.push(format!("{key}={value}"));
        }

        // Get the actual image name
        let image_name = built_images
            .get(&service.name)
            .cloned()
            .unwrap_or_else(|| service.image.clone());

        // Generate container name
        let container_name =
            NamingConvention::container_name(&self.config.product_name, &service.name);

        // Clean up any existing container
        if self.docker.exists(&container_name).await? {
            info!("Removing existing container: {container_name}");
            self.docker.stop(&container_name).await?;
            self.docker.remove(&container_name).await?;
        }

        // Create run options
        let run_options = RunOptions {
            name: container_name.clone(),
            image: image_name,
            network: Some(self.config.network_name.clone()),
            env_vars,
            ports: vec![format!("{}:{}", service.port, service.target_port)],
            volumes: vec![],
            extra_args: vec![],
            workdir: None,
            command: None,
            detached: false,
        };

        // Run the container interactively with output streaming
        let container_id = self.docker.run_interactive(run_options).await?;
        info!("Container {container_name} started with ID: {container_id}");

        Ok(container_name)
    }

    /// Stop all services
    pub async fn stop_all_services(&self) -> Result<()> {
        info!("Stopping all services with SimpleDocker");

        // Update state if not already shutting down
        {
            let mut state = self.state.write().await;
            if state.phase() != &ReactorPhase::ShuttingDown {
                state.transition_to(ReactorPhase::ShuttingDown)?;
            }
        }

        // Broadcast shutdown
        let _ = self.shutdown_sender.send(());

        // Publish shutdown event
        let _ = self
            .event_bus
            .publish(Event::new(
                "lifecycle",
                ContainerEvent::ShutdownInitiated {
                    reason: crate::events::ShutdownReason::UserRequested,
                },
            ))
            .await;

        // Get all running containers
        let containers = self.docker.list().await?;
        info!("Found {} containers to stop", containers.len());

        // Stop each container
        for container_name in containers {
            // Only stop containers managed by this product
            if container_name.starts_with(&self.config.product_name) {
                match self.docker.stop(&container_name).await {
                    Ok(_) => {
                        info!("Stopped container: {container_name}");

                        // Extract component name from container name
                        let component_name = container_name
                            .strip_prefix(&format!("{}-", self.config.product_name))
                            .unwrap_or(&container_name)
                            .to_string();

                        // Update state
                        {
                            let mut state = self.state.write().await;
                            state.mark_component_stopped(&component_name);
                        }

                        // Publish stopped event
                        let _ = self
                            .event_bus
                            .publish(Event::new(
                                "lifecycle",
                                ContainerEvent::ContainerStopped {
                                    component: component_name,
                                    container_id: container_name.clone(),
                                    exit_code: Some(0),
                                    reason: crate::events::StopReason::Shutdown,
                                },
                            ))
                            .await;
                    }
                    Err(e) => {
                        warn!("Failed to stop container {container_name}: {e}");
                    }
                }

                // Remove the container
                if let Err(e) = self.docker.remove(&container_name).await {
                    warn!("Failed to remove container {container_name}: {e}");
                }
            }
        }

        info!("All services stopped");
        Ok(())
    }

    /// Shutdown the lifecycle manager
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down SimpleLifecycleManager");
        self.docker.shutdown().await?;
        Ok(())
    }

    /// Stop a specific component
    pub async fn stop_component(&self, component_name: &str) -> Result<()> {
        info!("Stopping component: {component_name}");

        let container_name =
            NamingConvention::container_name(&self.config.product_name, component_name);

        if self.docker.exists(&container_name).await? {
            self.docker.stop(&container_name).await?;
            self.docker.remove(&container_name).await?;
        }

        // Update state
        {
            let mut state = self.state.write().await;
            state.mark_component_stopped(component_name);
        }

        // Publish event
        let _ = self
            .event_bus
            .publish(Event::new(
                "lifecycle",
                ContainerEvent::ContainerStopped {
                    component: component_name.to_string(),
                    container_id: container_name,
                    exit_code: Some(0),
                    reason: crate::events::StopReason::Shutdown,
                },
            ))
            .await;

        Ok(())
    }

    /// Restart a component
    pub async fn restart_component(
        &self,
        component_name: &str,
        services: &[ContainerService],
        component_specs: &[rush_build::ComponentBuildSpec],
        built_images: &HashMap<String, String>,
    ) -> Result<String> {
        info!("Restarting component: {component_name}");

        // Stop it first
        self.stop_component(component_name).await?;

        // Find the service configuration
        let service = services
            .iter()
            .find(|s| s.name == component_name)
            .ok_or_else(|| Error::Docker(format!("Service {component_name} not found")))?;

        // Start it again
        self.start_service(service, component_specs, built_images)
            .await
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
    fn test_simple_lifecycle_config_default() {
        let config = SimpleLifecycleConfig::default();
        assert_eq!(config.environment, "dev");
        assert_eq!(config.network_name, "rush-network");
        assert!(config.redirected_components.is_empty());
    }
}
