//! Trait-based local service manager
//!
//! This module provides a manager for trait-based local services.

use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::{sleep, timeout};

use crate::error::{Error, Result};

use crate::r#trait::LocalService;

/// Manages a collection of local services
pub struct LocalServiceManager {
    /// Collection of local services
    services: Vec<Box<dyn LocalService>>,
    
    /// Service startup order based on dependencies
    startup_order: Vec<String>,
    
    /// Aggregated environment variables from all services
    env_vars: HashMap<String, String>,
    
    /// Aggregated secrets from all services
    env_secrets: HashMap<String, String>,
}

impl LocalServiceManager {
    /// Create a new LocalServiceManager
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            startup_order: Vec::new(),
            env_vars: HashMap::new(),
            env_secrets: HashMap::new(),
        }
    }
    
    /// Register a new service
    pub fn register(&mut self, service: Box<dyn LocalService>) {
        let name = service.name().to_string();
        info!("Registering local service: {}", name);
        
        // Add to startup order (simple for now - could be enhanced with dependency resolution)
        self.startup_order.push(name.clone());
        self.services.push(service);
    }
    
    /// Start all services in order
    pub async fn start_all(&mut self) -> Result<()> {
        info!("Starting all local services");
        
        for service in &mut self.services {
            let name = service.name().to_string();
            info!("Starting service: {}", name);
            
            if let Err(e) = service.start().await {
                error!("Failed to start service {}: {}", name, e);
                // Stop already started services
                self.stop_all().await?;
                return Err(Error::Docker(format!("Failed to start {}: {}", name, e)));
            }
            
            // Give the service a moment to stabilize
            sleep(Duration::from_millis(500)).await;
        }
        
        // Collect environment variables and secrets
        self.collect_env_vars().await?;
        
        info!("All local services started successfully");
        Ok(())
    }
    
    /// Stop all services
    pub async fn stop_all(&mut self) -> Result<()> {
        info!("Stopping all local services");
        
        // Stop in reverse order
        for service in self.services.iter_mut().rev() {
            let name = service.name().to_string();
            info!("Stopping service: {}", name);
            
            if let Err(e) = service.stop().await {
                warn!("Failed to stop service {}: {}", name, e);
                // Continue stopping other services
            }
        }
        
        // Clear environment variables
        self.env_vars.clear();
        self.env_secrets.clear();
        
        info!("All local services stopped");
        Ok(())
    }
    
    /// Wait for all services to be healthy
    pub async fn wait_for_healthy(&self, timeout_duration: Duration) -> Result<()> {
        info!("Waiting for all services to be healthy (timeout: {:?})", timeout_duration);
        
        let start_time = std::time::Instant::now();
        
        for service in &self.services {
            let name = service.name();
            let remaining = timeout_duration.saturating_sub(start_time.elapsed());
            
            if remaining.is_zero() {
                return Err(Error::Configuration(format!(
                    "Timeout waiting for services to be healthy"
                )));
            }
            
            info!("Waiting for {} to be healthy...", name);
            
            // Wait for this service to be healthy
            let result = timeout(remaining, async {
                loop {
                    match service.is_healthy().await {
                        Ok(true) => {
                            info!("{} is healthy", name);
                            return Ok::<(), Error>(());
                        }
                        Ok(false) => {
                            debug!("{} is not healthy yet, waiting...", name);
                            sleep(Duration::from_secs(1)).await;
                        }
                        Err(e) => {
                            warn!("Health check failed for {}: {}", name, e);
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }).await;
            
            match result {
                Ok(Ok(())) => continue,
                Ok(Err(e)) => return Err(Error::Docker(format!("Health check error: {}", e))),
                Err(_) => {
                    return Err(Error::Configuration(format!(
                        "Timeout waiting for {} to be healthy",
                        name
                    )));
                }
            }
        }
        
        info!("All services are healthy");
        Ok(())
    }
    
    /// Collect environment variables and secrets from all services
    async fn collect_env_vars(&mut self) -> Result<()> {
        self.env_vars.clear();
        self.env_secrets.clear();
        
        for service in &self.services {
            // Collect regular environment variables
            match service.generated_env_vars().await {
                Ok(vars) => {
                    for (key, value) in vars {
                        debug!("Adding env var from {}: {}=...", service.name(), key);
                        self.env_vars.insert(key, value);
                    }
                }
                Err(e) => {
                    warn!("Failed to get env vars from {}: {}", service.name(), e);
                }
            }
            
            // Collect secrets
            match service.generated_env_secrets().await {
                Ok(secrets) => {
                    for (key, value) in secrets {
                        debug!("Adding secret from {}: {}=...", service.name(), key);
                        self.env_secrets.insert(key, value);
                    }
                }
                Err(e) => {
                    warn!("Failed to get secrets from {}: {}", service.name(), e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Get aggregated environment variables from all services
    pub fn get_env_vars(&self) -> &HashMap<String, String> {
        &self.env_vars
    }
    
    /// Get aggregated secrets from all services
    pub fn get_env_secrets(&self) -> &HashMap<String, String> {
        &self.env_secrets
    }
    
    /// Get status of all services
    pub async fn get_status(&self) -> Vec<(String, bool)> {
        let mut status = Vec::new();
        
        for service in &self.services {
            let name = service.name().to_string();
            let healthy = service.is_healthy().await.unwrap_or(false);
            status.push((name, healthy));
        }
        
        status
    }
    
    /// Check if a specific service is running
    pub fn is_service_running(&self, name: &str) -> bool {
        self.services
            .iter()
            .find(|s| s.name() == name)
            .map(|s| s.is_running())
            .unwrap_or(false)
    }
}