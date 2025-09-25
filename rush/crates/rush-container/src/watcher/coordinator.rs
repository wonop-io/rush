//! Coordinator for file watching and rebuild triggering
//!
//! This module coordinates between the file watcher and the reactor,
//! managing rebuilds based on file changes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rush_build::ComponentBuildSpec;
use tokio::sync::{broadcast, RwLock};

use crate::events::EventBus;
use crate::reactor::state::{ReactorPhase, SharedReactorState};
use crate::watcher::handler::{ChangeBatch, FileChangeHandler, HandlerConfig};

/// Configuration for the watcher coordinator
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Handler configuration
    pub handler_config: HandlerConfig,
    /// Whether to automatically trigger rebuilds
    pub auto_rebuild: bool,
    /// Minimum time between rebuilds
    pub rebuild_cooldown: Duration,
    /// Maximum pending changes before forcing a rebuild
    pub max_pending_changes: usize,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            handler_config: HandlerConfig::default(),
            auto_rebuild: true,
            rebuild_cooldown: Duration::from_secs(2),
            max_pending_changes: 50,
        }
    }
}

/// Result of waiting for changes
#[derive(Debug, Clone)]
pub enum WatchResult {
    /// Changes detected that require rebuild
    Rebuild(ChangeBatch),
    /// Changes detected but no rebuild needed
    NoRebuildNeeded,
    /// Shutdown requested
    Shutdown,
    /// Error occurred
    Error(String),
}

/// Coordinates file watching with the reactor
pub struct WatcherCoordinator {
    config: CoordinatorConfig,
    handler: Arc<FileChangeHandler>,
    _event_bus: EventBus,
    state: SharedReactorState,
    shutdown_receiver: broadcast::Receiver<()>,
    last_rebuild_time: Arc<RwLock<std::time::Instant>>,
    pending_changes: Arc<RwLock<Vec<ChangeBatch>>>,
    watcher: Option<RecommendedWatcher>,
    event_sender: Option<tokio::sync::mpsc::UnboundedSender<notify::Event>>,
    event_receiver: Option<tokio::sync::mpsc::UnboundedReceiver<notify::Event>>,
}

impl WatcherCoordinator {
    /// Create a new watcher coordinator
    pub fn new(
        config: CoordinatorConfig,
        event_bus: EventBus,
        state: SharedReactorState,
        shutdown_sender: broadcast::Sender<()>,
    ) -> Self {
        Self::new_with_base_dir(config, event_bus, state, shutdown_sender, None)
    }

    /// Create a new watcher coordinator with base directory
    pub fn new_with_base_dir(
        config: CoordinatorConfig,
        event_bus: EventBus,
        state: SharedReactorState,
        shutdown_sender: broadcast::Sender<()>,
        base_dir: Option<PathBuf>,
    ) -> Self {
        let mut handler_builder =
            FileChangeHandler::new(config.handler_config.clone()).with_event_bus(event_bus.clone());

        // Set base directory if provided
        if let Some(base_dir) = &base_dir {
            handler_builder = handler_builder.with_base_dir(base_dir.clone());
        }

        let handler = Arc::new(handler_builder);
        let shutdown_receiver = shutdown_sender.subscribe();
        let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();

        Self {
            config,
            handler,
            _event_bus: event_bus,
            state,
            shutdown_receiver,
            // Initialize to a time in the past to allow immediate first rebuild
            last_rebuild_time: Arc::new(RwLock::new(
                std::time::Instant::now() - Duration::from_secs(60),
            )),
            pending_changes: Arc::new(RwLock::new(Vec::new())),
            watcher: None,
            event_sender: Some(event_sender),
            event_receiver: Some(event_receiver),
        }
    }

    /// Initialize the coordinator with component specs
    pub async fn init(&mut self, component_specs: Vec<ComponentBuildSpec>) {
        self.handler.set_component_specs(component_specs).await;
    }

    /// Start watching a directory
    pub fn watch_directory(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting file watcher for: {}", path.display());

        let sender = self
            .event_sender
            .as_ref()
            .ok_or("Event sender not available")?
            .clone();

        // Create the watcher - send events through channel instead of spawning directly
        let mut watcher =
            notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
                match event {
                    Ok(event) => {
                        // Send event through channel to be processed in Tokio context
                        if let Err(e) = sender.send(event) {
                            error!("Failed to send file event: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("File watcher error: {}", e);
                    }
                }
            })?;

        // Watch the directory recursively
        watcher.watch(path, RecursiveMode::Recursive)?;

        // Store the watcher
        self.watcher = Some(watcher);

        // Start the background processor
        let handler = self.handler.clone();
        handler.start_background_processor();

        info!("File watcher started successfully");
        Ok(())
    }

