//! Docker integration for the reactor
//!
//! This module integrates the enhanced Docker features with the reactor,
//! providing a unified interface for Docker operations.

use std::sync::Arc;

use log::{debug, info};
use rush_core::error::Result;

use crate::docker::{
    ConnectionPool, DockerClient, DockerClientWrapper, DockerWrapperConfig, LogStreamConfig,
    LogStreamManager, MetricsCollector, OperationType, PoolConfig, PooledDockerClient,
};
use crate::events::EventBus;
use crate::reactor::state::SharedReactorState;

/// Configuration for Docker integration
#[derive(Debug, Clone)]
pub struct DockerIntegrationConfig {
    /// Whether to use the enhanced Docker client
    pub use_enhanced_client: bool,
    /// Docker wrapper configuration
    pub wrapper_config: DockerWrapperConfig,
    /// Connection pool configuration
    pub pool_config: PoolConfig,
    /// Log streaming configuration
    pub log_config: LogStreamConfig,
    /// Whether to enable metrics collection
    pub enable_metrics: bool,
    /// Whether to use connection pooling
    pub enable_pooling: bool,
}

impl Default for DockerIntegrationConfig {
    fn default() -> Self {
        Self {
            use_enhanced_client: true,
            wrapper_config: DockerWrapperConfig::default(),
            pool_config: PoolConfig::default(),
            log_config: LogStreamConfig::default(),
            enable_metrics: true,
            enable_pooling: true,
        }
    }
}

/// Enhanced Docker integration for the reactor
pub struct DockerIntegration {
    /// Configuration
    _config: DockerIntegrationConfig,
    /// The Docker client (wrapped with enhancements)
    client: Arc<dyn DockerClient>,
    /// Log stream manager
    log_manager: Option<Arc<LogStreamManager>>,
    /// Metrics collector
    metrics: Option<Arc<MetricsCollector>>,
    /// Event bus
    event_bus: EventBus,
    /// Reactor state
    _state: SharedReactorState,
}

impl DockerIntegration {
    /// Create a new Docker integration
    pub fn new(
        base_client: Arc<dyn DockerClient>,
        config: DockerIntegrationConfig,
        event_bus: EventBus,
        state: SharedReactorState,
    ) -> Result<Self> {
        info!("Initializing Docker integration with enhanced features");

        // Create the client chain based on configuration
        let client = Self::create_client_chain(base_client, &config, event_bus.clone())?;

        // Create log manager if enabled
        let log_manager = if config.use_enhanced_client {
            let manager = LogStreamManager::new(client.clone(), config.log_config.clone());
            Some(Arc::new(manager.with_event_bus(event_bus.clone())))
        } else {
            None
        };

        // Create metrics collector if enabled
        let metrics = if config.enable_metrics {
            Some(Arc::new(MetricsCollector::new(true)))
        } else {
            None
        };

        Ok(Self {
            _config: config,
            client,
            log_manager,
            metrics,
            event_bus,
            _state: state,
        })
    }

    /// Create the client chain with all enhancements
    fn create_client_chain(
        base_client: Arc<dyn DockerClient>,
        config: &DockerIntegrationConfig,
        event_bus: EventBus,
    ) -> Result<Arc<dyn DockerClient>> {
        let mut client = base_client;

        // Add connection pooling if enabled
        if config.enable_pooling && config.pool_config.enabled {
            debug!("Enabling connection pooling");
            let pool = ConnectionPool::new(config.pool_config.clone(), move || client.clone());

            // Initialize the pool
            let pool_clone = pool.clone();
            tokio::spawn(async move {
                if let Err(e) = pool_clone.init().await {
                    log::error!("Failed to initialize connection pool: {}", e);
                }
            });

            client = Arc::new(PooledDockerClient::new(pool));
        }

        // Add retry wrapper if enhanced client is enabled
        if config.use_enhanced_client {
            debug!("Enabling enhanced Docker client with retry logic");
            let mut wrapper = DockerClientWrapper::new(client, config.wrapper_config.clone());
            wrapper.set_event_bus(event_bus);
            client = Arc::new(wrapper);
        }

        Ok(client)
    }

    /// Get the Docker client
    pub fn client(&self) -> Arc<dyn DockerClient> {
        self.client.clone()
    }

    /// Start log streaming for a container
    pub async fn start_log_streaming(&self, container_id: String, component: String) -> Result<()> {
        if let Some(manager) = &self.log_manager {
            let receiver = manager
                .start_streaming(container_id.clone(), component.clone())
                .await;

            // Spawn a task to handle log entries
            let event_bus = self.event_bus.clone();
            tokio::spawn(async move {
                let mut receiver = receiver;
                while let Some(entry) = receiver.recv().await {
                    debug!(
                        "[{}] {}: {}",
                        entry.component, entry.level as u8, entry.message
                    );

                    // Publish important log events
                    if entry.level >= crate::docker::LogLevel::Error {
                        let _ = event_bus
                            .publish(crate::events::Event::error(
                                &entry.component,
                                entry.message.clone(),
                                false,
                            ))
                            .await;
                    }
                }
            });

            info!("Started log streaming for {} ({})", component, container_id);
        }

        Ok(())
    }

