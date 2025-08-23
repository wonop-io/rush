//! Plugin architecture for Rush
//!
//! This module provides a plugin system that allows extending Rush with custom
//! functionality without modifying the core codebase.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{Error, Result};

/// Plugin metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Unique plugin identifier
    pub id: String,
    /// Plugin name
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin author
    pub author: String,
    /// Plugin description
    pub description: String,
    /// Required Rush version
    pub rush_version: String,
    /// Plugin dependencies
    pub dependencies: Vec<String>,
}

/// Plugin lifecycle events
#[derive(Debug)]
pub enum PluginEvent {
    /// Plugin is being loaded
    Load,
    /// Plugin is being initialized
    Initialize,
    /// Plugin is being started
    Start,
    /// Plugin is being stopped
    Stop,
    /// Plugin is being unloaded
    Unload,
    /// Custom event with data
    Custom(String, Box<dyn Any + Send + Sync>),
}

/// Plugin capability flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginCapabilities {
    /// Can handle build operations
    pub can_build: bool,
    /// Can handle deployment operations
    pub can_deploy: bool,
    /// Can handle container operations
    pub can_manage_containers: bool,
    /// Can handle configuration
    pub can_configure: bool,
    /// Can handle secrets
    pub can_manage_secrets: bool,
}

impl Default for PluginCapabilities {
    fn default() -> Self {
        Self {
            can_build: false,
            can_deploy: false,
            can_manage_containers: false,
            can_configure: false,
            can_manage_secrets: false,
        }
    }
}

/// Core plugin trait that all plugins must implement
#[async_trait]
pub trait Plugin: Send + Sync + Debug {
    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;
    
    /// Get plugin capabilities
    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::default()
    }
    
    /// Initialize the plugin
    async fn initialize(&mut self, config: PluginConfig) -> Result<()>;
    
    /// Start the plugin
    async fn start(&mut self) -> Result<()>;
    
    /// Stop the plugin
    async fn stop(&mut self) -> Result<()>;
    
    /// Handle a plugin event
    async fn handle_event(&mut self, event: PluginEvent) -> Result<()> {
        match event {
            PluginEvent::Load => self.on_load().await,
            PluginEvent::Initialize => Ok(()),
            PluginEvent::Start => Ok(()),
            PluginEvent::Stop => Ok(()),
            PluginEvent::Unload => self.on_unload().await,
            PluginEvent::Custom(name, data) => self.on_custom_event(&name, data).await,
        }
    }
    
    /// Called when plugin is loaded
    async fn on_load(&mut self) -> Result<()> {
        Ok(())
    }
    
    /// Called when plugin is unloaded
    async fn on_unload(&mut self) -> Result<()> {
        Ok(())
    }
    
    /// Handle custom events
    async fn on_custom_event(
        &mut self,
        _name: &str,
        _data: Box<dyn Any + Send + Sync>,
    ) -> Result<()> {
        Ok(())
    }
}

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Plugin-specific configuration values
    pub values: HashMap<String, serde_json::Value>,
    /// Plugin data directory
    pub data_dir: PathBuf,
    /// Plugin cache directory
    pub cache_dir: PathBuf,
}

/// Build plugin trait for plugins that extend build functionality
#[async_trait]
pub trait BuildPlugin: Plugin {
    /// Check if this plugin can handle the build type
    async fn can_handle_build(&self, build_type: &str) -> bool;
    
    /// Execute a build
    async fn build(
        &mut self,
        build_type: &str,
        context: HashMap<String, String>,
    ) -> Result<()>;
    
    /// Validate build configuration
    async fn validate_build(&self, build_type: &str, config: &serde_json::Value) -> Result<()>;
}

/// Deploy plugin trait for plugins that extend deployment functionality
#[async_trait]
pub trait DeployPlugin: Plugin {
    /// Check if this plugin can handle the deployment target
    async fn can_handle_deploy(&self, target: &str) -> bool;
    
    /// Execute a deployment
    async fn deploy(
        &mut self,
        target: &str,
        context: HashMap<String, String>,
    ) -> Result<()>;
    
    /// Rollback a deployment
    async fn rollback(
        &mut self,
        target: &str,
        version: &str,
    ) -> Result<()>;
}

/// Plugin manager that handles plugin lifecycle
pub struct PluginManager {
    /// Registered plugins
    plugins: Arc<RwLock<HashMap<String, Box<dyn Plugin>>>>,
    /// Plugin configurations
    configs: Arc<RwLock<HashMap<String, PluginConfig>>>,
    /// Plugin load order for dependency resolution
    load_order: Arc<RwLock<Vec<String>>>,
}

