use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Command to run for health check
    pub command: String,
    
    /// Interval between health checks
    pub interval: Duration,
    
    /// Number of retries before considering unhealthy
    pub retries: u32,
    
    /// Timeout for each health check
    pub timeout: Duration,
    
    /// Start period (grace period before starting health checks)
    pub start_period: Duration,
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self {
            command: String::new(),
            interval: Duration::from_secs(5),
            retries: 10,
            timeout: Duration::from_secs(3),
            start_period: Duration::from_secs(10),
        }
    }
}

impl HealthCheck {
    pub fn new(command: String) -> Self {
        Self {
            command,
            ..Default::default()
        }
    }
}

/// Health status of a service
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    /// Service is starting up
    Starting,
    
    /// Service is healthy and ready
    Healthy,
    
    /// Service is unhealthy
    Unhealthy(String),
    
    /// Service is not running
    NotRunning,
    
    /// Health check not configured
    Unknown,
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }
    
    pub fn is_running(&self) -> bool {
        !matches!(self, HealthStatus::NotRunning)
    }
}