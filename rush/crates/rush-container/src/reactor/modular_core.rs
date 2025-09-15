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
use rush_build::{ComponentBuildSpec, BuildType};
use rush_core::error::{Error, Result};
use rush_core::shutdown;
use rush_config::Config;
use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;
use tokio::sync::broadcast;
use log::{info, debug, error, warn};
use tera;

/// Docker registry configuration
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Registry URL (e.g., "docker.io", "gcr.io", custom registry)
    pub url: Option<String>,
    /// Registry namespace/organization
    pub namespace: Option<String>,
    /// Registry username (for authentication)
    pub username: Option<String>,
    /// Registry password (from environment or secrets)
    pub password: Option<String>,
    /// Whether to use Docker credentials helper
    pub use_credentials_helper: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: None,
            namespace: None,
            username: None,
            password: None,
            use_credentials_helper: true,
        }
    }
}

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
    /// Docker registry configuration
    pub registry: RegistryConfig,
}

impl Default for ModularReactorConfig {
    fn default() -> Self {
        Self {
            base: ContainerReactorConfig::default(),
            lifecycle: LifecycleConfig::default(),
            build: BuildOrchestratorConfig::default(),
            watcher: CoordinatorConfig::default(),
            docker: DockerIntegrationConfig::default(),
            registry: RegistryConfig::default(),
        }
    }
}

/// Primary container reactor that manages container lifecycle and coordinates rebuilds
pub struct Reactor {
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
    /// Force rebuild flag
    force_rebuild: bool,
    /// Output sink for container and build logs
    output_sink: Arc<tokio::sync::Mutex<Box<dyn rush_output::simple::Sink>>>,
    /// Kubernetes manifest output directory
    k8s_manifest_dir: Option<std::path::PathBuf>,
    /// Vault for secrets management
    vault: Option<Arc<std::sync::Mutex<dyn rush_security::Vault + Send>>>,
    /// Secrets encoder for K8s
    secrets_encoder: Option<Arc<dyn rush_security::SecretsEncoder>>,
    /// Track deployment versions for rollback
    deployment_versions: Vec<rush_k8s::kubectl::DeploymentVersion>,
}

impl Reactor {
    /// Create a new modular reactor
    pub async fn new(
        config: ModularReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
    ) -> Result<Self> {
        // Use default toolchain
        let toolchain = Arc::new(rush_toolchain::ToolchainContext::default());
        Self::with_toolchain(config, docker_client, component_specs, toolchain).await
    }

