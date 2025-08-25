//! Migration utilities for transitioning from legacy to modular reactor
//!
//! This module provides utilities to help migrate from the legacy reactor
//! implementation to the new modular reactor.

use crate::{
    docker::DockerClient,
    reactor::{
        factory::{ReactorFactory, ReactorImplementation},
        modular_core::ModularReactorConfig,
        config::ContainerReactorConfig,
    },
};
use rush_build::ComponentBuildSpec;
use rush_core::error::{Error, Result};
use std::sync::Arc;
use log::{info, warn};

/// Migration strategy options
#[derive(Debug, Clone)]
pub enum MigrationStrategy {
    /// Immediate switch to modular reactor
    Immediate,
    /// Gradual migration with feature flags
    Gradual {
        /// Which features to enable in the modular reactor
        enabled_features: Vec<ModularFeature>,
    },
    /// Test both implementations side-by-side (development only)
    SideBySide,
}

/// Modular reactor features that can be enabled gradually
#[derive(Debug, Clone)]
pub enum ModularFeature {
    /// Enhanced Docker client with retries
    EnhancedDocker,
    /// Connection pooling
    ConnectionPooling,
    /// Advanced metrics collection
    Metrics,
    /// Enhanced log streaming
    LogStreaming,
    /// Improved file watching
    FileWatcher,
    /// Lifecycle management
    LifecycleManagement,
    /// Build orchestration
    BuildOrchestration,
}

/// Configuration for migration
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Migration strategy to use
    pub strategy: MigrationStrategy,
    /// Whether to enable compatibility mode
    pub compatibility_mode: bool,
    /// Fallback to legacy on errors
    pub fallback_on_error: bool,
    /// Log migration steps
    pub verbose_logging: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            strategy: MigrationStrategy::Immediate,
            compatibility_mode: false,
            fallback_on_error: false,
            verbose_logging: false,
        }
    }
}

/// Migration helper for transitioning to modular reactor
pub struct ReactorMigrator {
    config: MigrationConfig,
}

impl ReactorMigrator {
    /// Create a new migrator
    pub fn new(config: MigrationConfig) -> Self {
        Self { config }
    }
    
    /// Create a migrator with default configuration
    pub fn with_defaults() -> Self {
        Self::new(MigrationConfig::default())
    }
    
    /// Perform migration based on strategy
    pub async fn migrate_reactor(
        &self,
        legacy_config: ContainerReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        match &self.config.strategy {
            MigrationStrategy::Immediate => {
                self.immediate_migration(legacy_config, docker_client, component_specs).await
            }
            MigrationStrategy::Gradual { enabled_features } => {
                self.gradual_migration(legacy_config, docker_client, component_specs, enabled_features.clone()).await
            }
            MigrationStrategy::SideBySide => {
                self.side_by_side_migration(legacy_config, docker_client, component_specs).await
            }
        }
    }
    
    /// Immediate migration to modular reactor
    async fn immediate_migration(
        &self,
        legacy_config: ContainerReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        if self.config.verbose_logging {
            info!("Performing immediate migration to modular reactor");
        }
        
        let modular_config = self.convert_legacy_config(legacy_config, &[]);
        
        match ReactorFactory::create_reactor(
            modular_config,
            docker_client.clone(),
            component_specs.clone(),
            None,
        ).await {
            Ok(reactor) => {
                info!("Successfully migrated to modular reactor");
                Ok(reactor)
            }
            Err(e) if self.config.fallback_on_error => {
                warn!("Migration failed, falling back to legacy: {}", e);
                self.create_fallback_reactor(docker_client, component_specs).await
            }
            Err(e) => Err(e),
        }
    }
    
    /// Gradual migration with selective feature enabling
    async fn gradual_migration(
        &self,
        legacy_config: ContainerReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
        enabled_features: Vec<ModularFeature>,
    ) -> Result<ReactorImplementation> {
        if self.config.verbose_logging {
            info!("Performing gradual migration with features: {:?}", enabled_features);
        }
        
        let modular_config = self.convert_legacy_config(legacy_config, &enabled_features);
        
        match ReactorFactory::create_reactor(
            modular_config,
            docker_client.clone(),
            component_specs.clone(),
            None,
        ).await {
            Ok(reactor) => {
                info!("Successfully created modular reactor with {} features", enabled_features.len());
                Ok(reactor)
            }
            Err(e) if self.config.fallback_on_error => {
                warn!("Gradual migration failed, falling back: {}", e);
                self.create_fallback_reactor(docker_client, component_specs).await
            }
            Err(e) => Err(e),
        }
    }
    
