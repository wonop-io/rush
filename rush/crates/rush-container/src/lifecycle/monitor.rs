//! Container health monitoring
//!
//! This module provides health checking and monitoring capabilities
//! for running containers.

use crate::{
    docker::{DockerClient, DockerService, ContainerStatus},
    events::{Event, EventBus, ContainerEvent},
    reactor::state::SharedReactorState,
};
use rush_core::error::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use log::{debug, info, warn, error};
use tokio::time::interval;

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    /// Interval between health checks
    pub check_interval: Duration,
    /// Timeout for health check operations
    pub timeout: Duration,
    /// Number of consecutive failures before marking unhealthy
    pub failure_threshold: u32,
    /// Number of consecutive successes before marking healthy
    pub success_threshold: u32,
    /// Enable detailed health logging
    pub verbose: bool,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(5),
            timeout: Duration::from_secs(10),
            failure_threshold: 3,
            success_threshold: 2,
            verbose: false,
        }
    }
}

/// Health status of a container
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    /// Container is healthy
    Healthy,
    /// Container is unhealthy
    Unhealthy(String),
    /// Container is starting up
    Starting,
    /// Health status is unknown
    Unknown,
}

/// Health check result
#[derive(Debug)]
struct HealthCheckResult {
    container_id: String,
    component: String,
    status: HealthStatus,
    timestamp: Instant,
}

/// Container health monitor
pub struct HealthMonitor {
    config: HealthCheckConfig,
    docker_client: Arc<dyn DockerClient>,
    event_bus: EventBus,
    state: SharedReactorState,
    /// Track consecutive failures/successes per container
    health_history: HashMap<String, (u32, u32)>, // (failures, successes)
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(
        config: HealthCheckConfig,
        docker_client: Arc<dyn DockerClient>,
        event_bus: EventBus,
        state: SharedReactorState,
    ) -> Self {
        Self {
            config,
            docker_client,
            event_bus,
            state,
            health_history: HashMap::new(),
        }
    }

    /// Monitor container health
    pub async fn monitor(&mut self, services: &[DockerService]) -> Result<()> {
        info!("Starting health monitoring for {} services", services.len());
        
        let mut check_interval = interval(self.config.check_interval);
        
        loop {
            check_interval.tick().await;
            
            // Perform health checks on all services
            let results = self.check_all_services(services).await;
            
            // Process results and update state
            for result in results {
                self.process_health_result(result).await?;
            }
            
            // Check if all services are healthy
            let all_healthy = {
                let state = self.state.read().await;
                state.all_healthy()
            };
            
            if self.config.verbose {
                if all_healthy {
                    debug!("All services are healthy");
                } else {
                    let state = self.state.read().await;
                    let unhealthy = state.unhealthy_components();
                    warn!("Unhealthy services: {:?}", 
                        unhealthy.iter().map(|c| &c.name).collect::<Vec<_>>());
                }
            }
        }
    }

    /// Check health of all services
    async fn check_all_services(&self, services: &[DockerService]) -> Vec<HealthCheckResult> {
        let mut results = Vec::new();
        
        for service in services {
            let result = self.check_service_health(service).await;
            results.push(result);
        }
        
        results
    }

    /// Check health of a single service
    async fn check_service_health(&self, service: &DockerService) -> HealthCheckResult {
        let container_id = service.id().to_string();
        let component = service.name().unwrap_or_else(|| "unknown".to_string());
        
        // Check container status
        let status = match self.docker_client.container_status(service.id()).await {
            Ok(ContainerStatus::Running) => {
                // Container is running, check if it has health checks
                match self.check_container_health(&container_id).await {
                    Ok(healthy) => {
                        if healthy {
                            HealthStatus::Healthy
                        } else {
                            HealthStatus::Unhealthy("Health check failed".to_string())
                        }
                    }
                    Err(e) => {
                        debug!("Health check error for {}: {}", component, e);
                        HealthStatus::Unknown
                    }
                }
            }
            Ok(ContainerStatus::Created) => {
                HealthStatus::Starting
            }
            Ok(ContainerStatus::Restarting) => {
                HealthStatus::Starting
            }
            Ok(ContainerStatus::Exited(code)) => {
                HealthStatus::Unhealthy(format!("Container exited with code {}", code))
            }
            Ok(ContainerStatus::Paused) => {
                HealthStatus::Unhealthy("Container is paused".to_string())
            }
            Ok(ContainerStatus::Dead) => {
                HealthStatus::Unhealthy("Container is dead".to_string())
            }
            Ok(ContainerStatus::Unknown) => {
                HealthStatus::Unknown
            }
            Err(e) => {
                warn!("Failed to get status for {}: {}", component, e);
                HealthStatus::Unknown
            }
        };
        
        HealthCheckResult {
            container_id,
            component,
            status,
            timestamp: Instant::now(),
        }
    }