    /// Wait for changes that require action
    pub async fn wait_for_changes(&mut self) -> WatchResult {
        loop {
            tokio::select! {
                // Check for shutdown
                _ = self.shutdown_receiver.recv() => {
                    info!("Shutdown requested during file watch");
                    return WatchResult::Shutdown;
                }

                // Process events from the watcher channel
                Some(event) = async {
                    match &mut self.event_receiver {
                        Some(receiver) => receiver.recv().await,
                        None => None,
                    }
                } => {
                    // Handle the event in the Tokio context
                    self.handler.handle_event(event).await;
                }

                // Wait for file changes
                batch = self.handler.wait_for_changes() => {
                    if let Some(batch) = batch {
                        // Check if we should rebuild
                        if self.should_rebuild(&batch).await {
                            return WatchResult::Rebuild(batch);
                        } else {
                            // Store the batch but don't rebuild yet
                            let mut pending = self.pending_changes.write().await;
                            pending.push(batch);

                            // Check if we have too many pending changes
                            let total_changes: usize = pending.iter().map(|b| b.len()).sum();
                            if total_changes >= self.config.max_pending_changes {
                                // Merge all pending changes and rebuild
                                let mut merged = ChangeBatch::new();
                                for batch in pending.drain(..) {
                                    merged.merge(batch);
                                }
                                return WatchResult::Rebuild(merged);
                            }
                        }
                    }
                }

                // Periodic check for pending changes
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    let mut pending = self.pending_changes.write().await;
                    if !pending.is_empty() {
                        // Check if cooldown has passed
                        let last_rebuild = *self.last_rebuild_time.read().await;
                        if last_rebuild.elapsed() >= self.config.rebuild_cooldown {
                            // Merge and rebuild
                            let mut merged = ChangeBatch::new();
                            for batch in pending.drain(..) {
                                merged.merge(batch);
                            }
                            if !merged.is_empty() {
                                return WatchResult::Rebuild(merged);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Check if a rebuild should be triggered
    async fn should_rebuild(&self, batch: &ChangeBatch) -> bool {
        if !self.config.auto_rebuild {
            return false;
        }

        // Check reactor state
        let state = self.state.read().await;
        let phase = state.phase();

        // Only rebuild in certain phases
        match phase {
            ReactorPhase::Running | ReactorPhase::Idle => {
                // Check cooldown
                let last_rebuild = *self.last_rebuild_time.read().await;
                if last_rebuild.elapsed() < self.config.rebuild_cooldown {
                    debug!("Rebuild cooldown not met, deferring rebuild");
                    return false;
                }

                // Check if any components are affected
                if batch.affected_components.is_empty() {
                    debug!("No components affected by changes");
                    return false;
                }

                true
            }
            _ => {
                debug!("Reactor not in a state to rebuild: {:?}", phase);
                false
            }
        }
    }

    /// Mark that a rebuild has started
    pub async fn mark_rebuild_started(&self) {
        let mut last_rebuild = self.last_rebuild_time.write().await;
        *last_rebuild = std::time::Instant::now();

        // Clear pending changes
        let mut pending = self.pending_changes.write().await;
        pending.clear();
    }

    /// Stop watching
    pub fn stop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
            info!("File watcher stopped");
        }
    }
}

/// Builder for creating a watcher coordinator
pub struct CoordinatorBuilder {
    config: CoordinatorConfig,
    event_bus: Option<EventBus>,
    state: Option<SharedReactorState>,
    shutdown_sender: Option<broadcast::Sender<()>>,
    base_dir: Option<PathBuf>,
}

impl Default for CoordinatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CoordinatorBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: CoordinatorConfig::default(),
            event_bus: None,
            state: None,
            shutdown_sender: None,
            base_dir: None,
        }
    }

    /// Set the configuration
    pub fn with_config(mut self, config: CoordinatorConfig) -> Self {
        self.config = config;
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

    /// Set the shutdown sender
    pub fn with_shutdown_sender(mut self, sender: broadcast::Sender<()>) -> Self {
        self.shutdown_sender = Some(sender);
        self
    }

    /// Set the base directory
    pub fn with_base_dir(mut self, base_dir: PathBuf) -> Self {
        self.base_dir = Some(base_dir);
        self
    }

    /// Build the coordinator
    pub fn build(self) -> Result<WatcherCoordinator, String> {
        let event_bus = self.event_bus.ok_or("Event bus not set")?;
        let state = self.state.ok_or("Reactor state not set")?;
        let shutdown_sender = self.shutdown_sender.ok_or("Shutdown sender not set")?;

        Ok(WatcherCoordinator::new_with_base_dir(
            self.config,
            event_bus,
            state,
            shutdown_sender,
            self.base_dir,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventBus;
    use crate::reactor::state::SharedReactorState;

    #[test]
    fn test_coordinator_config_default() {
        let config = CoordinatorConfig::default();
        assert!(config.auto_rebuild);
        assert_eq!(config.rebuild_cooldown, Duration::from_secs(2));
        assert_eq!(config.max_pending_changes, 50);
    }

    #[tokio::test]
    async fn test_coordinator_builder() {
        let event_bus = EventBus::new();
        let state = SharedReactorState::new();
        let (shutdown_tx, _) = broadcast::channel(1);

        let result = CoordinatorBuilder::new()
            .with_event_bus(event_bus)
            .with_state(state)
            .with_shutdown_sender(shutdown_tx)
            .build();

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_should_rebuild_cooldown() {
        let event_bus = EventBus::new();
        let state = SharedReactorState::new();
        let (shutdown_tx, _) = broadcast::channel(1);

        // Set state to idle (which allows rebuilds)
        {
            let mut state_guard = state.write().await;
            // Transition through valid states to reach Idle
            // The state starts at Idle by default, so no transition needed
            assert_eq!(state_guard.phase(), &ReactorPhase::Idle);
        }

        let coordinator = WatcherCoordinator::new(
            CoordinatorConfig {
                rebuild_cooldown: Duration::from_millis(100),
                ..Default::default()
            },
            event_bus,
            state,
            shutdown_tx,
        );

        let mut batch = ChangeBatch::new();
        batch.affected_components.insert("test".to_string());

        // First check should succeed
        assert!(coordinator.should_rebuild(&batch).await);

        // Mark rebuild started
        coordinator.mark_rebuild_started().await;

        // Immediate check should fail due to cooldown
        assert!(!coordinator.should_rebuild(&batch).await);

        // Wait for cooldown
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should succeed after cooldown
        assert!(coordinator.should_rebuild(&batch).await);
    }
}
