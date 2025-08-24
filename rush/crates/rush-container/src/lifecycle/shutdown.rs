//! Graceful shutdown management
//!
//! This module handles graceful shutdown of containers and cleanup operations.

use crate::{
    docker::{DockerClient, DockerService},
    events::{Event, EventBus, ContainerEvent, ShutdownReason, StopReason},
    reactor::state::{SharedReactorState, ReactorPhase},
};
use rush_build::BuildType;
use rush_core::error::Result;
use std::sync::Arc;
use std::time::Duration;
use log::{debug, error, info, trace, warn};
use tokio::time::sleep;

/// Shutdown configuration
#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    /// Grace period before forceful termination
    pub grace_period: Duration,
    /// Maximum retries for stop operations
    pub max_retries: u32,
    /// Delay between retries
    pub retry_delay: Duration,
    /// Whether to preserve local services
    pub preserve_local_services: bool,
    /// Timeout for individual container operations
    pub operation_timeout: Duration,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            grace_period: Duration::from_secs(10),
            max_retries: 3,
            retry_delay: Duration::from_secs(1),
            preserve_local_services: true,
            operation_timeout: Duration::from_secs(30),
        }
    }
}

/// Strategy for shutting down containers
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShutdownStrategy {
    /// Stop containers gracefully with SIGTERM
    Graceful,
    /// Force stop with SIGKILL
    Forced,
    /// Stop gracefully, then force if needed
    GracefulThenForced,
}

/// Manages graceful shutdown of containers
pub struct ShutdownManager {
    config: ShutdownConfig,
    docker_client: Arc<dyn DockerClient>,
    event_bus: EventBus,
    state: SharedReactorState,
}

impl ShutdownManager {
    /// Create a new shutdown manager
    pub fn new(
        config: ShutdownConfig,
        docker_client: Arc<dyn DockerClient>,
        event_bus: EventBus,
        state: SharedReactorState,
    ) -> Self {
        Self {
            config,
            docker_client,
            event_bus,
            state,
        }
    }