    /// Check container health using Docker health check
    async fn check_container_health(&self, container_id: &str) -> Result<bool> {
        // This is a simplified health check
        // In a real implementation, you'd use Docker's health check API
        // or execute a health check command in the container
        
        // For now, just check if we can get container logs
        match tokio::time::timeout(
            self.config.timeout,
            self.docker_client.container_logs(container_id, 1)
        ).await {
            Ok(Ok(_)) => Ok(true),
            Ok(Err(e)) => {
                debug!("Health check failed: {}", e);
                Ok(false)
            }
            Err(_) => {
                debug!("Health check timed out");
                Ok(false)
            }
        }
    }

    /// Process a health check result
    async fn process_health_result(&mut self, result: HealthCheckResult) -> Result<()> {
        let (failures, successes) = self.health_history
            .entry(result.container_id.clone())
            .or_insert((0, 0));
        
        let previous_healthy = *failures < self.config.failure_threshold;
        let mut currently_healthy = false;
        
        match result.status {
            HealthStatus::Healthy => {
                *failures = 0;
                *successes += 1;
                
                if *successes >= self.config.success_threshold {
                    currently_healthy = true;
                    
                    if !previous_healthy {
                        info!("{} is now healthy", result.component);
                        
                        // Publish health change event
                        if let Err(e) = self.event_bus.publish(Event::new(
                            "monitor",
                            ContainerEvent::ContainerHealthChanged {
                                component: result.component.clone(),
                                container_id: result.container_id.clone(),
                                healthy: true,
                            },
                        )).await {
                            warn!("Failed to publish health change event: {}", e);
                        }
                    }
                }
            }
            HealthStatus::Unhealthy(reason) => {
                *successes = 0;
                *failures += 1;
                
                if *failures >= self.config.failure_threshold && previous_healthy {
                    error!("{} is unhealthy: {}", result.component, reason);
                    
                    // Update state
                    {
                        let mut state = self.state.write().await;
                        state.record_component_error(&result.component, reason.clone());
                    }
                    
                    // Publish health change event
                    if let Err(e) = self.event_bus.publish(Event::new(
                        "monitor",
                        ContainerEvent::ContainerHealthChanged {
                            component: result.component.clone(),
                            container_id: result.container_id.clone(),
                            healthy: false,
                        },
                    )).await {
                        warn!("Failed to publish health change event: {}", e);
                    }
                }
            }
            HealthStatus::Starting => {
                // Reset counters for starting containers
                *failures = 0;
                *successes = 0;
                debug!("{} is starting", result.component);
            }
            HealthStatus::Unknown => {
                // Don't change counters for unknown status
                debug!("{} health status unknown", result.component);
            }
        }
        
        Ok(())
    }

    /// Get current health status for all monitored containers
    pub fn get_health_status(&self) -> HashMap<String, HealthStatus> {
        let mut status_map = HashMap::new();
        
        for (container_id, (failures, successes)) in &self.health_history {
            let status = if *failures >= self.config.failure_threshold {
                HealthStatus::Unhealthy("Threshold exceeded".to_string())
            } else if *successes >= self.config.success_threshold {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unknown
            };
            
            status_map.insert(container_id.clone(), status);
        }
        
        status_map
    }

    /// Reset health history for a container
    pub fn reset_container(&mut self, container_id: &str) {
        self.health_history.remove(container_id);
    }

    /// Clear all health history
    pub fn clear(&mut self) {
        self.health_history.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_health_check_config_default() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(5));
        assert_eq!(config.timeout, Duration::from_secs(10));
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.success_threshold, 2);
        assert!(!config.verbose);
    }
    
    #[test]
    fn test_health_status_equality() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_ne!(HealthStatus::Healthy, HealthStatus::Unknown);
        assert_eq!(
            HealthStatus::Unhealthy("test".to_string()),
            HealthStatus::Unhealthy("test".to_string())
        );
    }
}