impl PluginManager {
    /// Create a new plugin manager
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
            load_order: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Register a plugin
    pub async fn register(&self, mut plugin: Box<dyn Plugin>) -> Result<()> {
        let metadata = plugin.metadata();
        let id = metadata.id.clone();
        
        // Check if plugin already exists
        let plugins = self.plugins.read().await;
        if plugins.contains_key(&id) {
            return Err(Error::Internal(format!("Plugin {} already registered", id)));
        }
        drop(plugins);
        
        // Initialize plugin with default config
        let config = PluginConfig {
            values: HashMap::new(),
            data_dir: PathBuf::from(format!(".rush/plugins/{}/data", id)),
            cache_dir: PathBuf::from(format!(".rush/plugins/{}/cache", id)),
        };
        
        plugin.initialize(config.clone()).await?;
        
        // Store plugin and config
        let mut plugins = self.plugins.write().await;
        let mut configs = self.configs.write().await;
        let mut load_order = self.load_order.write().await;
        
        plugins.insert(id.clone(), plugin);
        configs.insert(id.clone(), config);
        load_order.push(id.clone());
        
        log::info!("Registered plugin: {}", id);
        Ok(())
    }
    
    /// Unregister a plugin
    pub async fn unregister(&self, id: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        
        if let Some(mut plugin) = plugins.remove(id) {
            plugin.stop().await?;
            plugin.handle_event(PluginEvent::Unload).await?;
            
            let mut configs = self.configs.write().await;
            let mut load_order = self.load_order.write().await;
            
            configs.remove(id);
            load_order.retain(|pid| pid != id);
            
            log::info!("Unregistered plugin: {}", id);
            Ok(())
        } else {
            Err(Error::Internal(format!("Plugin {} not found", id)))
        }
    }
    
    /// Start all plugins
    pub async fn start_all(&self) -> Result<()> {
        let load_order = self.load_order.read().await;
        let mut plugins = self.plugins.write().await;
        
        for id in load_order.iter() {
            if let Some(plugin) = plugins.get_mut(id) {
                plugin.start().await?;
                log::info!("Started plugin: {}", id);
            }
        }
        
        Ok(())
    }
    
    /// Stop all plugins
    pub async fn stop_all(&self) -> Result<()> {
        let load_order = self.load_order.read().await;
        let mut plugins = self.plugins.write().await;
        
        // Stop in reverse order
        for id in load_order.iter().rev() {
            if let Some(plugin) = plugins.get_mut(id) {
                plugin.stop().await?;
                log::info!("Stopped plugin: {}", id);
            }
        }
        
        Ok(())
    }
    
    /// Send an event to a specific plugin
    pub async fn send_event(&self, plugin_id: &str, event: PluginEvent) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        
        if let Some(plugin) = plugins.get_mut(plugin_id) {
            plugin.handle_event(event).await?;
        } else {
            return Err(Error::Internal(format!("Plugin {} not found", plugin_id)));
        }
        
        Ok(())
    }
    
    /// Broadcast a simple event to all plugins (non-custom events only)
    pub async fn broadcast_simple_event(&self, event_type: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        
        for (id, plugin) in plugins.iter_mut() {
            let event = match event_type {
                "load" => PluginEvent::Load,
                "initialize" => PluginEvent::Initialize,
                "start" => PluginEvent::Start,
                "stop" => PluginEvent::Stop,
                "unload" => PluginEvent::Unload,
                _ => {
                    log::warn!("Unknown event type: {}", event_type);
                    continue;
                }
            };
            
            if let Err(e) = plugin.handle_event(event).await {
                log::warn!("Plugin {} failed to handle event: {}", id, e);
            }
        }
        
        Ok(())
    }
    
    /// Check if a plugin is registered
    pub async fn has_plugin(&self, id: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins.contains_key(id)
    }
    
    /// Execute a function with a plugin reference
    pub async fn with_plugin<F, R>(&self, id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&dyn Plugin) -> R,
    {
        let plugins = self.plugins.read().await;
        plugins.get(id).map(|p| f(p.as_ref()))
    }
    
    /// List all registered plugins
    pub async fn list_plugins(&self) -> Vec<PluginMetadata> {
        let plugins = self.plugins.read().await;
        plugins
            .values()
            .map(|p| p.metadata().clone())
            .collect()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global plugin manager instance
static PLUGIN_MANAGER: once_cell::sync::Lazy<PluginManager> = 
    once_cell::sync::Lazy::new(PluginManager::new);

/// Get the global plugin manager
pub fn plugin_manager() -> &'static PluginManager {
    &PLUGIN_MANAGER
}

// Example plugin implementation

/// Example plugin that logs events
#[derive(Debug)]
pub struct LoggingPlugin {
    metadata: PluginMetadata,
}

impl LoggingPlugin {
    pub fn new() -> Self {
        Self {
            metadata: PluginMetadata {
                id: "logging".to_string(),
                name: "Logging Plugin".to_string(),
                version: "1.0.0".to_string(),
                author: "Rush Team".to_string(),
                description: "Logs all plugin events".to_string(),
                rush_version: "0.1.0".to_string(),
                dependencies: vec![],
            },
        }
    }
}

#[async_trait]
impl Plugin for LoggingPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }
    
    async fn initialize(&mut self, _config: PluginConfig) -> Result<()> {
        log::info!("Logging plugin initialized");
        Ok(())
    }
    
    async fn start(&mut self) -> Result<()> {
        log::info!("Logging plugin started");
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<()> {
        log::info!("Logging plugin stopped");
        Ok(())
    }
    
    async fn handle_event(&mut self, event: PluginEvent) -> Result<()> {
        log::info!("Logging plugin received event: {:?}", event);
        Ok(())
    }
}