    /// Stop log streaming for a container
    pub async fn stop_log_streaming(&self, container_id: &str) {
        if let Some(manager) = &self.log_manager {
            manager.stop_streaming(container_id).await;
        }
    }

    /// Start a Docker operation with metrics
    pub async fn start_operation(
        &self,
        op_type: OperationType,
    ) -> Option<crate::docker::metrics::OperationTimer> {
        if let Some(metrics) = &self.metrics {
            Some(metrics.start_operation(op_type).await)
        } else {
            None
        }
    }

    /// Get metrics report
    pub async fn get_metrics_report(&self) -> Option<crate::docker::MetricsReport> {
        if let Some(metrics) = &self.metrics {
            Some(metrics.generate_report().await)
        } else {
            None
        }
    }

    /// Get Docker statistics if using enhanced client
    pub async fn get_docker_stats(&self) -> Option<crate::docker::DockerStats> {
        // Check if client is a DockerClientWrapper
        // This would require downcasting, which is not directly possible with trait objects
        // For now, return None
        None
    }

    /// Health check with enhanced features
    pub async fn health_check(&self) -> Result<()> {
        let timer = self.start_operation(OperationType::NetworkExists).await;

        let result = self.client.network_exists("bridge").await.map(|_| ());

        if let Some(timer) = timer {
            timer.complete(result.is_ok(), false).await;
        }

        result
    }

    /// Build image with enhanced features
    pub async fn build_image(&self, tag: &str, dockerfile: &str, context: &str) -> Result<()> {
        let timer = self.start_operation(OperationType::BuildImage).await;

        info!("Building image {} with enhanced Docker client", tag);
        let result = self.client.build_image(tag, dockerfile, context).await;

        if let Some(timer) = timer {
            timer.complete(result.is_ok(), false).await;
        }

        result
    }

    /// Run container with enhanced features
    pub async fn run_container(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
    ) -> Result<String> {
        let timer = self.start_operation(OperationType::RunContainer).await;

        info!("Running container {} with enhanced Docker client", name);
        let result = self
            .client
            .run_container(image, name, network, env_vars, ports, volumes)
            .await;

        if let Some(timer) = timer {
            timer.complete(result.is_ok(), false).await;
        }

        // Start log streaming if successful
        if let Ok(ref container_id) = result {
            let _ = self
                .start_log_streaming(container_id.clone(), name.to_string())
                .await;
        }

        result
    }

    /// Stop container with enhanced features
    pub async fn stop_container(&self, id: &str) -> Result<()> {
        // Stop log streaming first
        self.stop_log_streaming(id).await;

        let timer = self.start_operation(OperationType::StopContainer).await;

        let result = self.client.stop_container(id).await;

        if let Some(timer) = timer {
            timer.complete(result.is_ok(), false).await;
        }

        result
    }

    /// Shutdown the integration
    pub async fn shutdown(&self) {
        info!("Shutting down Docker integration");

        // Stop all log streams
        if let Some(manager) = &self.log_manager {
            manager.clear_all_buffers().await;
        }

        // Connection pool shutdown handled automatically on drop
    }
}

/// Builder for Docker integration
pub struct DockerIntegrationBuilder {
    config: DockerIntegrationConfig,
    base_client: Option<Arc<dyn DockerClient>>,
    event_bus: Option<EventBus>,
    state: Option<SharedReactorState>,
}

impl Default for DockerIntegrationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerIntegrationBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: DockerIntegrationConfig::default(),
            base_client: None,
            event_bus: None,
            state: None,
        }
    }

    /// Set the configuration
    pub fn with_config(mut self, config: DockerIntegrationConfig) -> Self {
        self.config = config;
        self
    }

    /// Set the base Docker client
    pub fn with_client(mut self, client: Arc<dyn DockerClient>) -> Self {
        self.base_client = Some(client);
        self
    }

    /// Set the event bus
    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Set the reactor state
    pub fn with_state(mut self, state: SharedReactorState) -> Self {
        self.state = Some(state);
        self
    }

    /// Build the Docker integration
    pub fn build(self) -> Result<DockerIntegration> {
        let base_client = self
            .base_client
            .ok_or_else(|| rush_core::error::Error::Internal("Docker client not set".into()))?;
        let event_bus = self
            .event_bus
            .ok_or_else(|| rush_core::error::Error::Internal("Event bus not set".into()))?;
        let state = self
            .state
            .ok_or_else(|| rush_core::error::Error::Internal("Reactor state not set".into()))?;

        DockerIntegration::new(base_client, self.config, event_bus, state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_integration_config_default() {
        let config = DockerIntegrationConfig::default();
        assert!(config.use_enhanced_client);
        assert!(config.enable_metrics);
        assert!(config.enable_pooling);
    }

    #[test]
    fn test_docker_integration_builder() {
        let builder =
            DockerIntegrationBuilder::new().with_config(DockerIntegrationConfig::default());

        // Can't build without required components
        assert!(builder.build().is_err());
    }
}
