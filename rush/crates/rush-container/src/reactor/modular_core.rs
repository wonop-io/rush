//! Modular container reactor implementation
//!
//! This module provides the new modular reactor that integrates all the 
//! extracted components from previous phases.

use crate::{
    docker::{DockerClient, DockerService},
    events::{EventBus, Event, ContainerEvent},
    lifecycle::{LifecycleManager, LifecycleConfig},
    build::{BuildOrchestrator, BuildOrchestratorConfig},
    watcher::{WatcherCoordinator, CoordinatorConfig, WatchResult},
    reactor::{
        config::ContainerReactorConfig,
        docker_integration::{DockerIntegration, DockerIntegrationConfig, DockerIntegrationBuilder},
        state::{SharedReactorState, ReactorState, ReactorPhase},
        errors::ReactorError,
    },
};
use rush_build::ComponentBuildSpec;
use rush_core::error::{Error, Result};
use rush_core::shutdown;
use rush_config::Config;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use log::{info, debug, error, warn};

/// Configuration for the modular reactor
#[derive(Debug, Clone)]
pub struct ModularReactorConfig {
    /// Base reactor configuration
    pub base: ContainerReactorConfig,
    /// Lifecycle management configuration
    pub lifecycle: LifecycleConfig,
    /// Build orchestration configuration
    pub build: BuildOrchestratorConfig,
    /// File watcher configuration
    pub watcher: CoordinatorConfig,
    /// Docker integration configuration
    pub docker: DockerIntegrationConfig,
    /// Whether to use legacy reactor implementation
    pub use_legacy: bool,
}

impl Default for ModularReactorConfig {
    fn default() -> Self {
        Self {
            base: ContainerReactorConfig::default(),
            lifecycle: LifecycleConfig::default(),
            build: BuildOrchestratorConfig::default(),
            watcher: CoordinatorConfig::default(),
            docker: DockerIntegrationConfig::default(),
            use_legacy: false,
        }
    }
}

/// Modular container reactor that uses all extracted components
pub struct ModularReactor {
    /// Reactor configuration
    config: ModularReactorConfig,
    /// Event bus for component communication
    event_bus: EventBus,
    /// Shared reactor state
    state: SharedReactorState,
    /// Lifecycle manager for container operations
    lifecycle_manager: LifecycleManager,
    /// Build orchestrator for builds
    build_orchestrator: Arc<BuildOrchestrator>,
    /// File watcher coordinator
    watcher_coordinator: Option<WatcherCoordinator>,
    /// Enhanced Docker integration
    docker_integration: DockerIntegration,
    /// Shutdown coordination
    shutdown_sender: broadcast::Sender<()>,
    shutdown_receiver: broadcast::Receiver<()>,
    /// Services to run (set after build)
    services: Vec<crate::ContainerService>,
    /// Component specs for building
    component_specs: Vec<ComponentBuildSpec>,
    /// Built images mapping
    built_images: std::collections::HashMap<String, String>,
    /// Output sink for container and build logs
    output_sink: Arc<tokio::sync::Mutex<Box<dyn rush_output::simple::Sink>>>,
}

