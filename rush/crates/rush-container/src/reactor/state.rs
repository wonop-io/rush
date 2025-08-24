//! State management for the Container Reactor
//!
//! This module defines the state structure and transitions for the ContainerReactor,
//! ensuring clear and consistent state management throughout the lifecycle.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use rush_build::ComponentBuildSpec;
use crate::docker::DockerService;

/// The current state of the reactor
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactorPhase {
    /// Initial state, not yet started
    Idle,
    /// Building container images
    Building,
    /// Starting containers
    Starting,
    /// Running and monitoring containers
    Running,
    /// Rebuilding due to file changes
    Rebuilding,
    /// Error state
    Error,
    /// Shutting down containers
    ShuttingDown,
    /// Terminated
    Shutdown,
    /// Terminated (old name for backward compatibility)
    Terminated,
}

/// Status of an individual component
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentStatus {
    /// Component is idle/not started
    Idle,
    /// Component is being built
    Building,
    /// Component is starting
    Starting,
    /// Component is running
    Running,
    /// Component has failed
    Failed,
    /// Component is stopping
    Stopping,
    /// Component is stopped
    Stopped,
}

/// State of an individual component
#[derive(Debug, Clone)]
pub struct ComponentState {
    /// Component name
    pub name: String,
    /// Current status
    pub status: ComponentStatus,
    /// The actual image name (with tag) if built
    pub image_name: Option<String>,
    /// Container ID if running
    pub container_id: Option<String>,
    /// Last build time
    pub last_build: Option<Instant>,
    /// Last error if any
    pub error: Option<String>,
    /// Number of restart attempts
    pub restart_count: u32,
    /// Build specification
    pub build_spec: Option<ComponentBuildSpec>,
}

impl ComponentState {
    /// Create a new component state
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ComponentStatus::Idle,
            image_name: None,
            container_id: None,
            last_build: None,
            error: None,
            restart_count: 0,
            build_spec: None,
        }
    }

    /// Mark component as built
    pub fn mark_built(&mut self, image_name: String) {
        self.status = ComponentStatus::Idle; // Built but not running
        self.image_name = Some(image_name);
        self.last_build = Some(Instant::now());
        self.error = None;
    }

    /// Mark component as running
    pub fn mark_running(&mut self, container_id: String) {
        self.status = ComponentStatus::Running;
        self.container_id = Some(container_id);
        self.error = None;
    }

    /// Mark component as stopped
    pub fn mark_stopped(&mut self) {
        self.status = ComponentStatus::Stopped;
        self.container_id = None;
    }

    /// Mark component as failed
    pub fn mark_failed(&mut self, error: String) {
        self.status = ComponentStatus::Failed;
        self.error = Some(error);
    }

    /// Record an error
    pub fn record_error(&mut self, error: String) {
        self.error = Some(error);
    }

    /// Increment restart count
    pub fn increment_restart(&mut self) {
        self.restart_count += 1;
    }

    /// Reset restart count
    pub fn reset_restart(&mut self) {
        self.restart_count = 0;
    }
}

/// Mutable state for the reactor
#[derive(Debug)]
pub struct ReactorState {
    /// Current phase of the reactor
    phase: ReactorPhase,
    /// State of individual components
    components: HashMap<String, ComponentState>,
    /// Components currently being rebuilt
    rebuilding_components: HashSet<String>,
    /// Running Docker services
    running_services: Vec<DockerService>,
    /// Indicates if a rebuild is in progress
    rebuild_in_progress: bool,
    /// Start time of the reactor
    start_time: Instant,
    /// Number of rebuild cycles
    rebuild_count: u32,
    /// Last error that occurred
    last_error: Option<String>,
}

impl ReactorState {
    /// Create a new reactor state
    pub fn new() -> Self {
        Self {
            phase: ReactorPhase::Idle,
            components: HashMap::new(),
            rebuilding_components: HashSet::new(),
            running_services: Vec::new(),
            rebuild_in_progress: false,
            start_time: Instant::now(),
            rebuild_count: 0,
            last_error: None,
        }
    }

    /// Initialize components from specs
    pub fn init_components(&mut self, specs: &[ComponentBuildSpec]) {
        for spec in specs {
            self.components.insert(
                spec.component_name.clone(),
                ComponentState::new(spec.component_name.clone())
            );
        }
    }

    /// Get the current phase
    pub fn phase(&self) -> &ReactorPhase {
        &self.phase
    }