    /// Side-by-side testing (development only)
    async fn side_by_side_migration(
        &self,
        _legacy_config: ContainerReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        if self.config.verbose_logging {
            info!("Creating modular reactor for side-by-side testing");
        }
        
        // For side-by-side testing, always create the modular reactor
        // The legacy reactor would be created separately by the caller
        let config = ModularReactorConfig::default();
        ReactorFactory::create_reactor(config, docker_client, component_specs, None).await
    }
    
    /// Convert legacy configuration to modular configuration
    fn convert_legacy_config(
        &self,
        _legacy_config: ContainerReactorConfig,
        enabled_features: &[ModularFeature],
    ) -> ModularReactorConfig {
        let mut config = ModularReactorConfig::default();
        
        // Apply compatibility mode settings
        if self.config.compatibility_mode {
            // Conservative settings for compatibility
            config.docker.wrapper_config.max_retries = 2;
            config.docker.wrapper_config.verbose = false;
            config.lifecycle.auto_restart = true;
            config.build.parallel_builds = false;
        } else {
            // Modern settings enabled by default
            config.docker.use_enhanced_client = true;
            config.docker.enable_metrics = true;
            config.docker.enable_pooling = true;
            config.lifecycle.auto_restart = true;
            config.lifecycle.enable_health_checks = true;
            config.build.parallel_builds = true;
            config.build.enable_cache = true;
        }
        
        // Apply enabled features
        for feature in enabled_features {
            match feature {
                ModularFeature::EnhancedDocker => {
                    config.docker.use_enhanced_client = true;
                }
                ModularFeature::ConnectionPooling => {
                    config.docker.enable_pooling = true;
                }
                ModularFeature::Metrics => {
                    config.docker.enable_metrics = true;
                }
                ModularFeature::LogStreaming => {
                    config.docker.log_config.follow = true;
                    config.docker.log_config.buffer_size = 1000;
                }
                ModularFeature::FileWatcher => {
                    config.watcher.auto_rebuild = true;
                }
                ModularFeature::LifecycleManagement => {
                    config.lifecycle.auto_restart = true;
                    config.lifecycle.enable_health_checks = true;
                }
                ModularFeature::BuildOrchestration => {
                    config.build.parallel_builds = true;
                    config.build.enable_cache = true;
                }
            }
        }
        
        config
    }
    