    /// Initiate shutdown of all services
    pub async fn shutdown_all(
        &self,
        services: &[DockerService],
        reason: ShutdownReason,
        strategy: ShutdownStrategy,
    ) -> Result<()> {
        info!("Initiating shutdown of {} services: {:?}", services.len(), reason);
        
        // Update state
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::ShuttingDown)?;
        }
        
        // Publish shutdown event
        if let Err(e) = self.event_bus.publish(Event::new(
            "shutdown",
            ContainerEvent::ShutdownInitiated { reason: reason.clone() },
        )).await {
            debug!("Failed to publish shutdown event: {}", e);
        }
        
        // Shutdown each service
        for service in services {
            let service_name = service.name().unwrap_or_else(|| "unknown".to_string());
            match self.shutdown_service(service, strategy).await {
                Ok(_) => {
                    debug!("Successfully shut down {}", service_name);
                    
                    // Update state
                    {
                        let mut state = self.state.write().await;
                        // Find component by container ID
                        let components = state.components().clone();
                        if let Some((name, _)) = components.iter()
                            .find(|(_, c)| c.container_id.as_deref() == Some(service.id()))
                        {
                            state.mark_component_stopped(name);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to shut down {}: {}", service_name, e);
                    // Continue with other services
                }
            }
        }
        
        // Update state to terminated
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::Terminated)?;
        }
        
        info!("Shutdown complete");
        Ok(())
    }

    /// Shutdown a single service
    async fn shutdown_service(
        &self,
        service: &DockerService,
        strategy: ShutdownStrategy,
    ) -> Result<()> {
        match strategy {
            ShutdownStrategy::Graceful => {
                self.stop_gracefully(service).await
            }
            ShutdownStrategy::Forced => {
                self.force_stop(service.id()).await
            }
            ShutdownStrategy::GracefulThenForced => {
                match tokio::time::timeout(
                    self.config.grace_period,
                    self.stop_gracefully(service)
                ).await {
                    Ok(Ok(_)) => Ok(()),
                    _ => {
                        let service_name = service.name().unwrap_or_else(|| "unknown".to_string());
                        warn!("Graceful shutdown timed out for {}, forcing stop", service_name);
                        self.force_stop(service.id()).await
                    }
                }
            }
        }
    }

    /// Stop a service gracefully
    async fn stop_gracefully(&self, service: &DockerService) -> Result<()> {
        let service_name = service.name().unwrap_or_else(|| "unknown".to_string());
        debug!("Stopping {} gracefully", service_name);
        
        // Send stop signal
        service.stop().await?;
        
        // Wait for container to stop
        let start = std::time::Instant::now();
        while start.elapsed() < self.config.operation_timeout {
            match self.docker_client.container_status(service.id()).await {
                Ok(status) if !status.is_running() => {
                    // Container stopped
                    break;
                }
                _ => {
                    sleep(Duration::from_millis(500)).await;
                }
            }
        }
        
        // Remove container
        service.remove().await?;
        
        // Publish event
        let service_name = service.name().unwrap_or_else(|| "unknown".to_string());
        if let Err(e) = self.event_bus.publish(Event::new(
            "shutdown",
            ContainerEvent::ContainerStopped {
                component: service_name,
                container_id: service.id().to_string(),
                exit_code: Some(0),
                reason: StopReason::Shutdown,
            },
        )).await {
            debug!("Failed to publish container stopped event: {}", e);
        }
        
        Ok(())
    }

    /// Force stop a container with retries
    pub async fn force_stop(&self, container_id: &str) -> Result<()> {
        warn!("Force stopping container {}", container_id);
        
        let mut retries = 0;
        while retries < self.config.max_retries {
            // Try to stop the container
            match self.docker_client.stop_container(container_id).await {
                Ok(_) => {
                    debug!("Container {} stopped", container_id);
                }
                Err(e) if retries < self.config.max_retries - 1 => {
                    warn!(
                        "Failed to stop container {} (attempt {}/{}): {}",
                        container_id,
                        retries + 1,
                        self.config.max_retries,
                        e
                    );
                }
                Err(e) => {
                    error!("Failed to stop container {} after {} attempts", container_id, self.config.max_retries);
                    return Err(e);
                }
            }
            
            // Try to remove the container
            match self.docker_client.remove_container(container_id).await {
                Ok(_) => {
                    info!("Container {} removed", container_id);
                    
                    // Publish event
                    if let Err(e) = self.event_bus.publish(Event::new(
                        "shutdown",
                        ContainerEvent::ContainerStopped {
                            component: container_id.to_string(),
                            container_id: container_id.to_string(),
                            exit_code: None,
                            reason: StopReason::Killed,
                        },
                    )).await {
                        debug!("Failed to publish container stopped event: {}", e);
                    }
                    
                    return Ok(());
                }
                Err(e) if retries < self.config.max_retries - 1 => {
                    warn!(
                        "Failed to remove container {} (attempt {}/{}): {}",
                        container_id,
                        retries + 1,
                        self.config.max_retries,
                        e
                    );
                    retries += 1;
                    sleep(self.config.retry_delay).await;
                }
                Err(e) => {
                    error!("Failed to remove container {} after {} attempts", container_id, self.config.max_retries);
                    return Err(e);
                }
            }
        }
        
        Ok(())
    }

    /// Clean up containers by name pattern
    pub async fn cleanup_by_name(
        &self,
        product_name: &str,
        component_specs: &[rush_build::ComponentBuildSpec],
    ) -> Result<()> {
        trace!("Cleaning up containers by name pattern");
        
        for spec in component_specs {
            // Skip local services if configured
            if self.config.preserve_local_services {
                if matches!(spec.build_type, BuildType::LocalService { .. }) {
                    debug!("Preserving local service {}", spec.component_name);
                    continue;
                }
            }
            
            // Build container name
            let container_name = format!("{}-{}", product_name, spec.component_name);
            
            // Check if container exists
            match self.docker_client.container_exists(&container_name).await {
                Ok(true) => {
                    info!("Cleaning up container {}", container_name);
                    
                    // Get container ID
                    match self.docker_client.get_container_by_name(&container_name).await {
                        Ok(container_id) => {
                            self.force_stop(&container_id).await?;
                        }
                        Err(e) => {
                            warn!("Failed to get container ID for {}: {}", container_name, e);
                        }
                    }
                }
                Ok(false) => {
                    debug!("Container {} does not exist", container_name);
                }
                Err(e) => {
                    warn!("Failed to check container {}: {}", container_name, e);
                }
            }
        }
        
        trace!("Container cleanup complete");
        Ok(())
    }

    /// Emergency shutdown - force stop all containers immediately
    pub async fn emergency_shutdown(&self, container_ids: &[String]) -> Result<()> {
        error!("EMERGENCY SHUTDOWN - Force stopping all containers");
        
        // Publish emergency shutdown event
        if let Err(e) = self.event_bus.publish(Event::new(
            "shutdown",
            ContainerEvent::ShutdownInitiated {
                reason: ShutdownReason::Error("Emergency shutdown".to_string()),
            },
        )).await {
            debug!("Failed to publish emergency shutdown event: {}", e);
        }
        
        // Force stop all containers in parallel
        let mut tasks = Vec::new();
        for container_id in container_ids {
            let docker_client = self.docker_client.clone();
            let id = container_id.clone();
            
            tasks.push(tokio::spawn(async move {
                // Best effort - ignore errors
                let _ = docker_client.stop_container(&id).await;
                let _ = docker_client.remove_container(&id).await;
            }));
        }
        
        // Wait for all tasks
        for task in tasks {
            let _ = task.await;
        }
        
        info!("Emergency shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shutdown_config_default() {
        let config = ShutdownConfig::default();
        assert_eq!(config.grace_period, Duration::from_secs(10));
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay, Duration::from_secs(1));
        assert!(config.preserve_local_services);
        assert_eq!(config.operation_timeout, Duration::from_secs(30));
    }
    
    #[test]
    fn test_shutdown_strategy_equality() {
        assert_eq!(ShutdownStrategy::Graceful, ShutdownStrategy::Graceful);
        assert_ne!(ShutdownStrategy::Graceful, ShutdownStrategy::Forced);
        assert_ne!(ShutdownStrategy::Forced, ShutdownStrategy::GracefulThenForced);
    }
}