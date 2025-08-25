//! Reactor factory for creating appropriate reactor implementations
//!
//! This module provides a factory for creating either the legacy reactor
//! or the new primary reactor based on configuration.

use crate::{
    docker::DockerClient,
    reactor::{
        core::ContainerReactor,
        modular_core::{Reactor, ModularReactorConfig},
        config::ContainerReactorConfig,
    },
};
use rush_build::ComponentBuildSpec;
use rush_core::error::{Error, Result};
use std::sync::Arc;
use log::{info, warn};

/// Reactor implementation variants
pub enum ReactorImplementation {
    /// Legacy monolithic reactor
    Legacy(ContainerReactor),
    /// Primary reactor implementation
    Primary(Reactor),
}

impl ReactorImplementation {
    /// Start the reactor
    pub async fn start(&mut self) -> Result<()> {
        match self {
            ReactorImplementation::Legacy(reactor) => {
                // Legacy reactor doesn't have a separate start method
                // It starts immediately in run()
                Ok(())
            }
            ReactorImplementation::Primary(reactor) => {
                reactor.start().await
            }
        }
    }
    
    /// Run the reactor main loop
    pub async fn run(&mut self) -> Result<()> {
        match self {
            ReactorImplementation::Legacy(reactor) => {
                reactor.run().await
            }
            ReactorImplementation::Primary(reactor) => {
                reactor.run().await
            }
        }
    }
    
    /// Trigger a rebuild of all components
    pub async fn rebuild_all(&mut self) -> Result<()> {
        match self {
            ReactorImplementation::Legacy(reactor) => {
                reactor.rebuild_all().await
            }
            ReactorImplementation::Primary(reactor) => {
                reactor.rebuild_all().await
            }
        }
    }
    
    /// Build all components
    pub async fn build(&mut self) -> Result<()> {
        match self {
            ReactorImplementation::Legacy(reactor) => {
                reactor.build().await
            }
            ReactorImplementation::Primary(reactor) => {
                reactor.build().await
            }
        }
    }
    
    /// Roll out containers
    pub async fn rollout(&mut self) -> Result<()> {
        match self {
            ReactorImplementation::Legacy(reactor) => {
                reactor.rollout().await
            }
            ReactorImplementation::Primary(reactor) => {
                reactor.rollout().await
            }
        }
    }
    
    /// Build and push images
    pub async fn build_and_push(&mut self) -> Result<()> {
        match self {
            ReactorImplementation::Legacy(reactor) => {
                reactor.build_and_push().await
            }
            ReactorImplementation::Primary(reactor) => {
                reactor.build_and_push().await
            }
        }
    }
    
    /// Deploy to Kubernetes
    pub async fn deploy(&mut self) -> Result<()> {
        match self {
            ReactorImplementation::Legacy(reactor) => {
                reactor.deploy().await
            }
            ReactorImplementation::Primary(reactor) => {
                reactor.deploy().await
            }
        }
    }
    
    /// Get status information
    pub async fn status(&self) -> ReactorStatusInfo {
        match self {
            ReactorImplementation::Legacy(_reactor) => {
                ReactorStatusInfo {
                    implementation: "legacy".to_string(),
                    components: 0, // Legacy doesn't easily provide this
                    running_containers: 0,
                    phase: "unknown".to_string(),
                }
            }
            ReactorImplementation::Primary(reactor) => {
                let status = reactor.status().await;
                ReactorStatusInfo {
                    implementation: "primary".to_string(),
                    components: status.components,
                    running_containers: status.running_containers,
                    phase: format!("{:?}", status.phase),
                }
            }
        }
    }
    
    /// Shutdown the reactor gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        match self {
            ReactorImplementation::Legacy(_reactor) => {
                // Legacy reactor shutdown is handled via signals
                Ok(())
            }
            ReactorImplementation::Primary(reactor) => {
                reactor.shutdown().await
            }
        }
    }
}

/// Status information that works for both implementations
#[derive(Debug, Clone)]
pub struct ReactorStatusInfo {
    pub implementation: String,
    pub components: usize,
    pub running_containers: usize,
    pub phase: String,
}

/// Factory for creating reactor implementations
pub struct ReactorFactory;

impl ReactorFactory {
    /// Create a reactor implementation based on configuration
    pub async fn create_reactor(
        config: ModularReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
        legacy_config: Option<ContainerReactorConfig>,
    ) -> Result<ReactorImplementation> {
        if config.use_legacy {
            info!("Creating legacy reactor implementation");
            Self::create_legacy_reactor(
                legacy_config.unwrap_or_else(|| config.base.clone()),
                docker_client,
                component_specs,
            ).await
        } else {
            info!("Creating primary reactor implementation");
            Self::create_primary_reactor(config, docker_client, component_specs).await
        }
    }
    
    /// Create the legacy reactor
    async fn create_legacy_reactor(
        config: ContainerReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        warn!("Using legacy reactor implementation - consider migrating to modular");
        
        // We can't easily create the legacy reactor here since it has many dependencies
        // and a complex constructor. For now, return an error suggesting modular usage.
        Err(Error::Internal(
            "Legacy reactor creation not supported in factory. Use primary reactor or create legacy reactor directly.".into()
        ))
    }
    
    /// Create the primary reactor
    async fn create_primary_reactor(
        config: ModularReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        let reactor = Reactor::new(config, docker_client, component_specs).await?;
        Ok(ReactorImplementation::Primary(reactor))
    }
    