    /// Transition to a new phase
    pub fn transition_to(&mut self, new_phase: ReactorPhase) -> Result<(), StateError> {
        // Validate transition
        let valid = match (&self.phase, &new_phase) {
            (ReactorPhase::Idle, ReactorPhase::Building) => true,
            (ReactorPhase::Building, ReactorPhase::Starting) => true,
            (ReactorPhase::Building, ReactorPhase::ShuttingDown) => true,
            (ReactorPhase::Starting, ReactorPhase::Running) => true,
            (ReactorPhase::Starting, ReactorPhase::ShuttingDown) => true,
            (ReactorPhase::Running, ReactorPhase::Rebuilding) => true,
            (ReactorPhase::Running, ReactorPhase::ShuttingDown) => true,
            (ReactorPhase::Rebuilding, ReactorPhase::Running) => true,
            (ReactorPhase::Rebuilding, ReactorPhase::ShuttingDown) => true,
            (ReactorPhase::ShuttingDown, ReactorPhase::Terminated) => true,
            _ => false,
        };

        if !valid {
            return Err(StateError::InvalidTransition {
                from: self.phase.clone(),
                to: new_phase,
            });
        }

        log::debug!("Reactor phase transition: {:?} -> {:?}", self.phase, new_phase);
        self.phase = new_phase;
        Ok(())
    }

    /// Mark a component as built
    pub fn mark_component_built(&mut self, name: &str, image_name: String) {
        if let Some(component) = self.components.get_mut(name) {
            component.mark_built(image_name);
        }
    }

    /// Mark a component as running
    pub fn mark_component_running(&mut self, name: &str, container_id: String) {
        if let Some(component) = self.components.get_mut(name) {
            component.mark_running(container_id);
        }
    }

    /// Mark a component as stopped
    pub fn mark_component_stopped(&mut self, name: &str) {
        if let Some(component) = self.components.get_mut(name) {
            component.mark_stopped();
        }
    }

    /// Record a component error
    pub fn record_component_error(&mut self, name: &str, error: String) {
        if let Some(component) = self.components.get_mut(name) {
            component.record_error(error);
        }
    }
    
    /// Add a new component to the state
    pub fn add_component(&mut self, component: ComponentState) {
        self.components.insert(component.name.clone(), component);
    }
    
    /// Record a general error
    pub fn record_error(&mut self, error: String) {
        self.last_error = Some(error);
    }
    
    /// Get last error
    pub fn last_error(&self) -> Option<&String> {
        self.last_error.as_ref()
    }
    
    /// Get running components
    pub fn running_components(&self) -> Vec<&ComponentState> {
        self.components.values().filter(|c| c.status == ComponentStatus::Running).collect()
    }

    /// Get component state
    pub fn get_component(&self, name: &str) -> Option<&ComponentState> {
        self.components.get(name)
    }

    /// Get all components
    pub fn components(&self) -> &HashMap<String, ComponentState> {
        &self.components
    }

    /// Start rebuilding
    pub fn start_rebuild(&mut self, components: Vec<String>) {
        self.rebuild_in_progress = true;
        self.rebuilding_components = components.into_iter().collect();
        self.rebuild_count += 1;
    }

    /// Complete rebuilding
    pub fn complete_rebuild(&mut self) {
        self.rebuild_in_progress = false;
        self.rebuilding_components.clear();
    }

    /// Check if rebuild is in progress
    pub fn is_rebuilding(&self) -> bool {
        self.rebuild_in_progress
    }

    /// Get rebuilding components
    pub fn rebuilding_components(&self) -> &HashSet<String> {
        &self.rebuilding_components
    }

    /// Set running services
    pub fn set_running_services(&mut self, services: Vec<DockerService>) {
        self.running_services = services;
    }

    /// Get running services
    pub fn running_services(&self) -> &[DockerService] {
        &self.running_services
    }

    /// Get uptime
    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Get rebuild count
    pub fn rebuild_count(&self) -> u32 {
        self.rebuild_count
    }

    /// Check if all components are healthy
    pub fn all_healthy(&self) -> bool {
        self.components.values().all(|c| c.status == ComponentStatus::Running && c.error.is_none())
    }

    /// Get unhealthy components
    pub fn unhealthy_components(&self) -> Vec<&ComponentState> {
        self.components
            .values()
            .filter(|c| c.status != ComponentStatus::Running || c.error.is_some())
            .collect()
    }
}

/// Thread-safe wrapper for ReactorState
pub struct SharedReactorState {
    inner: Arc<RwLock<ReactorState>>,
}

impl SharedReactorState {
    /// Create a new shared reactor state
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ReactorState::new())),
        }
    }

    /// Get a read lock on the state
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, ReactorState> {
        self.inner.read().await
    }
    
    /// Try to get a read lock on the state (non-blocking)
    pub fn try_read(&self) -> Result<tokio::sync::RwLockReadGuard<'_, ReactorState>, tokio::sync::TryLockError> {
        self.inner.try_read()
    }

    /// Get a write lock on the state
    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, ReactorState> {
        self.inner.write().await
    }

    /// Clone the Arc for sharing
    pub fn clone_inner(&self) -> Arc<RwLock<ReactorState>> {
        self.inner.clone()
    }
}