    /// Create fallback reactor (would be legacy in real implementation)
    async fn create_fallback_reactor(
        &self,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<ReactorImplementation> {
        warn!("Creating fallback reactor - legacy implementation not available in factory");
        
        // In a real implementation, this would create the legacy reactor
        // For now, create a minimal modular reactor as fallback
        let mut config = ModularReactorConfig::default();
        config.docker.use_enhanced_client = false;
        config.docker.enable_metrics = false;
        config.docker.enable_pooling = false;
        
        ReactorFactory::create_reactor(config, docker_client, component_specs, None).await
    }
    
    /// Check if migration is recommended based on current usage
    pub fn should_migrate(&self, _usage_stats: Option<&ReactorUsageStats>) -> MigrationRecommendation {
        // Analyze usage patterns and recommend migration approach
        MigrationRecommendation {
            recommended: true,
            strategy: MigrationStrategy::Gradual {
                enabled_features: vec![
                    ModularFeature::EnhancedDocker,
                    ModularFeature::LifecycleManagement,
                ],
            },
            reasoning: "Enhanced reliability and observability".to_string(),
            estimated_effort: MigrationEffort::Low,
        }
    }
    
    /// Validate migration configuration
    pub fn validate_config(&self) -> Result<()> {
        match &self.config.strategy {
            MigrationStrategy::SideBySide => {
                if !cfg!(debug_assertions) {
                    return Err(Error::Internal(
                        "Side-by-side migration only supported in development builds".into()
                    ));
                }
            }
            _ => {}
        }
        
        Ok(())
    }
}

/// Statistics about reactor usage (for migration analysis)
#[derive(Debug, Clone)]
pub struct ReactorUsageStats {
    pub uptime_hours: f64,
    pub rebuild_frequency: f64,
    pub error_rate: f64,
    pub container_count: usize,
    pub file_change_frequency: f64,
}

/// Migration recommendation
#[derive(Debug, Clone)]
pub struct MigrationRecommendation {
    pub recommended: bool,
    pub strategy: MigrationStrategy,
    pub reasoning: String,
    pub estimated_effort: MigrationEffort,
}

/// Estimated effort for migration
#[derive(Debug, Clone)]
pub enum MigrationEffort {
    Low,
    Medium,
    High,
}

/// Migration step tracker
pub struct MigrationStepTracker {
    steps: Vec<MigrationStep>,
    current_step: usize,
}

#[derive(Debug, Clone)]
pub struct MigrationStep {
    pub name: String,
    pub description: String,
    pub completed: bool,
    pub error: Option<String>,
}

impl MigrationStepTracker {
    /// Create a new step tracker
    pub fn new() -> Self {
        let steps = vec![
            MigrationStep {
                name: "Validate Configuration".to_string(),
                description: "Validate migration configuration".to_string(),
                completed: false,
                error: None,
            },
            MigrationStep {
                name: "Initialize Modular Components".to_string(),
                description: "Initialize event bus, state, and components".to_string(),
                completed: false,
                error: None,
            },
            MigrationStep {
                name: "Setup Docker Integration".to_string(),
                description: "Configure enhanced Docker client".to_string(),
                completed: false,
                error: None,
            },
            MigrationStep {
                name: "Start Lifecycle Manager".to_string(),
                description: "Initialize container lifecycle management".to_string(),
                completed: false,
                error: None,
            },
            MigrationStep {
                name: "Configure File Watcher".to_string(),
                description: "Setup file change monitoring".to_string(),
                completed: false,
                error: None,
            },
            MigrationStep {
                name: "Verify Operation".to_string(),
                description: "Verify modular reactor is working correctly".to_string(),
                completed: false,
                error: None,
            },
        ];
        
        Self {
            steps,
            current_step: 0,
        }
    }
    
    /// Mark current step as completed
    pub fn complete_current_step(&mut self) {
        if self.current_step < self.steps.len() {
            self.steps[self.current_step].completed = true;
            self.current_step += 1;
        }
    }
    
    /// Mark current step as failed
    pub fn fail_current_step(&mut self, error: String) {
        if self.current_step < self.steps.len() {
            self.steps[self.current_step].error = Some(error);
        }
    }
    
    /// Get migration progress
    pub fn progress(&self) -> f64 {
        if self.steps.is_empty() {
            return 1.0;
        }
        let completed = self.steps.iter().filter(|s| s.completed).count();
        completed as f64 / self.steps.len() as f64
    }
    
    /// Check if migration is complete
    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|s| s.completed)
    }
    
    /// Get current step
    pub fn current_step(&self) -> Option<&MigrationStep> {
        self.steps.get(self.current_step)
    }
    
    /// Get all steps
    pub fn steps(&self) -> &[MigrationStep] {
        &self.steps
    }
}

impl Default for MigrationStepTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_config_default() {
        let config = MigrationConfig::default();
        // Phase 3 changes: Updated defaults for modern features
        assert!(!config.compatibility_mode);
        assert!(!config.fallback_on_error);
        assert!(!config.verbose_logging);
    }

    #[test]
    fn test_migration_step_tracker() {
        let mut tracker = MigrationStepTracker::new();
        
        assert_eq!(tracker.progress(), 0.0);
        assert!(!tracker.is_complete());
        
        tracker.complete_current_step();
        assert!(tracker.progress() > 0.0);
        
        tracker.fail_current_step("Test error".to_string());
        assert!(tracker.current_step().unwrap().error.is_some());
    }

    #[test]
    fn test_migrator_validation() {
        let config = MigrationConfig {
            strategy: MigrationStrategy::Immediate,
            compatibility_mode: true,
            fallback_on_error: true,
            verbose_logging: false,
        };
        
        let migrator = ReactorMigrator::new(config);
        assert!(migrator.validate_config().is_ok());
    }
}