    /// Create a new modular reactor with custom toolchain
    pub async fn with_toolchain(
        config: ModularReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        component_specs: Vec<ComponentBuildSpec>,
        toolchain: Arc<rush_toolchain::ToolchainContext>,
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

        // Create build orchestrator with the provided toolchain
        let build_orchestrator = Arc::new(
            BuildOrchestrator::with_toolchain(
                config.build.clone(),
                docker_integration.client(),
                event_bus.clone(),
                state.clone(),
                toolchain,
            )
        );
        
        // Create file watcher coordinator
        // Always create watcher for automatic rebuilds during development
        let watcher_coordinator = {
            let mut coordinator = crate::watcher::CoordinatorBuilder::new()
                .with_config(config.watcher.clone())
                .with_event_bus(event_bus.clone())
                .with_state(state.clone())
                .with_shutdown_sender(shutdown_sender.clone())
                .with_base_dir(config.base.product_dir.clone())
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
            force_rebuild: false,
            output_sink: Arc::new(tokio::sync::Mutex::new(
                Box::new(rush_output::simple::StdoutSink::new()),
            )),
            k8s_manifest_dir: None,
            vault: None,
            secrets_encoder: None,
            deployment_versions: Vec::new(),
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
        
        // Propagate output sink to build orchestrator and lifecycle manager
        self.build_orchestrator.set_output_sink(self.output_sink.clone()).await;
        self.lifecycle_manager.set_output_sink(self.output_sink.clone()).await;
        
        // Start Docker health monitoring
        self.docker_integration.health_check().await?;
        
        // Start lifecycle manager
        self.lifecycle_manager.start().await?;
        
        // Setup and start file watching with watch patterns
        if let Err(e) = self.setup_watchers().await {
            warn!("Failed to setup file watchers: {}", e);
            // Continue anyway - file watching is optional
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

                // Periodic tag-based rebuild check (every 30 seconds)
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    debug!("Performing periodic tag-based rebuild check");
                    if let Err(e) = self.trigger_tag_based_rebuild().await {
                        warn!("Periodic rebuild check failed: {}", e);
                    }
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
                            
                            if let Err(e) = self.handle_rebuild(batch).await {
                                error!("Rebuild failed: {}", e);
                                // Build failure is handled in handle_rebuild - containers are stopped
                                // Continue processing to maintain reactive behavior for future file changes
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
    async fn handle_rebuild(&mut self, batch: crate::watcher::handler::ChangeBatch) -> Result<()> {
        debug!("Handling rebuild for components: {:?}", batch.affected_components);
        
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
        
        // Invalidate cache based on changed files
        let all_changed_files: Vec<std::path::PathBuf> = batch.modified.iter()
            .chain(batch.created.iter())
            .chain(batch.deleted.iter())
            .cloned()
            .collect();
        
        if !all_changed_files.is_empty() {
            info!("Invalidating cache for {} changed files", all_changed_files.len());
            if let Err(e) = self.build_orchestrator.invalidate_cache_for_files(&all_changed_files).await {
                warn!("Failed to invalidate cache: {}", e);
                // Continue with rebuild even if cache invalidation fails
            }
        }
        
        // Stop affected containers before rebuilding
        for component_name in &batch.affected_components {
            if let Err(e) = self.lifecycle_manager.stop_component(component_name).await {
                warn!("Failed to stop component {}: {}", component_name, e);
            }
        }
        
        // Get component specs for affected components
        let component_specs = {
            let state = self.state.read().await;
            batch.affected_components.iter()
                .filter_map(|name| state.get_component(name))
                .filter_map(|comp| comp.build_spec.as_ref().cloned())
                .collect::<Vec<_>>()
        };
        
        if component_specs.is_empty() {
            warn!("No component specs found for rebuild, skipping build");
            // Still transition back to running state
            let mut state = self.state.write().await;
            if state.phase() == &ReactorPhase::Rebuilding {
                state.transition_to(ReactorPhase::Running)?;
            }
            return Ok(());
        }
        
        let build_result = self.build_orchestrator.build_components(component_specs, false).await;
        
        match build_result {
            Ok(successful_builds) => {
                info!("Build completed successfully for {} components", successful_builds.len());
                
                // Update built images
                for (name, image) in &successful_builds {
                    self.built_images.insert(name.clone(), image.clone());
                }
                
                // Recreate services from updated specs
                self.create_services_from_specs()?;
                
                // Start the rebuilt components using start_services
                // This will actually create the Docker containers
                let services_to_start: Vec<_> = self.services.iter()
                    .filter(|s| successful_builds.contains_key(&s.name))
                    .cloned()
                    .collect();
                
                if !services_to_start.is_empty() {
                    let running_services = self.lifecycle_manager.start_services(
                        services_to_start,
                        &self.component_specs,
                        &self.built_images,
                    ).await?;
                    
                    info!("Started {} rebuilt containers", running_services.len());
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
                
                // For development workflow: ensure no containers are running for failed components
                // This prevents confusing behavior where old containers might still be running
                for component_name in &batch.affected_components {
                    if let Err(e) = self.lifecycle_manager.stop_component(component_name).await {
                        warn!("Failed to ensure component {} is stopped after build failure: {}", component_name, e);
                    }
                }
                
                info!("Build failed for {} components, all containers stopped", batch.affected_components.len());
                
                Err(e)
            }
        }
    }
    
    /// Handle manual rebuild request (not triggered by file changes)
    async fn handle_manual_rebuild(&mut self, components: std::collections::HashSet<String>, force_rebuild: bool) -> Result<()> {
        debug!("Handling manual rebuild for components: {:?}, force: {}", components, force_rebuild);
        
        // Transition to rebuilding phase
        {
            let mut state = self.state.write().await;
            // We should be in Running state to start a rebuild
            if state.phase() == &ReactorPhase::Running {
                state.transition_to(ReactorPhase::Rebuilding)?;
            } else if state.phase() != &ReactorPhase::Rebuilding {
                // If not running and not already rebuilding, something is wrong
                warn!("Unexpected state for manual rebuild: {:?}", state.phase());
                return Ok(()); // Skip rebuild
            }
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
        
        // For manual rebuilds, use cache unless force_rebuild is true
        // This allows cache invalidation logic to work properly
        let build_result = self.build_orchestrator.build_components(component_specs, force_rebuild).await;
        
        match build_result {
            Ok(successful_builds) => {
                info!("Manual build completed successfully for {} components", successful_builds.len());
                
                // Update built images
                for (name, image) in &successful_builds {
                    self.built_images.insert(name.clone(), image.clone());
                }
                
                // Recreate services from updated specs
                self.create_services_from_specs()?;
                
                // Start the services
                self.lifecycle_manager.start_services(
                    self.services.clone(),
                    &self.component_specs,
                    &successful_builds,
                ).await?;
                
                info!("Started {} rebuilt containers", successful_builds.len());
                
                // Publish build success event
                let _ = self.event_bus.publish(Event::new(
                    "build",
                    ContainerEvent::BuildCompleted {
                        component: "all".to_string(),
                        success: true,
                        duration: std::time::Duration::from_secs(0), // TODO: Track actual duration
                        error: None,
                    },
                )).await;
                
                // Transition back to running phase
                {
                    let mut state = self.state.write().await;
                    state.transition_to(ReactorPhase::Running)?;
                }
                
                Ok(())
            }
            Err(e) => {
                error!("Manual build failed: {}", e);
                
                // Publish build failure event
                let _ = self.event_bus.publish(Event::new(
                    "build",
                    ContainerEvent::BuildCompleted {
                        component: "all".to_string(),
                        success: false,
                        duration: std::time::Duration::from_secs(0),
                        error: Some(e.to_string()),
                    },
                )).await;
                
                Err(e)
            }
        }
    }
    
    /// Trigger a manual rebuild of all components
    pub async fn rebuild_all(&mut self) -> Result<()> {
        self.rebuild_all_with_force(self.force_rebuild).await
    }
    
    /// Trigger a manual rebuild of all components with optional force
    pub async fn rebuild_all_with_force(&mut self, force_rebuild: bool) -> Result<()> {
        if force_rebuild {
            info!("Manual rebuild of all components requested (force: true)");
        } else {
            info!("Manual rebuild of all components requested (force: false)");
        }
        
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
                // Manual rebuild (not triggered by file changes)
                self.handle_manual_rebuild(component_names, force_rebuild).await
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
        
        // Stop all running Docker services
        let services_to_stop = {
            let state_guard = self.state.read().await;
            state_guard.running_services().to_vec()
        };
        
        if !services_to_stop.is_empty() {
            info!("Stopping {} Docker services", services_to_stop.len());
            if let Err(e) = self.lifecycle_manager.stop_services(&services_to_stop).await {
                warn!("Failed to stop services during shutdown: {}", e);
            }
        }
        
        // Also update component states
        let component_names: Vec<String> = {
            let state = self.state.read().await;
            state.components().keys().cloned().collect()
        };
        
        for component_name in component_names {
            if let Err(e) = self.lifecycle_manager.stop_component(&component_name).await {
                warn!("Failed to update component {} state during shutdown: {}", component_name, e);
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
            // Skip components that don't require Docker builds (LocalService, PureKubernetes, etc.)
            if !spec.build_type.requires_docker_build() {
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
            
            // Ports are already resolved in from_product_dir, just use them
            let host_port = spec.port.expect("Port should have been resolved");
            let target_port = spec.target_port.expect("Target port should have been resolved");
            
            let service = crate::ContainerService {
                id: uuid::Uuid::new_v4().to_string(),
                name: spec.component_name.clone(),
                image,
                host: spec.component_name.clone(),
                port: host_port,
                target_port,
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
    pub fn set_output_sink(&mut self, sink: Arc<tokio::sync::Mutex<Box<dyn rush_output::simple::Sink>>>) {
        self.output_sink = sink;
        
        // Store for later propagation to build orchestrator and lifecycle manager
        // This will be done during launch() when they are active
    }
    
    /// Set output sink from Box
    pub fn set_output_sink_boxed(&mut self, sink: Box<dyn rush_output::simple::Sink>) {
        self.output_sink = Arc::new(tokio::sync::Mutex::new(sink));
    }
    
    /// Add an environment variable
    pub fn add_env_var(&mut self, key: String, value: String) {
        // Add to component specs environment
        for spec in &mut self.component_specs {
            spec.dotenv.insert(key.clone(), value.clone());
        }
    }
    
    /// Set verbose mode
    pub fn set_verbose(&mut self, verbose: bool) {
        // Update configuration for verbose logging
        // This would typically be handled through the configuration system
        info!("Verbose mode set to: {}", verbose);
    }
    
    /// Set force rebuild flag
    pub fn set_force_rebuild(&mut self, force: bool) {
        // Store the force rebuild setting for use in build operations
        // This affects the behavior of build_components method
        info!("Force rebuild set to: {}", force);
        self.force_rebuild = force;
    }
    
    /// Get the Docker client
    pub fn docker_client(&self) -> Arc<dyn DockerClient> {
        self.docker_integration.client()
    }
    
    /// Get component specs
    pub fn component_specs(&self) -> &Vec<ComponentBuildSpec> {
        &self.component_specs
    }
    
    /// Get mutable component specs
    pub fn component_specs_mut(&mut self) -> &mut Vec<ComponentBuildSpec> {
        &mut self.component_specs
    }
    
    /// Get a change processor for file watching
    pub fn change_processor(&self) -> Arc<crate::watcher::ChangeProcessor> {
        // Create a change processor for file watching
        // File watching is handled by WatcherCoordinator
        let product_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        Arc::new(crate::watcher::ChangeProcessor::new(&product_dir, 500))
    }

    /// Setup file watchers for components with watch patterns
    pub async fn setup_watchers(&mut self) -> Result<()> {
        info!("Setting up file watchers for components with watch patterns");

        // Collect all unique directories to watch based on expanded patterns
        let mut watch_dirs = std::collections::HashSet::new();

        for spec in &self.component_specs {
            if let Some(watch) = &spec.watch {
                // Expand patterns to get actual files/directories to watch
                match watch.expand_patterns_from(&self.config.base.product_dir) {
                    Ok(paths) => {
                        for path in paths {
                            // Add the parent directory of each matched file
                            if let Some(parent) = path.parent() {
                                watch_dirs.insert(parent.to_path_buf());
                            }
                        }
                        info!("Component '{}' will watch {} directories based on patterns",
                            spec.component_name, watch_dirs.len());
                    }
                    Err(e) => {
                        warn!("Failed to expand watch patterns for component '{}': {}",
                            spec.component_name, e);
                    }
                }
            }
        }

        // If we have directories to watch, ensure the watcher is active
        if !watch_dirs.is_empty() {
            if let Some(watcher) = &mut self.watcher_coordinator {
                // The watcher is already initialized with component specs
                // Just ensure it's watching the product directory (recursive)
                if let Err(e) = watcher.watch_directory(&self.config.base.product_dir) {
                    warn!("Failed to start file watcher: {}", e);
                }
                info!("File watcher active, monitoring {} unique directories", watch_dirs.len());
            }
        } else {
            info!("No watch patterns defined, file watching disabled");
        }

        Ok(())
    }

    /// Check if any components need rebuilding based on tag changes
    pub async fn check_for_tag_changes(&self) -> Vec<String> {
        let mut changed_components = Vec::new();

        for spec in &self.component_specs {
            // Skip if no watch patterns defined
            if spec.watch.is_none() {
                continue;
            }

            // Check if rebuild is needed based on tag comparison
            match self.needs_rebuild(spec).await {
                Ok(true) => {
                    debug!("Component '{}' needs rebuild (tag changed)", spec.component_name);
                    changed_components.push(spec.component_name.clone());
                }
                Ok(false) => {
                    // No rebuild needed
                }
                Err(e) => {
                    warn!("Failed to check rebuild status for '{}': {}",
                        spec.component_name, e);
                }
            }
        }

        if !changed_components.is_empty() {
            info!("Tag changes detected for {} components: {:?}",
                changed_components.len(), changed_components);
        }

        changed_components
    }

    /// Trigger rebuild for components with changed tags
    pub async fn trigger_tag_based_rebuild(&mut self) -> Result<()> {
        let changed_components = self.check_for_tag_changes().await;

        if !changed_components.is_empty() {
            // Create a change batch for the rebuild
            let mut batch = crate::watcher::handler::ChangeBatch::new();
            batch.affected_components = changed_components.into_iter().collect();

            info!("Triggering rebuild for {} components due to tag changes",
                batch.affected_components.len());

            self.handle_rebuild(batch).await
        } else {
            Ok(())
        }
    }

    /// Check if a component needs rebuild based on tag change
    pub async fn needs_rebuild(&self, spec: &ComponentBuildSpec) -> Result<bool> {
        // Get current tag
        let current_tag = self.build_orchestrator.tag_generator.compute_tag(spec)?;

        // Get deployed tag (from running container or cache)
        let deployed_tag = self.get_deployed_tag(&spec.component_name).await?;

        // Simple comparison
        debug!("Component '{}': current_tag={}, deployed_tag={}",
            spec.component_name, current_tag, deployed_tag);
        Ok(current_tag != deployed_tag)
    }

    /// Get the tag of the currently deployed container or cached image
    pub async fn get_deployed_tag(&self, component_name: &str) -> Result<String> {
        // First check if there's a running container
        let container_name = rush_core::naming::NamingConvention::container_name(&self.config.base.product_name, component_name);

        // Try to get container ID by name and check its status
        if let Ok(container_id) = self.docker_client().get_container_by_name(&container_name).await {
            // Container exists, try to get its image tag
            // We'll just use the container ID as a proxy for now
            debug!("Found running container for '{}': {}", component_name, container_id);
            // In a real implementation, we'd need to inspect the container to get its image tag
            // For now, we'll skip this and check other sources
        }

        // Check if we have a built image in memory
        if let Some(image_name) = self.built_images.get(component_name) {
            if let Some(tag_pos) = image_name.rfind(':') {
                let tag = &image_name[tag_pos + 1..];
                debug!("Found built image for '{}' with tag: {}", component_name, tag);
                return Ok(tag.to_string());
            }
        }

        // Check the build cache
        let cache_guard = self.build_orchestrator.cache.lock().await;
        if let Some(cached_entry) = cache_guard.get_raw_entry(component_name).await {
            if let Some(tag_pos) = cached_entry.image_name.rfind(':') {
                let tag = &cached_entry.image_name[tag_pos + 1..];
                debug!("Found cached image for '{}' with tag: {}", component_name, tag);
                return Ok(tag.to_string());
            }
        }

        // No existing deployment or cached image
        debug!("No deployed or cached image found for '{}'", component_name);
        Ok(String::new())
    }
    
    /// Build all components
    pub async fn build(&mut self) -> Result<()> {
        info!("Building all components");
        
        {
            let mut state = self.state.write().await;
            if state.phase() != &ReactorPhase::Building {
                state.transition_to(ReactorPhase::Building)?;
            }
        }
        
        // Propagate output sink to build orchestrator
        self.build_orchestrator.set_output_sink(self.output_sink.clone()).await;
        
        let built_images = self.build_orchestrator.build_components(
            self.component_specs.clone(),
            false, // force_rebuild
        ).await?;
        
        self.built_images = built_images;

        // No state transition needed - stay in Building state
        // The caller can decide what state to transition to next

        info!("All components built successfully");
        Ok(())
    }
    
    /// Roll out to production using GitOps workflow
    pub async fn rollout(&mut self) -> Result<()> {
        info!("Starting GitOps rollout...");

        // Step 1: Build and push images to registry
        self.build_and_push().await?;

        // Step 2: Build Kubernetes manifests with secrets
        // Note: build_manifests() already handles:
        // - Fetching secrets from vault for each component
        // - Encoding secrets with the configured encoder (Base64 or Noop)
        // - Generating manifests with secrets injected
        // - Optionally applying SealedSecrets with kubeseal
        self.build_manifests().await?;

        // Step 3: Initialize infrastructure repository
        let infra_repo = self.create_infrastructure_repo()?;

        // Step 4: Checkout/clone infrastructure repository
        infra_repo.checkout().await?;

        // Step 5: Copy manifests to infrastructure repository
        let source_directory = self.k8s_manifest_dir
            .as_ref()
            .ok_or_else(|| Error::Internal("Manifests not built".to_string()))?;
        infra_repo.copy_manifests(source_directory).await?;

        // Step 6: Commit and push to trigger GitOps deployment
        let commit_message = format!(
            "Deploying {} for {}",
            self.config.base.environment,
            self.config.base.product_name
        );
        infra_repo.commit_and_push(&commit_message).await?;

        info!("GitOps rollout completed successfully");
        Ok(())
    }

    /// Create infrastructure repository for GitOps
    fn create_infrastructure_repo(&self) -> Result<rush_k8s::infrastructure::InfrastructureRepo> {
        // Load the full config to get infrastructure_repository
        let root_dir = std::env::var("RUSHD_ROOT")
            .map_err(|_| Error::Config("RUSHD_ROOT not set".to_string()))?;
        let config_loader = rush_config::ConfigLoader::new(std::path::PathBuf::from(&root_dir));
        let config = config_loader.load_config(
            &self.config.base.product_name,
            &self.config.base.environment,
            &self.config.base.docker_registry,
            8129, // Default port, not used for rollout
        ).map_err(|e| Error::Config(e.to_string()))?;

        let toolchain = Arc::new(rush_toolchain::ToolchainContext::default());

        let local_path = self.config.base.product_dir.join(".infra");

        Ok(rush_k8s::infrastructure::InfrastructureRepo::new(
            config.infrastructure_repository().to_string(),
            local_path,
            self.config.base.environment.clone(),
            self.config.base.product_name.clone(),
            toolchain,
        ))
    }
    
    /// Perform Docker login if credentials are configured
    async fn docker_login(&self) -> Result<()> {
        // Check if we have credentials configured
        let username = self.config.registry.username.as_ref();
        let password = self.config.registry.password.as_ref();
        
        match (username, password) {
            (Some(user), Some(pass)) => {
                info!("Logging into Docker registry...");
                
                let registry_url = self.config.registry.url.as_deref().unwrap_or("");
                
                // Create a temporary file for the password to avoid shell injection
                use std::io::Write;
                let mut temp_file = ::tempfile::NamedTempFile::new()
                    .map_err(|e| Error::Docker(format!("Failed to create temp file: {}", e)))?;
                    
                temp_file.write_all(pass.as_bytes())
                    .map_err(|e| Error::Docker(format!("Failed to write password: {}", e)))?;
                    
                temp_file.flush()
                    .map_err(|e| Error::Docker(format!("Failed to flush temp file: {}", e)))?;
                
                // Build docker login command
                let mut cmd = tokio::process::Command::new("docker");
                cmd.arg("login");
                
                if !registry_url.is_empty() {
                    cmd.arg(registry_url);
                }
                
                cmd.args(&["--username", user, "--password-stdin"]);
                cmd.stdin(std::process::Stdio::from(temp_file.reopen()
                    .map_err(|e| Error::Docker(format!("Failed to reopen temp file: {}", e)))?));
                
                let output = cmd.output().await
                    .map_err(|e| Error::Docker(format!("Failed to run docker login: {}", e)))?;
                
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    
                    // Check for common errors
                    if stderr.contains("unauthorized") || stdout.contains("unauthorized") {
                        return Err(Error::Docker("Docker login failed: Invalid credentials".to_string()));
                    }
                    
                    return Err(Error::Docker(format!("Docker login failed: {}", stderr)));
                }
                
                info!("Successfully logged into Docker registry");
            }
            (Some(_), None) => {
                warn!("Docker registry username configured but no password provided");
                if !self.config.registry.use_credentials_helper {
                    return Err(Error::Docker("Registry password required when credentials helper is disabled".to_string()));
                }
            }
            (None, Some(_)) => {
                warn!("Docker registry password configured but no username provided");
            }
            (None, None) => {
                if self.config.registry.url.is_some() && !self.config.registry.use_credentials_helper {
                    info!("No registry credentials configured, using anonymous access");
                }
            }
        }
        
        Ok(())
    }
    
    /// Get the full image tag including registry URL if configured
    fn get_registry_tag(&self, image_name: &str) -> String {
        if let Some(url) = &self.config.registry.url {
            // Skip empty URLs (for local environment)
            if url.is_empty() {
                return image_name.to_string();
            }
            if let Some(namespace) = &self.config.registry.namespace {
                format!("{}/{}/{}", url, namespace, image_name)
            } else {
                format!("{}/{}", url, image_name)
            }
        } else if let Some(namespace) = &self.config.registry.namespace {
            format!("{}/{}", namespace, image_name)
        } else {
            image_name.to_string()
        }
    }

    /// Build and push Docker images for deployable components
    pub async fn build_and_push(&mut self) -> Result<()> {
        info!("Building and pushing Docker images for deployment...");

        // Filter components that produce pushable images
        let pushable_components: Vec<ComponentBuildSpec> = self.component_specs
            .iter()
            .filter(|spec| Self::produces_pushable_image(&spec.build_type))
            .cloned()
            .collect();

        if pushable_components.is_empty() {
            info!("No components with pushable images found");
            return Ok(());
        }

        info!("Found {} components with pushable images", pushable_components.len());

        // Build only pushable components
        let built_images = self.build_orchestrator.build_components(
            pushable_components,
            false, // force_rebuild
        ).await?;

        self.built_images = built_images;

        // Skip pushing for local environment (no registry configured)
        if self.config.registry.url.is_none() {
            info!("Skipping Docker push for local environment");
            return Ok(());
        }

        // Login to registry if needed
        self.docker_login().await?;

        // Push images to registry
        for (component_name, image_name) in &self.built_images {
            // Get the full registry tag
            let registry_tag = self.get_registry_tag(image_name);
            info!("Pushing image: {} -> {}", component_name, registry_tag);

            // Tag the image for the registry if needed
            if registry_tag != *image_name {
                // Tag the local image with the registry URL
                let tag_output = tokio::process::Command::new("docker")
                    .args(&["tag", image_name, &registry_tag])
                    .output()
                    .await
                    .map_err(|e| Error::Docker(format!("Failed to tag image: {}", e)))?;

                if !tag_output.status.success() {
                    let stderr = String::from_utf8_lossy(&tag_output.stderr);
                    return Err(Error::Docker(format!("Failed to tag image: {}", stderr)));
                }
            }

            // Use the Docker client to push the image
            if let Err(e) = self.docker_integration.client().push_image(&registry_tag).await {
                error!("Failed to push image {} for component {}: {}",
                       registry_tag, component_name, e);
                return Err(e);
            }

            info!("Successfully pushed image: {}", registry_tag);
        }

        info!("Build and push completed successfully");
        Ok(())
    }

    /// Determines if a build type produces a pushable Docker image
    fn produces_pushable_image(build_type: &BuildType) -> bool {
        matches!(
            build_type,
            BuildType::RustBinary { .. } |
            BuildType::TrunkWasm { .. } |
            BuildType::DixiousWasm { .. } |
            BuildType::Script { .. } |
            BuildType::Zola { .. } |
            BuildType::Book { .. } |
            BuildType::Ingress { .. } |
            BuildType::PureDockerImage { .. }
        )
    }
    
    /// Select Kubernetes context for deployment
    pub async fn select_kubernetes_context(&self, context: &str) -> Result<()> {
        info!("Selecting Kubernetes context: {}", context);
        
        // Kubectl context selection implementation would go here
        // This would typically run: kubectl config use-context <context>
        debug!("Kubernetes context selection not implemented yet: {}", context);
        
        Ok(())
    }
    
    /// Apply Kubernetes manifests to the cluster
    pub async fn apply(&mut self) -> Result<()> {
        info!("Applying Kubernetes manifests...");
        
        // Ensure manifests have been built
        let manifest_dir = match &self.k8s_manifest_dir {
            Some(dir) => dir,
            None => {
                // Build manifests if not already done
                self.build_manifests().await?;
                self.k8s_manifest_dir.as_ref()
                    .ok_or_else(|| Error::Internal("Failed to build manifests".to_string()))?
            }
        };
        
        // Check if manifest directory exists
        if !manifest_dir.exists() {
            return Err(Error::Filesystem(format!(
                "Manifest directory does not exist: {}",
                manifest_dir.display()
            )));
        }
        
        // Create kubectl wrapper with configuration
        let mut kubectl_config = rush_k8s::KubectlConfig::default();
        
        // Set namespace from environment or use default
        let namespace = std::env::var("K8S_NAMESPACE")
            .unwrap_or_else(|_| format!("{}-{}", 
                self.config.build.product_name, 
                std::env::var("RUSH_ENV").unwrap_or_else(|_| "default".to_string())
            ));
        kubectl_config.namespace = Some(namespace);
        
        // Set context if provided
        if let Ok(context) = std::env::var("K8S_CONTEXT") {
            kubectl_config.context = Some(context);
        }
        
        // Enable dry-run if requested
        kubectl_config.dry_run = std::env::var("K8S_DRY_RUN")
            .unwrap_or_else(|_| "false".to_string()) == "true";
        
        kubectl_config.verbose = true;
        
        let kubectl = rush_k8s::Kubectl::new(kubectl_config);
        
        // Apply all manifests in the directory
        let results = kubectl.apply_dir(manifest_dir).await?;
        
        // Check if all applications succeeded
        let failed_count = results.iter().filter(|r| !r.success).count();
        if failed_count > 0 {
            return Err(Error::External(format!(
                "Failed to apply {} out of {} manifests", 
                failed_count, 
                results.len()
            )));
        }
        
        info!("Successfully applied {} Kubernetes manifests", results.len());
        
        // Track deployment versions for rollback support
        if !kubectl.config.dry_run {
            let timestamp = chrono::Utc::now();
            let version = std::env::var("GIT_COMMIT")
                .or_else(|_| std::env::var("DEPLOYMENT_VERSION"))
                .unwrap_or_else(|_| timestamp.timestamp().to_string());
            
            // Track each deployed component
            for spec in &self.component_specs {
                // Skip components that don't create deployments
                match &spec.build_type {
                    BuildType::LocalService { .. } => continue,
                    _ => {}
                }
                
                // Calculate manifest hash for change detection
                let manifest_path = manifest_dir.join(format!("{}-deployment.yaml", spec.component_name));
                let manifest_hash = if manifest_path.exists() {
                    let content = std::fs::read_to_string(&manifest_path)?;
                    format!("{:x}", md5::compute(content.as_bytes()))
                } else {
                    String::new()
                };
                
                let deployment_version = rush_k8s::kubectl::DeploymentVersion {
                    deployment_name: spec.component_name.clone(),
                    namespace: kubectl.config.namespace.clone().unwrap_or_else(|| "default".to_string()),
                    version: version.clone(),
                    timestamp,
                    manifest_hash,
                };
                
                self.deployment_versions.push(deployment_version);
            }
            
            // Keep only last 10 versions per deployment
            self.deployment_versions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            self.deployment_versions.truncate(10 * self.component_specs.len());
        }
        
        // Wait for deployments to be ready if not in dry-run mode
        if !kubectl.config.dry_run {
            info!("Waiting for deployments to be ready...");
            for spec in &self.component_specs {
                // Skip components that don't create deployments
                match &spec.build_type {
                    BuildType::LocalService { .. } => continue,
                    _ => {}
                }
                
                match kubectl.wait_for_deployment(&spec.component_name, 300).await {
                    Ok(_) => info!("Deployment {} is ready", spec.component_name),
                    Err(e) => warn!("Deployment {} may not be ready: {}", spec.component_name, e),
                }
            }
        }
        
        Ok(())
    }
    
    /// Remove Kubernetes resources from the cluster
    pub async fn unapply(&mut self) -> Result<()> {
        info!("Removing Kubernetes resources...");
        
        // Use manifest directory if available
        if let Some(manifest_dir) = &self.k8s_manifest_dir {
            if manifest_dir.exists() {
                // Create kubectl wrapper with same configuration as apply
                let mut kubectl_config = rush_k8s::KubectlConfig::default();
                
                // Set namespace from environment or use default
                let namespace = std::env::var("K8S_NAMESPACE")
                    .unwrap_or_else(|_| format!("{}-{}", 
                        self.config.build.product_name, 
                        std::env::var("RUSH_ENV").unwrap_or_else(|_| "default".to_string())
                    ));
                kubectl_config.namespace = Some(namespace);
                
                // Set context if provided
                if let Ok(context) = std::env::var("K8S_CONTEXT") {
                    kubectl_config.context = Some(context);
                }
                
                // Enable dry-run if requested
                kubectl_config.dry_run = std::env::var("K8S_DRY_RUN")
                    .unwrap_or_else(|_| "false".to_string()) == "true";
                
                kubectl_config.verbose = true;
                
                let kubectl = rush_k8s::Kubectl::new(kubectl_config);
                
                // Delete all resources from manifests
                let results = kubectl.delete_dir(manifest_dir).await?;
                
                // Check results
                let failed_count = results.iter()
                    .filter(|r| !r.success && !r.stderr.contains("NotFound"))
                    .count();
                
                if failed_count > 0 {
                    warn!("Failed to delete {} out of {} manifests", failed_count, results.len());
                } else {
                    info!("Successfully removed {} Kubernetes resources", results.len());
                }
            } else {
                warn!("Manifest directory does not exist, nothing to remove");
            }
        } else {
            warn!("No manifests have been generated, nothing to remove");
        }
        
        Ok(())
    }
    
    /// Rollback to a previous deployment version
    pub async fn rollback(&mut self, version: Option<String>) -> Result<()> {
        info!("Rolling back Kubernetes deployment...");
        
        if self.deployment_versions.is_empty() {
            return Err(Error::Internal("No deployment versions available for rollback".to_string()));
        }
        
        // Find the version to rollback to
        let target_version = if let Some(v) = version {
            self.deployment_versions.iter()
                .find(|dv| dv.version == v)
                .ok_or_else(|| Error::Internal(format!("Version {} not found", v)))?
        } else {
            // Rollback to previous version (skip current which is at index 0)
            self.deployment_versions.get(1)
                .ok_or_else(|| Error::Internal("No previous version available".to_string()))?
        };
        
        info!("Rolling back to version {} from {}", 
              target_version.version, 
              target_version.timestamp);
        
        // Create kubectl wrapper
        let mut kubectl_config = rush_k8s::KubectlConfig::default();
        kubectl_config.namespace = Some(target_version.namespace.clone());
        
        if let Ok(context) = std::env::var("K8S_CONTEXT") {
            kubectl_config.context = Some(context);
        }
        
        let kubectl = rush_k8s::Kubectl::new(kubectl_config);
        
        // Perform rollback using kubectl rollout undo
        for deployment_version in &self.deployment_versions {
            if deployment_version.version == target_version.version {
                info!("Rolling back deployment: {}", deployment_version.deployment_name);
                let result = kubectl.execute(vec![
                    "rollout".to_string(),
                    "undo".to_string(),
                    format!("deployment/{}", deployment_version.deployment_name),
                ]).await?;
                
                if !result.success {
                    return Err(Error::External(format!(
                        "Failed to rollback {}: {}", 
                        deployment_version.deployment_name,
                        result.stderr
                    )));
                }
                
                // Wait for rollout to complete
                kubectl.rollout_status(&deployment_version.deployment_name).await?;
                info!("Successfully rolled back {}", deployment_version.deployment_name);
            }
        }
        
        Ok(())
    }
    
    /// Get deployment history
    pub fn get_deployment_history(&self) -> Vec<rush_k8s::kubectl::DeploymentVersion> {
        self.deployment_versions.clone()
    }
    
    /// Install Kubernetes manifests
    pub async fn install_manifests(&mut self) -> Result<()> {
        info!("Installing Kubernetes manifests...");
        
        // TODO: Install manifests
        debug!("Kubernetes manifest installation not implemented yet");
        
        Ok(())
    }
    
    /// Uninstall Kubernetes manifests
    pub async fn uninstall_manifests(&mut self) -> Result<()> {
        info!("Uninstalling Kubernetes manifests...");
        
        // TODO: Uninstall manifests
        debug!("Kubernetes manifest uninstallation not implemented yet");
        
        Ok(())
    }
    
    /// Build Kubernetes manifests
    pub async fn build_manifests(&mut self) -> Result<()> {
        info!("Building Kubernetes manifests...");

        // Create output directory for manifests
        let output_dir = std::path::PathBuf::from(".rush/k8s");

        // Clear existing manifests
        if output_dir.exists() {
            std::fs::remove_dir_all(&output_dir)
                .map_err(|e| Error::Filesystem(format!("Failed to remove k8s directory: {}", e)))?;
        }
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| Error::Filesystem(format!("Failed to create k8s directory: {}", e)))?;

        // Determine namespace from environment or use default
        let namespace = std::env::var("K8S_NAMESPACE")
            .unwrap_or_else(|_| format!("{}-{}",
                self.config.base.product_name,
                self.config.base.environment
            ));

        let environment = self.config.base.environment.clone();
        let docker_registry = self.config.base.docker_registry.clone();

        // Process each component that has k8s manifests
        for spec in &self.component_specs {
            // Skip components without K8s manifests
            let k8s_path = match &spec.k8s {
                Some(path) => path,
                None => continue,
            };

            info!("Building manifests for component: {}", spec.component_name);

            // Create component-specific output directory with priority
            let component_dir_name = format!("{}_{}", spec.priority, spec.component_name);
            let component_output_dir = output_dir.join(&component_dir_name);
            std::fs::create_dir_all(&component_output_dir)
                .map_err(|e| Error::Filesystem(format!("Failed to create component k8s directory: {}", e)))?;

            // Find the template directory
            let template_dir = std::path::PathBuf::from(&self.config.base.product_dir)
                .join(k8s_path);

            if !template_dir.exists() {
                warn!("K8s template directory not found for {}: {}",
                      spec.component_name, template_dir.display());
                continue;
            }

            // Get component-specific secrets from vault
            let component_secrets = if let Some(vault) = &self.vault {
                match vault.lock().unwrap().get(
                    &spec.product_name,
                    &spec.component_name,
                    &environment
                ).await {
                    Ok(secrets) => {
                        // Apply base64 encoding if we have a secrets encoder
                        if let Some(encoder) = &self.secrets_encoder {
                            encoder.encode_secrets(secrets)
                        } else {
                            secrets
                        }
                    }
                    Err(e) => {
                        debug!("No secrets found for component {}: {}", spec.component_name, e);
                        HashMap::new()
                    }
                }
            } else {
                HashMap::new()
            };

            // Create build context for this component
            let toolchain = Arc::new(rush_toolchain::ToolchainContext::default());
            let build_context = spec.generate_build_context(Some(toolchain), component_secrets);

            // Add additional context variables
            let mut tera_context = tera::Context::from_serialize(&build_context)
                .map_err(|e| Error::Template(format!("Failed to create context: {}", e)))?;
            tera_context.insert("namespace", &namespace);
            tera_context.insert("environment", &environment);
            tera_context.insert("docker_registry", &docker_registry);
            tera_context.insert("component", &spec.component_name);
            tera_context.insert("product_uri", &spec.product_name.replace('.', "-"));

            // Find the built image name for this component
            if let Some(image_tag) = self.built_images.get(&spec.component_name) {
                tera_context.insert("image_name", image_tag);
            }

            // Process each template file in the directory
            let template_files = std::fs::read_dir(&template_dir)
                .map_err(|e| Error::Filesystem(format!("Failed to read template directory: {}", e)))?;

            for entry in template_files {
                let entry = entry.map_err(|e| Error::Filesystem(format!("Failed to read directory entry: {}", e)))?;
                let path = entry.path();

                // Skip non-yaml files
                if !path.extension().map_or(false, |ext| ext == "yaml" || ext == "yml") {
                    continue;
                }

                let file_name = path.file_name().unwrap().to_str().unwrap();
                debug!("Processing template: {}", file_name);

                // Read template content
                let template_content = std::fs::read_to_string(&path)
                    .map_err(|e| Error::Filesystem(format!("Failed to read template {}: {}", file_name, e)))?;

                // Render template with Tera
                let mut tera = tera::Tera::default();
                tera.add_raw_template(file_name, &template_content)
                    .map_err(|e| Error::Template(format!("Failed to add template: {}", e)))?;

                let rendered = tera.render(file_name, &tera_context)
                    .map_err(|e| Error::Template(format!("Failed to render template {}: {}", file_name, e)))?;

                // Write rendered manifest to output directory
                let output_path = component_output_dir.join(file_name);
                std::fs::write(&output_path, rendered)
                    .map_err(|e| Error::Filesystem(format!("Failed to write manifest: {}", e)))?;

                // Apply SealedSecrets encoder if this is a secrets file
                if file_name.contains("secret") {
                    let use_sealed_secrets = std::env::var("K8S_USE_SEALED_SECRETS")
                        .unwrap_or_else(|_| "false".to_string()) == "true";

                    if use_sealed_secrets {
                        debug!("Applying SealedSecrets encoder to {}", file_name);
                        let encoder = rush_k8s::encoder::create_encoder("kubeseal");
                        if let Err(e) = encoder.encode_file(output_path.to_str().unwrap()) {
                            warn!("Failed to encode secrets with kubeseal: {}. Secrets will remain unencrypted.", e);
                        }
                    }
                }
            }

            info!("Generated manifests for {} in {}", spec.component_name, component_output_dir.display());
        }

        // Count total manifests generated
        let manifest_count = std::fs::read_dir(&output_dir)
            .map(|entries| entries.count())
            .unwrap_or(0);

        info!("Generated Kubernetes manifests for {} components in {}",
              manifest_count,
              output_dir.display());

        // Store the output directory for later use in apply()
        self.k8s_manifest_dir = Some(output_dir);

        Ok(())
    }
    
    /// Deploy to Kubernetes
    pub async fn deploy(&mut self) -> Result<()> {
        info!("Deploying to Kubernetes...");
        
        // Build manifests and apply them
        self.build_manifests().await?;
        self.apply().await?;
        
        Ok(())
    }
    
    /// Check if a rebuild is in progress
    pub fn rebuild_in_progress(&self) -> bool {
        // Check the current phase to determine if rebuilding
        match self.state.try_read() {
            Ok(state) => matches!(state.phase(), ReactorPhase::Building | ReactorPhase::Rebuilding),
            Err(_) => false, // If we can't read the state, assume not rebuilding
        }
    }
    
    /// Set rebuild in progress state
    pub async fn set_rebuild_in_progress(&mut self, in_progress: bool) {
        let mut state = self.state.write().await;
        if in_progress {
            if let Err(e) = state.transition_to(ReactorPhase::Rebuilding) {
                debug!("Could not transition to rebuilding state: {}", e);
            }
        } else {
            if let Err(e) = state.transition_to(ReactorPhase::Idle) {
                debug!("Could not transition to idle state: {}", e);
            }
        }
    }
    
    /// Setup Docker network for the reactor
    pub async fn setup_network(&self) -> Result<()> {
        let network_name = &self.config.base.network_name;
        
        // Check if network already exists
        if !self.docker_integration.client().network_exists(network_name).await? {
            info!("Creating Docker network: {}", network_name);
            self.docker_integration.client().create_network(network_name).await?;
        } else {
            debug!("Network {} already exists", network_name);
        }
        
        Ok(())
    }
    
    /// Combined launch method that sets up network and runs the reactor
    pub async fn launch(&mut self) -> Result<()> {
        info!("Starting primary reactor");
        
        // Setup Docker network first
        self.setup_network().await?;
        
        // Start the reactor lifecycle management
        self.start().await?;
        
        // Build and start all containers initially
        if let Err(e) = self.rebuild_all().await {
            error!("Initial build failed: {}", e);
            // Continue anyway - the reactor will handle file watching and rebuilds
            info!("Waiting for file changes to retry build...");
            info!("💡 Tip: Fix the build error and save a file to trigger rebuild");
        }
        
        // Run the main reactor loop
        self.run().await
    }
    
    /// Create a Reactor from a product directory
    /// This is the primary factory method for creating a reactor with all components configured
    pub async fn from_product_dir(
        config: Arc<rush_config::Config>,
        vault: Arc<std::sync::Mutex<dyn rush_security::Vault + Send>>,
        secrets_encoder: Arc<dyn rush_security::SecretsEncoder>,
        redirected_components: HashMap<String, (String, u16)>,
        silence_components: Vec<String>,
        network_manager: Arc<crate::network::NetworkManager>,
    ) -> Result<Self> {
        use std::collections::HashSet;
        use std::fs;
        use rush_build::{ComponentBuildSpec, BuildType};
        use crate::tagging::ImageTagGenerator;

        // Create toolchain and tag generator
        let toolchain = Arc::new(rush_toolchain::ToolchainContext::default());
        let tag_generator = Arc::new(ImageTagGenerator::new(
            toolchain.clone(),
            config.product_path().to_path_buf(),
        ));
        
        let product_path = config.product_path();
        let docker_client = Arc::new(crate::docker::DockerCliClient::new("docker".to_string()));
        
        // Create set of silenced components
        let silenced_components = silence_components.into_iter().collect::<HashSet<_>>();
        
        // Read stack configuration
        let stack_config = 
            match fs::read_to_string(format!("{}/stack.spec.yaml", product_path.display())) {
                Ok(config) => config,
                Err(e) => return Err(format!("Failed to read stack config: {e}").into()),
            };
        
        // Parse stack spec and create component build specs
        let spec = match serde_yaml::from_str::<serde_yaml::Value>(&stack_config) {
            Ok(spec) => spec,
            Err(e) => return Err(format!("Failed to parse stack config: {e}").into()),
        };
        
        // Build component specs from stack configuration
        let mut component_specs = Vec::new();
        
        if let Some(components) = spec.as_mapping() {
            for (name, component_config) in components {
                if let Some(name_str) = name.as_str() {
                    // Check if this component should be silenced
                    if silenced_components.contains(name_str) {
                        continue;
                    }
                    
                    // Use the proper from_yaml method to create ComponentBuildSpec
                    // This will properly load .env and .env.secrets files
                    // We need to inject the component_name into the YAML since it's not present
                    let mut component_config_with_name = component_config.clone();
                    if let serde_yaml::Value::Mapping(ref mut map) = component_config_with_name {
                        map.insert(
                            serde_yaml::Value::String("component_name".to_string()),
                            serde_yaml::Value::String(name_str.to_string())
                        );
                    }
                    
                    // Try to use the from_yaml method which properly loads env files
                    let mut spec = rush_build::ComponentBuildSpec::from_yaml(
                        config.clone(),
                        rush_build::Variables::empty(),
                        &component_config_with_name
                    );
                    // Compute deterministic tag for this component
                    let tag = tag_generator.compute_tag(&spec)
                        .unwrap_or_else(|e| {
                            warn!("Failed to compute tag for {}: {}, using 'latest'", name_str, e);
                            "latest".to_string()
                        });
                    spec.tagged_image_name = Some(format!("{}:{}", name_str, tag));
            
                    // Add the spec to our list
                    component_specs.push(spec);
                }
            }
        }
        
        // Create modular reactor configuration
        let mut modular_config = ModularReactorConfig::default();
        // Use consistent network name from network manager
        let network_name = network_manager.network_name().to_string();
        modular_config.base.network_name = network_name.clone();
        modular_config.base.product_name = config.product_name().to_string();
        modular_config.base.product_dir = config.product_path().to_path_buf();
        modular_config.base.environment = config.environment().to_string();
        // Git hash no longer needed at config level since tags are per-component
        modular_config.base.git_hash = String::new();
        modular_config.base.redirected_components = redirected_components;
        modular_config.base.start_port = config.start_port();
        modular_config.docker.use_enhanced_client = true;
        modular_config.watcher.auto_rebuild = true;
        modular_config.lifecycle.auto_restart = true;
        
        // Configure Docker registry from config
        // Only set URL if it's not empty (for local environments)
        let registry = config.docker_registry();
        modular_config.registry.url = if registry.is_empty() {
            None
        } else {
            Some(registry.to_string())
        };
        modular_config.registry.namespace = config.docker_registry_namespace().map(|s| s.to_string());
        modular_config.registry.username = config.docker_registry_username().map(|s| s.to_string());
        modular_config.registry.password = config.docker_registry_password().map(|s| s.to_string());
        
        // Configure build orchestrator with the product directory
        modular_config.build.product_dir = config.product_path().to_path_buf();
        modular_config.build.product_name = config.product_name().to_string();
        
        // Configure lifecycle manager with the product name and network name
        modular_config.lifecycle.product_name = config.product_name().to_string();
        modular_config.lifecycle.network_name = network_name;
        
        // Resolve ports for all components before creating the reactor
        Self::resolve_component_ports(&mut component_specs, &config);
        
        // Create the reactor using the existing new() method
        let mut reactor = Self::new(modular_config, docker_client, component_specs).await?;
        
        // Set the vault and secrets encoder
        reactor.vault = Some(vault);
        reactor.secrets_encoder = Some(secrets_encoder);
        
        Ok(reactor)
    }
    
    /// Scan a Dockerfile for EXPOSE directive to find the exposed port
    fn scan_dockerfile_for_expose(spec: &ComponentBuildSpec, product_dir: &std::path::Path) -> Option<u16> {
        if let Some(dockerfile_path) = spec.build_type.dockerfile_path() {
            let full_path = product_dir.join(&dockerfile_path);
            if let Ok(content) = std::fs::read_to_string(&full_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("EXPOSE ") {
                        let port_str = trimmed.trim_start_matches("EXPOSE ").trim();
                        if let Ok(port) = port_str.parse::<u16>() {
                            debug!("Found EXPOSE {} in Dockerfile for {}", port, spec.component_name);
                            return Some(port);
                        }
                    }
                }
            }
        }
        None
    }
    
    /// Resolve ports for all components before building
    fn resolve_component_ports(specs: &mut Vec<ComponentBuildSpec>, config: &Arc<rush_config::Config>) {
        let mut next_port = config.start_port();
        
        for spec in specs.iter_mut() {
            // Skip components that don't require Docker builds
            if !spec.build_type.requires_docker_build() {
                continue;
            }
            
            // Assign host port if not specified
            if spec.port.is_none() {
                spec.port = Some(next_port);
                info!("Auto-assigned port {} to component {}", next_port, spec.component_name);
                next_port += 1;
            }
            
            // Determine target port: YAML > Dockerfile EXPOSE > host port
            if spec.target_port.is_none() {
                let dockerfile_port = Self::scan_dockerfile_for_expose(spec, &config.product_path());
                spec.target_port = Some(dockerfile_port.unwrap_or_else(|| spec.port.unwrap()));
                info!("Set target_port {} for component {}", spec.target_port.unwrap(), spec.component_name);
            }
        }
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

    // Note: Full integration tests for needs_rebuild are complex due to the need for
    // proper ComponentBuildSpec setup with Config and Variables. The functionality
    // is tested through integration tests elsewhere.

}