impl Clone for SharedReactorState {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Errors related to state management
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: ReactorPhase,
        to: ReactorPhase,
    },
    
    #[error("Component {0} not found")]
    ComponentNotFound(String),
    
    #[error("Invalid operation in phase {0:?}")]
    InvalidOperation(ReactorPhase),
}

impl From<StateError> for rush_core::error::Error {
    fn from(err: StateError) -> Self {
        rush_core::error::Error::Internal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_transitions() {
        let mut state = ReactorState::new();
        
        // Valid transitions
        assert!(state.transition_to(ReactorPhase::Building).is_ok());
        assert_eq!(state.phase(), &ReactorPhase::Building);
        
        assert!(state.transition_to(ReactorPhase::Starting).is_ok());
        assert_eq!(state.phase(), &ReactorPhase::Starting);
        
        assert!(state.transition_to(ReactorPhase::Running).is_ok());
        assert_eq!(state.phase(), &ReactorPhase::Running);
        
        assert!(state.transition_to(ReactorPhase::Rebuilding).is_ok());
        assert_eq!(state.phase(), &ReactorPhase::Rebuilding);
        
        assert!(state.transition_to(ReactorPhase::Running).is_ok());
        assert_eq!(state.phase(), &ReactorPhase::Running);
        
        assert!(state.transition_to(ReactorPhase::ShuttingDown).is_ok());
        assert_eq!(state.phase(), &ReactorPhase::ShuttingDown);
        
        assert!(state.transition_to(ReactorPhase::Terminated).is_ok());
        assert_eq!(state.phase(), &ReactorPhase::Terminated);
    }

    #[test]
    fn test_invalid_transitions() {
        let mut state = ReactorState::new();
        
        // Can't go directly from Idle to Running
        assert!(state.transition_to(ReactorPhase::Running).is_err());
        
        // Can't go backwards from Terminated
        state.phase = ReactorPhase::Terminated;
        assert!(state.transition_to(ReactorPhase::Idle).is_err());
    }

    #[test]
    fn test_component_state_management() {
        let mut component = ComponentState::new("test");
        
        assert_eq!(component.status, ComponentStatus::Idle);
        assert!(component.image_name.is_none());
        
        component.mark_built("test:latest".to_string());
        assert_eq!(component.status, ComponentStatus::Idle); // Built but not running
        assert_eq!(component.image_name, Some("test:latest".to_string()));
        assert!(component.last_build.is_some());
        
        component.mark_running("container123".to_string());
        assert_eq!(component.status, ComponentStatus::Running);
        assert_eq!(component.container_id, Some("container123".to_string()));
        
        component.record_error("Failed to start".to_string());
        assert_eq!(component.error, Some("Failed to start".to_string()));
        
        component.mark_stopped();
        assert_eq!(component.status, ComponentStatus::Stopped);
        assert!(component.container_id.is_none());
    }

    #[test]
    fn test_reactor_state_components() {
        let mut state = ReactorState::new();
        
        // Test component state management without specs
        // We can test the component tracking directly
        state.components.insert(
            "frontend".to_string(),
            ComponentState::new("frontend")
        );
        state.components.insert(
            "backend".to_string(),
            ComponentState::new("backend")
        );
        
        assert_eq!(state.components.len(), 2);
        assert!(state.get_component("frontend").is_some());
        assert!(state.get_component("backend").is_some());
        
        state.mark_component_built("frontend", "frontend:abc123".to_string());
        let frontend = state.get_component("frontend").unwrap();
        assert_eq!(frontend.status, ComponentStatus::Idle); // Built but not running
        assert_eq!(frontend.image_name, Some("frontend:abc123".to_string()));
    }

    #[test]
    fn test_rebuild_tracking() {
        let mut state = ReactorState::new();
        
        assert!(!state.is_rebuilding());
        assert_eq!(state.rebuild_count(), 0);
        
        state.start_rebuild(vec!["frontend".to_string(), "backend".to_string()]);
        assert!(state.is_rebuilding());
        assert_eq!(state.rebuilding_components().len(), 2);
        assert_eq!(state.rebuild_count(), 1);
        
        state.complete_rebuild();
        assert!(!state.is_rebuilding());
        assert_eq!(state.rebuilding_components().len(), 0);
        assert_eq!(state.rebuild_count(), 1);
    }

    #[tokio::test]
    async fn test_shared_state() {
        let shared = SharedReactorState::new();
        
        {
            let mut state = shared.write().await;
            state.transition_to(ReactorPhase::Building).unwrap();
        }
        
        {
            let state = shared.read().await;
            assert_eq!(state.phase(), &ReactorPhase::Building);
        }
        
        // Test cloning
        let shared2 = shared.clone();
        {
            let state = shared2.read().await;
            assert_eq!(state.phase(), &ReactorPhase::Building);
        }
    }
}