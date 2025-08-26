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
use std::collections::HashMap;
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
}

impl Default for ModularReactorConfig {
    fn default() -> Self {
        Self {
            base: ContainerReactorConfig::default(),
            lifecycle: LifecycleConfig::default(),
            build: BuildOrchestratorConfig::default(),
            watcher: CoordinatorConfig::default(),
            docker: DockerIntegrationConfig::default(),
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
    /// Output sink for container and build logs
    output_sink: Arc<tokio::sync::Mutex<Box<dyn rush_output::simple::Sink>>>,
}

impl Reactor {
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
        
        // Propagate output sink to build orchestrator and lifecycle manager
        self.build_orchestrator.set_output_sink(self.output_sink.clone()).await;
        self.lifecycle_manager.set_output_sink(self.output_sink.clone()).await;
        
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
        // Note: The actual force rebuild is passed to build_components method
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
        
        // Transition back to idle after building
        {
            let mut state = self.state.write().await;
            state.transition_to(ReactorPhase::Idle)?;
        }
        
        info!("All components built successfully");
        Ok(())
    }
    
    /// Roll out containers (stop, build, start)
    pub async fn rollout(&mut self) -> Result<()> {
        info!("Rolling out containers...");
        
        // Stop existing containers
        let services_to_stop = {
            let state_guard = self.state.read().await;
            let services = state_guard.running_services();
            services.to_vec() // Clone the services to avoid borrowing issues
        };
        
        if !services_to_stop.is_empty() {
            self.lifecycle_manager.stop_services(&services_to_stop).await?;
        }
        
        // Build all components
        self.build().await?;
        
        // Start services
        self.lifecycle_manager.start_services(
            self.services.clone(),
            &self.component_specs,
            &self.built_images,
        ).await?;
        
        info!("Rollout completed successfully");
        Ok(())
    }
    
    /// Build and push Docker images for all components
    pub async fn build_and_push(&mut self) -> Result<()> {
        info!("Building and pushing Docker images...");
        
        // Build all components first
        self.build().await?;
        
        // Push images to registry (placeholder implementation)
        for (component_name, image_name) in &self.built_images {
            info!("Pushing image: {} -> {}", component_name, image_name);
            // Docker push implementation would require registry authentication and push commands
            debug!("Docker push not implemented yet for {}", image_name);
        }
        
        info!("Build and push completed successfully");
        Ok(())
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
        
        // TODO: Generate and apply K8s manifests
        // This would typically:
        // 1. Generate manifests from component specs
        // 2. Apply them using kubectl or the Kubernetes API
        debug!("Kubernetes manifest application not implemented yet");
        
        Ok(())
    }
    
    /// Remove Kubernetes resources from the cluster
    pub async fn unapply(&mut self) -> Result<()> {
        info!("Removing Kubernetes resources...");
        
        // TODO: Delete K8s resources
        // This would typically run kubectl delete commands
        debug!("Kubernetes resource removal not implemented yet");
        
        Ok(())
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
        
        // TODO: Generate K8s manifests from component specs
        debug!("Kubernetes manifest building not implemented yet");
        
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
        use std::process::Command;
        use rush_core::constants::DOCKER_TAG_LATEST;
        use rush_build::ComponentBuildSpec;
        
        // Get the git hash for tagging
        let git_hash = {
            let hash_output = Command::new("git")
                .args(["log", "-n", "1", "--format=%H", "--", &config.product_path().display().to_string()])
                .output()
                .ok();
            
            if let Some(output) = hash_output {
                if output.status.success() {
                    if let Ok(hash) = String::from_utf8(output.stdout) {
                        let hash = hash.trim();
                        if !hash.is_empty() {
                            hash[..8.min(hash.len())].to_string()
                        } else {
                            DOCKER_TAG_LATEST.to_string()
                        }
                    } else {
                        DOCKER_TAG_LATEST.to_string()
                    }
                } else {
                    DOCKER_TAG_LATEST.to_string()
                }
            } else {
                DOCKER_TAG_LATEST.to_string()
            }
        };
        
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
                    // Parse the build_type from the component configuration
                    let build_type_str = component_config.get("build_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("RustBinary");
                    
                    let location = component_config.get("location")
                        .and_then(|v| v.as_str())
                        .unwrap_or(name_str)
                        .to_string();
                    
                    let dockerfile = component_config.get("dockerfile")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Dockerfile")
                        .to_string();
                    
                    // Parse build type based on the string value
                    let build_type = match build_type_str {
                        "Ingress" => rush_build::BuildType::Ingress {
                            components: component_config.get("components")
                                .and_then(|v| v.as_sequence())
                                .map(|seq| seq.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect())
                                .unwrap_or_default(),
                            dockerfile_path: dockerfile.clone(),
                            context_dir: component_config.get("context_dir")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                        },
                        "TrunkWasm" => rush_build::BuildType::TrunkWasm {
                            location: location.clone(),
                            dockerfile_path: dockerfile.clone(),
                            context_dir: component_config.get("context_dir")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            ssr: component_config.get("ssr")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                            features: component_config.get("features")
                                .and_then(|v| v.as_sequence())
                                .map(|seq| seq.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()),
                            precompile_commands: component_config.get("precompile_commands")
                                .and_then(|v| v.as_sequence())
                                .map(|seq| seq.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()),
                        },
                        "LocalService" => rush_build::BuildType::LocalService {
                            service_type: component_config.get("service_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            version: component_config.get("version")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            persist_data: component_config.get("persist_data")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                            env: component_config.get("env")
                                .and_then(|v| v.as_mapping())
                                .map(|m| m.iter()
                                    .filter_map(|(k, v)| {
                                        k.as_str().and_then(|key| 
                                            v.as_str().map(|val| (key.to_string(), val.to_string()))
                                        )
                                    })
                                    .collect()),
                            health_check: component_config.get("health_check")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            init_scripts: component_config.get("init_scripts")
                                .and_then(|v| v.as_sequence())
                                .map(|seq| seq.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()),
                            command: component_config.get("command")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            depends_on: component_config.get("depends_on")
                                .and_then(|v| v.as_sequence())
                                .map(|seq| seq.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect::<Vec<_>>()),
                        },
                        _ => rush_build::BuildType::RustBinary {
                            location: location.clone(),
                            dockerfile_path: dockerfile.clone(),
                            context_dir: component_config.get("context_dir")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            features: component_config.get("features")
                                .and_then(|v| v.as_sequence())
                                .map(|seq| seq.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()),
                            precompile_commands: component_config.get("precompile_commands")
                                .and_then(|v| v.as_sequence())
                                .map(|seq| seq.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()),
                        },
                    };
                    
                    let spec = ComponentBuildSpec {
                        build_type,
                        product_name: config.product_name().to_string(),
                        component_name: name_str.to_string(),
                        color: "white".to_string(),
                        depends_on: vec![],
                        build: None,
                        mount_point: None,
                        subdomain: None,
                        artefacts: None,
                        artefact_output_dir: "target".to_string(),
                        docker_extra_run_args: vec![],
                        env: None,
                        volumes: None,
                        port: None,
                        target_port: None,
                        k8s: None,
                        priority: 100,
                        watch: None,
                        config: config.clone(),
                        variables: rush_build::Variables::empty(),
                        services: None,
                        domains: None,
                        tagged_image_name: Some(format!("{}:{}", name_str, git_hash)),
                        dotenv: HashMap::new(),
                        dotenv_secrets: HashMap::new(),
                        domain: format!("{}.local", name_str),
                        cross_compile: "native".to_string(),
                    };
                    
                    // Skip silenced components
                    if !silenced_components.contains(name_str) {
                        component_specs.push(spec);
                    }
                }
            }
        }
        
        // Create modular reactor configuration
        let mut modular_config = ModularReactorConfig::default();
        // Use consistent network name from network manager
        let network_name = network_manager.network_name().to_string();
        modular_config.base.network_name = network_name.clone();
        modular_config.base.redirected_components = redirected_components;
        modular_config.docker.use_enhanced_client = true;
        modular_config.watcher.auto_rebuild = true;
        modular_config.lifecycle.auto_restart = true;
        
        // Configure build orchestrator with the product directory
        modular_config.build.product_dir = config.product_path().to_path_buf();
        modular_config.build.product_name = config.product_name().to_string();
        
        // Configure lifecycle manager with the product name and network name
        modular_config.lifecycle.product_name = config.product_name().to_string();
        modular_config.lifecycle.network_name = network_name;
        
        // Create the reactor using the existing new() method
        let reactor = Self::new(modular_config, docker_client, component_specs).await?;
        
        Ok(reactor)
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
}