//! Integration between the reactor and the improved file watcher
//!
//! This module provides integration points for using the new watcher
//! coordinator with the reactor.

use crate::{
    events::EventBus,
    reactor::state::SharedReactorState,
    watcher::{CoordinatorBuilder, CoordinatorConfig, WatcherCoordinator, WatchResult, ChangeBatch},
};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;
use log::{debug, error, info, warn};

/// Integration configuration
#[derive(Debug, Clone)]
pub struct WatcherIntegrationConfig {
    /// Whether to use the new watcher system
    pub use_new_watcher: bool,
    /// Coordinator configuration
    pub coordinator_config: CoordinatorConfig,
}

impl Default for WatcherIntegrationConfig {
    fn default() -> Self {
        Self {
            use_new_watcher: true,
            coordinator_config: CoordinatorConfig::default(),
        }
    }
}

/// Integrates the new watcher with the reactor
pub struct WatcherIntegration {
    config: WatcherIntegrationConfig,
    coordinator: Option<WatcherCoordinator>,
    event_bus: EventBus,
    state: SharedReactorState,
}

impl WatcherIntegration {
    /// Create a new watcher integration
    pub fn new(
        config: WatcherIntegrationConfig,
        event_bus: EventBus,
        state: SharedReactorState,
        shutdown_sender: broadcast::Sender<()>,
    ) -> Result<Self, String> {
        let coordinator = if config.use_new_watcher {
            Some(
                CoordinatorBuilder::new()
                    .with_config(config.coordinator_config.clone())
                    .with_event_bus(event_bus.clone())
                    .with_state(state.clone())
                    .with_shutdown_sender(shutdown_sender)
                    .build()?
            )
        } else {
            None
        };
        
        Ok(Self {
            config,
            coordinator,
            event_bus,
            state,
        })
    }

    /// Initialize with component specs
    pub async fn init(&mut self, specs: Vec<rush_build::ComponentBuildSpec>) {
        if let Some(coordinator) = &mut self.coordinator {
            coordinator.init(specs).await;
        }
    }

    /// Start watching a directory
    pub fn start_watching(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(coordinator) = &mut self.coordinator {
            info!("Starting new watcher system for: {}", path.display());
            coordinator.watch_directory(path)?;
        } else {
            info!("New watcher system disabled, using legacy watcher");
        }
        Ok(())
    }

    /// Wait for changes that require rebuild
    pub async fn wait_for_rebuild(&mut self) -> Option<Vec<String>> {
        if let Some(coordinator) = &mut self.coordinator {
            match coordinator.wait_for_changes().await {
                WatchResult::Rebuild(batch) => {
                    info!(
                        "File changes detected: {} files changed, {} components affected",
                        batch.len(),
                        batch.affected_components.len()
                    );
                    
                    // Mark rebuild started
                    coordinator.mark_rebuild_started().await;
                    
                    // Return affected components
                    Some(batch.affected_components.into_iter().collect())
                }
                WatchResult::NoRebuildNeeded => {
                    debug!("File changes detected but no rebuild needed");
                    None
                }
                WatchResult::Shutdown => {
                    info!("Shutdown requested during file watch");
                    None
                }
                WatchResult::Error(e) => {
                    error!("Watcher error: {}", e);
                    None
                }
            }
        } else {
            // Legacy mode - return empty to indicate all components should rebuild
            None
        }
    }

    /// Stop watching
    pub fn stop(&mut self) {
        if let Some(coordinator) = &mut self.coordinator {
            coordinator.stop();
        }
    }

    /// Check if using new watcher
    pub fn is_using_new_watcher(&self) -> bool {
        self.coordinator.is_some()
    }
}

/// Helper to determine which components need rebuilding
pub fn determine_rebuild_targets(
    changed_batch: Option<ChangeBatch>,
    all_components: &[String],
) -> Vec<String> {
    if let Some(batch) = changed_batch {
        // Only rebuild affected components
        batch.affected_components.into_iter().collect()
    } else {
        // Rebuild all components (legacy behavior)
        all_components.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventBus;
    use crate::reactor::state::SharedReactorState;

    #[test]
    fn test_integration_config_default() {
        let config = WatcherIntegrationConfig::default();
        assert!(config.use_new_watcher);
    }

    #[tokio::test]
    async fn test_integration_creation() {
        let event_bus = EventBus::new();
        let state = SharedReactorState::new();
        let (shutdown_tx, _) = broadcast::channel(1);
        
        let integration = WatcherIntegration::new(
            WatcherIntegrationConfig::default(),
            event_bus,
            state,
            shutdown_tx,
        );
        
        assert!(integration.is_ok());
        assert!(integration.unwrap().is_using_new_watcher());
    }

    #[test]
    fn test_determine_rebuild_targets() {
        let all_components = vec!["comp1".to_string(), "comp2".to_string(), "comp3".to_string()];
        
        // Test with specific affected components
        let mut batch = ChangeBatch::new();
        batch.affected_components.insert("comp1".to_string());
        batch.affected_components.insert("comp3".to_string());
        
        let targets = determine_rebuild_targets(Some(batch), &all_components);
        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&"comp1".to_string()));
        assert!(targets.contains(&"comp3".to_string()));
        
        // Test with no batch (legacy mode)
        let targets = determine_rebuild_targets(None, &all_components);
        assert_eq!(targets, all_components);
    }
}