    /// Create a primary reactor with default configuration
    pub async fn create_default_primary_reactor(
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        let config = ModularReactorConfig::default();
        Self::create_primary_reactor(config, docker_client, component_specs).await
    }
    
    /// Create a reactor with enhanced Docker features enabled
    pub async fn create_enhanced_reactor(
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        let mut config = ModularReactorConfig::default();
        
        // Enable all enhanced features
        config.docker.use_enhanced_client = true;
        config.docker.enable_metrics = true;
        config.docker.enable_pooling = true;
        config.lifecycle.auto_restart = true;
        config.lifecycle.enable_health_checks = true;
        config.build.parallel_builds = true;
        config.build.enable_cache = true;
        
        Self::create_primary_reactor(config, docker_client, component_specs).await
    }
    
    /// Create a reactor for development (with file watching enabled)
    pub async fn create_dev_reactor(
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        let mut config = ModularReactorConfig::default();
        
        // Configure for development
        config.docker.use_enhanced_client = true;
        config.watcher.auto_rebuild = true;
        config.watcher.rebuild_cooldown = std::time::Duration::from_secs(1);
        config.lifecycle.auto_restart = true;
        config.build.parallel_builds = true;
        
        // Enable verbose logging for development
        config.docker.wrapper_config.verbose = true;
        config.watcher.handler_config.verbose = true;
        
        Self::create_primary_reactor(config, docker_client, component_specs).await
    }
    
    /// Create a reactor for production (optimized for stability)
    pub async fn create_production_reactor(
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        let mut config = ModularReactorConfig::default();
        
        // Configure for production
        config.docker.use_enhanced_client = true;
        config.docker.enable_metrics = true;
        config.docker.enable_pooling = true;
        
        // Disable file watching in production
        config.watcher.auto_rebuild = false;
        
        // Conservative retry settings
        config.docker.wrapper_config.max_retries = 5;
        config.docker.wrapper_config.max_retry_delay = std::time::Duration::from_secs(30);
        
        // Enable health checks with more conservative thresholds
        config.lifecycle.enable_health_checks = true;
        config.lifecycle.health_check_interval = std::time::Duration::from_secs(10);
        config.lifecycle.max_restart_attempts = 3;
        
        Self::create_primary_reactor(config, docker_client, component_specs).await
    }
}

/// Builder for reactor configuration
pub struct ModularReactorConfigBuilder {
    config: ModularReactorConfig,
}

impl ModularReactorConfigBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: ModularReactorConfig::default(),
        }
    }
    
    /// Use legacy reactor implementation
    pub fn use_legacy(mut self, use_legacy: bool) -> Self {
        self.config.use_legacy = use_legacy;
        self
    }
    
    /// Enable or disable enhanced Docker features
    pub fn with_enhanced_docker(mut self, enabled: bool) -> Self {
        self.config.docker.use_enhanced_client = enabled;
        self.config.docker.enable_metrics = enabled;
        self.config.docker.enable_pooling = enabled;
        self
    }
    
    /// Configure file watching
    pub fn with_file_watching(mut self, enabled: bool) -> Self {
        self.config.watcher.auto_rebuild = enabled;
        self
    }
    
    /// Configure automatic restarts
    pub fn with_auto_restart(mut self, enabled: bool) -> Self {
        self.config.lifecycle.auto_restart = enabled;
        self
    }
    
    /// Enable health checks
    pub fn with_health_checks(mut self, enabled: bool) -> Self {
        self.config.lifecycle.enable_health_checks = enabled;
        self
    }
    
    /// Enable parallel builds
    pub fn with_parallel_builds(mut self, enabled: bool) -> Self {
        self.config.build.parallel_builds = enabled;
        self
    }
    
    /// Enable verbose logging
    pub fn with_verbose_logging(mut self, enabled: bool) -> Self {
        self.config.docker.wrapper_config.verbose = enabled;
        self.config.watcher.handler_config.verbose = enabled;
        self
    }
    
    /// Build the configuration
    pub fn build(self) -> ModularReactorConfig {
        self.config
    }
    
    /// Build and create a reactor
    pub async fn create_reactor(
        self,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        ReactorFactory::create_primary_reactor(self.config, docker_client, component_specs).await
    }
}

impl Default for ModularReactorConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reactor_config_builder() {
        let config = ModularReactorConfigBuilder::new()
            .use_legacy(false)
            .with_enhanced_docker(true)
            .with_file_watching(true)
            .with_auto_restart(true)
            .with_health_checks(true)
            .with_parallel_builds(true)
            .with_verbose_logging(true)
            .build();
        
        assert!(!config.use_legacy);
        assert!(config.docker.use_enhanced_client);
        assert!(config.watcher.auto_rebuild);
        assert!(config.lifecycle.auto_restart);
        assert!(config.lifecycle.enable_health_checks);
        assert!(config.build.parallel_builds);
        assert!(config.docker.wrapper_config.verbose);
    }
    
    #[test]
    fn test_reactor_status_info() {
        let status = ReactorStatusInfo {
            implementation: "modular".to_string(),
            components: 5,
            running_containers: 3,
            phase: "Running".to_string(),
        };
        
        assert_eq!(status.implementation, "modular");
        assert_eq!(status.components, 5);
        assert_eq!(status.running_containers, 3);
        assert_eq!(status.phase, "Running");
    }
}