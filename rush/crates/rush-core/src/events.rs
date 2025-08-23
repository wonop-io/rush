//! Event-driven architecture support
//!
//! This module provides a simple event bus for decoupled communication
//! between components.

use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// System-wide events
#[derive(Debug, Clone)]
pub enum SystemEvent {
    /// Build started for a component
    BuildStarted { component: String },
    
    /// Build completed for a component
    BuildCompleted { 
        component: String, 
        success: bool 
    },
    
    /// Container started
    ContainerStarted { 
        id: String,
        component: String 
    },
    
    /// Container stopped
    ContainerStopped { 
        id: String,
        component: String,
        reason: StopReason 
    },
    
    /// File changed
    FileChanged { 
        path: std::path::PathBuf 
    },
    
    /// Configuration reloaded
    ConfigurationReloaded,
    
    /// Health check status changed
    HealthCheckStatusChanged {
        component: String,
        healthy: bool
    },
    
    /// Deployment started
    DeploymentStarted {
        environment: String
    },
    
    /// Deployment completed
    DeploymentCompleted {
        environment: String,
        success: bool
    },
}

/// Reason for container stop
#[derive(Debug, Clone)]
pub enum StopReason {
    /// User requested stop
    UserRequested,
    /// Container crashed
    Crashed(i32),
    /// Health check failed
    HealthCheckFailed,
    /// Dependency failed
    DependencyFailed,
    /// System shutdown
    SystemShutdown,
}

/// Event handler trait
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an event
    async fn handle(&self, event: &SystemEvent) -> crate::Result<()>;
    
    /// Get the event types this handler is interested in
    fn event_types(&self) -> Vec<TypeId> {
        vec![TypeId::of::<SystemEvent>()]
    }
}

/// Event bus for publishing and subscribing to events
pub struct EventBus {
    handlers: Arc<RwLock<Vec<Arc<dyn EventHandler>>>>,
    typed_handlers: Arc<RwLock<HashMap<TypeId, Vec<Arc<dyn EventHandler>>>>>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(Vec::new())),
            typed_handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Register an event handler
    pub async fn register_handler(&self, handler: Arc<dyn EventHandler>) {
        let mut handlers = self.handlers.write().await;
        handlers.push(handler.clone());
        
        // Also register by type for efficient routing
        let mut typed = self.typed_handlers.write().await;
        for type_id in handler.event_types() {
            typed.entry(type_id)
                .or_insert_with(Vec::new)
                .push(handler.clone());
        }
    }
    
    /// Publish an event to all registered handlers
    pub async fn publish(&self, event: SystemEvent) -> crate::Result<()> {
        let handlers = self.handlers.read().await;
        
        for handler in handlers.iter() {
            if let Err(e) = handler.handle(&event).await {
                log::warn!("Event handler error: {}", e);
                // Continue processing other handlers even if one fails
            }
        }
        
        Ok(())
    }
    
    /// Remove all handlers
    pub async fn clear(&self) {
        let mut handlers = self.handlers.write().await;
        handlers.clear();
        
        let mut typed = self.typed_handlers.write().await;
        typed.clear();
    }
    
    /// Get the number of registered handlers
    pub async fn handler_count(&self) -> usize {
        let handlers = self.handlers.read().await;
        handlers.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Global event bus instance
static GLOBAL_EVENT_BUS: once_cell::sync::Lazy<EventBus> = 
    once_cell::sync::Lazy::new(EventBus::new);

/// Get the global event bus
pub fn global_event_bus() -> &'static EventBus {
    &GLOBAL_EVENT_BUS
}

/// Convenience function to publish an event to the global bus
pub async fn publish_event(event: SystemEvent) -> crate::Result<()> {
    global_event_bus().publish(event).await
}

// Example event handler implementations

/// Logger event handler that logs all events
pub struct LoggingEventHandler {
    level: log::Level,
}

impl LoggingEventHandler {
    pub fn new(level: log::Level) -> Self {
        Self { level }
    }
}

#[async_trait::async_trait]
impl EventHandler for LoggingEventHandler {
    async fn handle(&self, event: &SystemEvent) -> crate::Result<()> {
        log::log!(self.level, "Event: {:?}", event);
        Ok(())
    }
}

/// Metrics event handler that collects metrics
pub struct MetricsEventHandler {
    build_count: Arc<RwLock<u64>>,
    container_count: Arc<RwLock<u64>>,
}

impl MetricsEventHandler {
    pub fn new() -> Self {
        Self {
            build_count: Arc::new(RwLock::new(0)),
            container_count: Arc::new(RwLock::new(0)),
        }
    }
    
    pub async fn get_build_count(&self) -> u64 {
        *self.build_count.read().await
    }
    
    pub async fn get_container_count(&self) -> u64 {
        *self.container_count.read().await
    }
}

#[async_trait::async_trait]
impl EventHandler for MetricsEventHandler {
    async fn handle(&self, event: &SystemEvent) -> crate::Result<()> {
        match event {
            SystemEvent::BuildCompleted { success: true, .. } => {
                let mut count = self.build_count.write().await;
                *count += 1;
            }
            SystemEvent::ContainerStarted { .. } => {
                let mut count = self.container_count.write().await;
                *count += 1;
            }
            _ => {}
        }
        Ok(())
    }
}