impl ModularReactor {
    /// Create a new modular reactor
    pub async fn new(
        config: ModularReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<Self> {
        info!("Initializing modular container reactor");
        
        // Create event bus for component communication
        let event_bus = EventBus::new();
        
        // Create shared state
        let state = SharedReactorState::new();
        
        // Set up shutdown coordination
        let (shutdown_sender, shutdown_receiver) = broadcast::channel(1);
        
        // Create Docker integration with all enhancements
        let docker_integration = DockerIntegrationBuilder::new()
            .with_config(config.docker.clone())
            .with_client(docker_client.clone())
            .with_event_bus(event_bus.clone())
            .with_state(state.clone())
            .build()?;
        
        // Create lifecycle manager with a mock vault for now
        let vault: Arc<std::sync::Mutex<dyn rush_security::Vault + Send>> = 
            Arc::new(std::sync::Mutex::new(rush_security::FileVault::new(
                std::path::PathBuf::from(".rush/vault"),
                None
            )));
        
        let lifecycle_manager = LifecycleManager::new(
            config.lifecycle.clone(),
            docker_integration.client(),
            vault,
            event_bus.clone(),
            state.clone(),
        );
        
        // Create build orchestrator
        let build_orchestrator = Arc::new(
            BuildOrchestrator::new(
                config.build.clone(),
                docker_integration.client(),
                event_bus.clone(),
                state.clone(),
            )
        );
        
        // Create file watcher coordinator (optional)
        let watcher_coordinator = if config.watcher.handler_config.ignore_patterns.is_empty() {
            None
        } else {
            let mut coordinator = crate::watcher::CoordinatorBuilder::new()
                .with_config(config.watcher.clone())
                .with_event_bus(event_bus.clone())
                .with_state(state.clone())
                .with_shutdown_sender(shutdown_sender.clone())
                .build()
                .map_err(|e| Error::Internal(format!("Failed to create watcher coordinator: {}", e)))?;
            
            coordinator.init(component_specs.clone()).await;
            Some(coordinator)
        };
        
        let mut reactor = Self {
            config,
            event_bus,
            state,
            lifecycle_manager,
            build_orchestrator,
            watcher_coordinator,
            docker_integration,
            shutdown_sender,
            shutdown_receiver,
            services: Vec::new(),
            component_specs: component_specs.clone(),
            built_images: std::collections::HashMap::new(),
            output_sink: Arc::new(tokio::sync::Mutex::new(
                Box::new(rush_output::simple::StdoutSink::new()),
            )),
        };
        
        // Initialize state with component specs
        reactor.initialize_state(component_specs).await?;
        
        info!("Modular reactor initialized successfully");
        Ok(reactor)
    }
    
    /// Initialize reactor state with component specifications
    async fn initialize_state(&mut self, component_specs: Vec<ComponentBuildSpec>) -> Result<()> {
        let mut state = self.state.write().await;
        
        // Set component specifications
        for spec in component_specs {
            let mut component_state = crate::reactor::state::ComponentState::new(spec.component_name.clone());
            component_state.build_spec = Some(spec);
            state.add_component(component_state);
        }
        
        // The state already starts in Idle, no need to transition
        
        Ok(())
    }
    
    /// Start the reactor and begin processing
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting modular container reactor");
        
        // Transition to starting phase if not already there
        {
            let mut state = self.state.write().await;
            if state.phase() != &ReactorPhase::Starting {
                state.transition_to(ReactorPhase::Starting)?;
            }
        }
        
        // Start Docker health monitoring
        self.docker_integration.health_check().await?;
        
        // Start lifecycle manager
        self.lifecycle_manager.start().await?;
        
        // Start file watching if configured
        if let Some(watcher) = &mut self.watcher_coordinator {
            let watch_path = std::env::current_dir()?;
            watcher.watch_directory(&watch_path)
                .map_err(|e| Error::Internal(format!("Failed to start file watcher: {}", e)))?;
        }
        
        // Transition to running phase
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::Running)?;
        }
        
        // Publish startup event
        let _ = self.event_bus.publish(Event::new(
            "reactor",
            ContainerEvent::ReactorStarted,
        )).await;
        
        info!("Modular reactor started successfully");
        Ok(())
    }
    
    /// Main processing loop
    pub async fn run(&mut self) -> Result<()> {
        info!("Modular reactor entering main processing loop");
        
        // Get the global shutdown token for Ctrl-C handling
        let shutdown_token = shutdown::global_shutdown().cancellation_token();
        
        loop {
            tokio::select! {
                // Handle Ctrl-C and other termination signals
                _ = shutdown_token.cancelled() => {
                    info!("Termination signal received (Ctrl-C)");
                    break;
                }
                
                // Handle shutdown signal from internal broadcasts
                _ = self.shutdown_receiver.recv() => {
                    info!("Internal shutdown signal received");
                    break;
                }
                
                // Handle file changes if watcher is configured
                watch_result = async {
                    match &mut self.watcher_coordinator {
                        Some(watcher) => watcher.wait_for_changes().await,
                        None => {
                            // Wait indefinitely if no watcher
                            tokio::time::sleep(Duration::from_secs(3600)).await;
                            WatchResult::NoRebuildNeeded
                        }
                    }
                } => {
                    match watch_result {
                        WatchResult::Rebuild(batch) => {
                            info!("File changes detected, triggering rebuild for {} components", 
                                batch.affected_components.len());
                            
                            if let Err(e) = self.handle_rebuild(batch.affected_components).await {
                                error!("Rebuild failed: {}", e);
                                // Don't break the loop, continue processing
                            }
                        }
                        WatchResult::Shutdown => {
                            info!("Watcher shutdown requested");
                            break;
                        }
                        WatchResult::NoRebuildNeeded => {
                            debug!("File changes detected but no rebuild needed");
                        }
                        WatchResult::Error(e) => {
                            error!("Watcher error: {}", e);
                            // Continue processing despite error
                        }
                    }
                }
            }
        }
        
        info!("Modular reactor exiting main loop, initiating shutdown");
        
        // Perform cleanup
        self.shutdown().await?;
        
        Ok(())
    }
    
    /// Handle rebuild request for specific components
    async fn handle_rebuild(&mut self, components: std::collections::HashSet<String>) -> Result<()> {
        debug!("Handling rebuild for components: {:?}", components);
        
        // Transition to rebuilding phase
        {
            let mut state = self.state.write().await;
            // We should be in Running state to start a rebuild
            if state.phase() == &ReactorPhase::Running {
                state.transition_to(ReactorPhase::Rebuilding)?;
            } else if state.phase() != &ReactorPhase::Rebuilding {
                // If not running and not already rebuilding, something is wrong
                warn!("Unexpected state for rebuild: {:?}", state.phase());
                return Ok(()); // Skip rebuild
            }
        }
        
        // Mark rebuild started in watcher
        if let Some(watcher) = &self.watcher_coordinator {
            watcher.mark_rebuild_started().await;
        }
        
        // Stop affected containers before rebuilding
        for component_name in &components {
            if let Err(e) = self.lifecycle_manager.stop_component(component_name).await {
                warn!("Failed to stop component {}: {}", component_name, e);
            }
        }
        
        // Get component specs for affected components
        let component_specs = {
            let state = self.state.read().await;
            components.iter()
                .filter_map(|name| state.get_component(name))
                .filter_map(|comp| comp.build_spec.as_ref().cloned())
                .collect::<Vec<_>>()
        };
        
        let build_result = self.build_orchestrator.build_components(component_specs, false).await;
        
        match build_result {
            Ok(successful_builds) => {
                info!("Build completed successfully for {} components", successful_builds.len());
                
                // Start the rebuilt components
                for component_name in successful_builds.keys() {
                    if let Err(e) = self.lifecycle_manager.start_component(component_name).await {
                        error!("Failed to start component {}: {}", component_name, e);
                    }
                }
                
                // Transition back to running from rebuilding
                {
                    let mut state = self.state.write().await;
                    if state.phase() == &ReactorPhase::Rebuilding {
                        state.transition_to(ReactorPhase::Running)?;
                    }
                }
                
                // Publish build success event
                let _ = self.event_bus.publish(Event::new(
                    "reactor",
                    ContainerEvent::BuildCompleted {
                        component: format!("{} components", successful_builds.len()),
                        success: true,
                        duration: Duration::from_secs(0), // TODO: track actual duration
                        error: None,
                    },
                )).await;
                
                Ok(())
            }
            Err(e) => {
                error!("Build failed: {}", e);
                
                // Record error but stay in current state
                {
                    let mut state = self.state.write().await;
                    state.record_error(format!("Build failed: {}", e));
                    // Try to transition back to running if we were rebuilding
                    if state.phase() == &ReactorPhase::Rebuilding {
                        state.transition_to(ReactorPhase::Running)?;
                    }
                }
                
                // Publish build failure event
                let _ = self.event_bus.publish(Event::new(
                    "reactor",
                    ContainerEvent::BuildCompleted {
                        component: "multiple".to_string(),
                        success: false,
                        duration: Duration::from_secs(0),
                        error: Some(e.to_string()),
                    },
                )).await;
                
                Err(e)
            }
        }
    }
    
    /// Trigger a manual rebuild of all components
    pub async fn rebuild_all(&mut self) -> Result<()> {
        info!("Manual rebuild of all components requested");
        
        let component_names: std::collections::HashSet<String> = {
            let state = self.state.read().await;
            let names = state.components().keys().cloned().collect();
            info!("Found {} components in state", state.components().len());
            names
        };
        
        if component_names.is_empty() {
            warn!("No components found in state - nothing to build");
            return Ok(());
        }
        
        // Check if this is an initial build (Idle state) or a rebuild (Running state)
        let current_phase = {
            let state = self.state.read().await;
            let phase = state.phase().clone();
            info!("Current reactor phase: {:?}", phase);
            phase
        };
        
        match current_phase {
            ReactorPhase::Idle => {
                // Initial build
                self.initial_build(component_names).await
            }
            ReactorPhase::Running => {
                // Normal rebuild
                self.handle_rebuild(component_names).await
            }
            _ => {
                warn!("Cannot rebuild in current phase: {:?}", current_phase);
                Ok(())
            }
        }
    }
    
    /// Handle initial build of all components (from Idle state)
    async fn initial_build(&mut self, components: std::collections::HashSet<String>) -> Result<()> {
        info!("Performing initial build for {} components", components.len());
        
        // Transition from Idle to Building
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::Building)?;
        }
        
        // Get component specs
        let component_specs = {
            let state = self.state.read().await;
            components.iter()
                .filter_map(|name| state.get_component(name))
                .filter_map(|comp| comp.build_spec.as_ref().cloned())
                .collect::<Vec<_>>()
        };
        
        let build_result = self.build_orchestrator.build_components(component_specs, false).await;
        
        match build_result {
            Ok(successful_builds) => {
                info!("Initial build completed successfully for {} components", successful_builds.len());
                
                // Store built images
                self.built_images = successful_builds.clone();
                
                // Create services from built images
                self.create_services_from_specs()?;
                
                // Start the services using lifecycle manager
                let running_services = self.lifecycle_manager.start_services(
                    self.services.clone(),
                    &self.component_specs,
                    &self.built_images,
                ).await?;
                
                info!("Started {} services", running_services.len());
                
                // Transition to Starting (not directly to Running)
                {
                    let mut state = self.state.write().await;
                    state.transition_to(ReactorPhase::Starting)?;
                }
                
                // Publish build success event
                let _ = self.event_bus.publish(Event::new(
                    "reactor",
                    ContainerEvent::BuildCompleted {
                        component: format!("{} components", successful_builds.len()),
                        success: true,
                        duration: Duration::from_secs(0),
                        error: None,
                    },
                )).await;
                
                Ok(())
            }
            Err(e) => {
                error!("Initial build failed: {}", e);
                
                // Record error and transition to Error state
                {
                    let mut state = self.state.write().await;
                    state.record_error(format!("Initial build failed: {}", e));
                    state.transition_to(ReactorPhase::Error)?;
                }
                
                // Publish build failure event
                let _ = self.event_bus.publish(Event::new(
                    "reactor",
                    ContainerEvent::BuildCompleted {
                        component: "multiple".to_string(),
                        success: false,
                        duration: Duration::from_secs(0),
                        error: Some(e.to_string()),
                    },
                )).await;
                
                Err(e)
            }
        }
    }
    
    /// Get current reactor status
    pub async fn status(&self) -> ReactorStatus {
        let state = self.state.read().await;
        let docker_stats = self.docker_integration.get_docker_stats().await;
        let metrics_report = self.docker_integration.get_metrics_report().await;
        
        ReactorStatus {
            phase: state.phase().clone(),
            components: state.components().len(),
            running_containers: state.running_components().len(),
            last_error: state.last_error().cloned(),
            docker_healthy: self.docker_integration.health_check().await.is_ok(),
            docker_stats,
            metrics_report,
        }
    }
    
    /// Initiate graceful shutdown
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Initiating graceful reactor shutdown");
        
        // Transition to shutting down phase
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::ShuttingDown)?;
        }
        
        // Stop all components
        let component_names: Vec<String> = {
            let state = self.state.read().await;
            state.components().keys().cloned().collect()
        };
        
        for component_name in component_names {
            if let Err(e) = self.lifecycle_manager.stop_component(&component_name).await {
                warn!("Failed to stop component {} during shutdown: {}", component_name, e);
            }
        }
        
        // Stop watcher
        if let Some(watcher) = &mut self.watcher_coordinator {
            watcher.stop();
        }
        
        // Stop lifecycle manager
        self.lifecycle_manager.stop().await;
        
        // Shutdown Docker integration
        self.docker_integration.shutdown().await;
        
        // Send shutdown signal
        let _ = self.shutdown_sender.send(());
        
        // Transition to shutdown phase
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::Terminated)?;
        }
        
        info!("Reactor shutdown complete");
        Ok(())
    }
    
    /// Get event bus for external subscribers
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }
    
    /// Get shared state for external access
    pub fn state(&self) -> &SharedReactorState {
        &self.state
    }
    
    /// Create services from component specs and built images
    fn create_services_from_specs(&mut self) -> Result<()> {
        self.services.clear();
        
        for spec in &self.component_specs {
            // Skip non-container components
            if !spec.build_type.requires_docker_build() {
                continue;
            }
            
            // Skip ingress and other special components that don't have images
            if matches!(spec.component_name.as_str(), "ingress" | "database" | "stripe") {
                continue;
            }
            
            // Get the built image name or skip if not built
            let image = match self.built_images.get(&spec.component_name) {
                Some(img) => img.clone(),
                None => {
                    debug!("Skipping {} - no built image available", spec.component_name);
                    continue;
                }
            };
            
            // Create a container service with proper ports
            // Use different default ports for different components
            let (default_port, default_target) = match spec.component_name.as_str() {
                "frontend" => (9000, 80),
                "backend" => (8000, 8000),
                "ingress" => (8080, 80),
                _ => (3000, 3000),
            };
            
            let service = crate::ContainerService {
                id: uuid::Uuid::new_v4().to_string(),
                name: spec.component_name.clone(),
                image,
                host: spec.component_name.clone(),
                port: spec.port.unwrap_or(default_port),
                target_port: spec.target_port.unwrap_or(default_target),
                mount_point: spec.mount_point.clone(),
                domain: spec.domain.clone(),
                docker_host: format!("{}.docker", spec.component_name),
            };
            
            self.services.push(service);
        }
        
        Ok(())
    }
    
    /// Set services (for external configuration)
    pub fn set_services(&mut self, services: Vec<crate::ContainerService>) {
        self.services = services;
    }
    
    /// Set output sink for capturing container and build logs
    pub async fn set_output_sink(&mut self, sink: Arc<tokio::sync::Mutex<Box<dyn rush_output::simple::Sink>>>) {
        self.output_sink = sink.clone();
        
        // Propagate to build orchestrator
        self.build_orchestrator.set_output_sink(sink.clone()).await;
        
        // Propagate to lifecycle manager
        self.lifecycle_manager.set_output_sink(sink).await;
    }
}

/// Status information about the reactor
#[derive(Debug, Clone)]
pub struct ReactorStatus {
    pub phase: ReactorPhase,
    pub components: usize,
    pub running_containers: usize,
    pub last_error: Option<String>,
    pub docker_healthy: bool,
    pub docker_stats: Option<crate::docker::DockerStats>,
    pub metrics_report: Option<crate::docker::MetricsReport>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_modular_reactor_config_default() {
        let config = ModularReactorConfig::default();
        assert!(!config.use_legacy);
        assert!(config.docker.use_enhanced_client);
        assert!(config.lifecycle.auto_restart);
    }

    #[tokio::test]
    async fn test_reactor_status() {
        // This would need mock implementations to test properly
        // For now, just ensure the type compiles
        let status = ReactorStatus {
            phase: ReactorPhase::Idle,
            components: 0,
            running_containers: 0,
            last_error: None,
            docker_healthy: true,
            docker_stats: None,
            metrics_report: None,
        };
        
        assert_eq!(status.phase, ReactorPhase::Idle);
        assert_eq!(status.components, 0);
    }
}