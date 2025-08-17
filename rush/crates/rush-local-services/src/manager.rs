//! Trait-based local service manager
//!
//! This module provides a manager for trait-based local services.

use log::{error, warn};
use rush_output::simple::Sink;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

use crate::error::{Error, Result};
use crate::output::ServiceOutput;
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
    
    /// Output sink for logging
    output_sink: Option<Arc<Mutex<Box<dyn Sink>>>>,
    
    /// Service output handler
    output: ServiceOutput,
}

impl LocalServiceManager {
    /// Create a new LocalServiceManager
    pub fn new() -> Self {
        let output = ServiceOutput::new("LocalServiceManager".to_string());
        Self {
            services: Vec::new(),
            startup_order: Vec::new(),
            env_vars: HashMap::new(),
            env_secrets: HashMap::new(),
            output_sink: None,
            output,
        }
    }
    
    /// Create with an output sink
    pub fn with_output_sink(sink: Arc<Mutex<Box<dyn Sink>>>) -> Self {
        let mut output = ServiceOutput::new("LocalServiceManager".to_string());
        output.set_sink(sink.clone());
        Self {
            services: Vec::new(),
            startup_order: Vec::new(),
            env_vars: HashMap::new(),
            env_secrets: HashMap::new(),
            output_sink: Some(sink),
            output,
        }
    }
    
    /// Register a new service
    pub fn register(&mut self, mut service: Box<dyn LocalService>) {
        let name = service.name().to_string();
        
        // Set the output sink on the service if we have one
        if let Some(ref sink) = self.output_sink {
            service.set_output_sink(sink.clone());
        }
        
        // Log registration using runtime context
        // Only spawn if we're in an async context (check if there's a runtime)
        if tokio::runtime::Handle::try_current().is_ok() {
            let output = self.output.clone();
            tokio::spawn(async move {
                output.info(format!("Registering local service: {}", name)).await;
            });
        }
        
        // Add to startup order (simple for now - could be enhanced with dependency resolution)
        self.startup_order.push(service.name().to_string());
        self.services.push(service);
    }
    
    /// Start all services in order
    pub async fn start_all(&mut self) -> Result<()> {
        self.output.info("Starting all local services").await;
        
        for service in &mut self.services {
            let name = service.name().to_string();
            self.output.info(format!("Starting service: {}", name)).await;
            
            if let Err(e) = service.start().await {
                error!("Failed to start service {}: {}", name, e);
                // Stop already started services
                self.stop_all().await?;
                return Err(Error::Docker(format!("Failed to start {}: {}", name, e)));
            }
            
            // Give the service a moment to stabilize
            sleep(Duration::from_millis(500)).await;
        }
        
        // Don't collect env vars here - they should be collected after wait_for_healthy
        // to ensure services like Stripe have generated their secrets
        
        self.output.info("All local services started successfully").await;
        Ok(())
    }
    
    /// Stop all services
    pub async fn stop_all(&mut self) -> Result<()> {
        self.output.info("Stopping all local services").await;
        
        // Stop in reverse order
        for service in self.services.iter_mut().rev() {
            let name = service.name().to_string();
            self.output.info(format!("Stopping service: {}", name)).await;
            
            if let Err(e) = service.stop().await {
                warn!("Failed to stop service {}: {}", name, e);
                // Continue stopping other services
            }
        }
        
        // Clear environment variables
        self.env_vars.clear();
        self.env_secrets.clear();
        
        self.output.info("All local services stopped").await;
        Ok(())
    }
    
    /// Wait for all services to be healthy
    pub async fn wait_for_healthy(&mut self, timeout_duration: Duration) -> Result<()> {
        self.output.info(format!("Waiting for all services to be healthy (timeout: {:?})", timeout_duration)).await;
        
        let start_time = std::time::Instant::now();
        
        for service in &self.services {
            let name = service.name();
            let remaining = timeout_duration.saturating_sub(start_time.elapsed());
            
            if remaining.is_zero() {
                return Err(Error::Configuration(format!(
                    "Timeout waiting for services to be healthy"
                )));
            }
            
            self.output.info(format!("Waiting for {} to be healthy...", name)).await;
            
            // Wait for this service to be healthy
            let result = timeout(remaining, async {
                loop {
                    match service.is_healthy().await {
                        Ok(true) => {
                            self.output.info(format!("{} is healthy", name)).await;
                            return Ok::<(), Error>(());
                        }
                        Ok(false) => {
                            // Service not healthy yet, waiting...
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
        
        self.output.info("All services are healthy").await;
        
        // Collect environment variables and secrets after all services are healthy
        // This ensures services like Stripe have had time to generate their secrets
        self.collect_env_vars().await?;
        
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
                        // Adding env var from service
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
                        // Adding secret from service
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