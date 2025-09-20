//! Health check configuration for container lifecycle management
//!
//! This module defines health check types and configurations that allow
//! Rush to verify that containers are ready before starting dependent services.

use serde::{Deserialize, Serialize};

/// Configuration for health checks and startup probes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthCheckConfig {
    /// Type of health check to perform
    #[serde(rename = "type")]
    pub check_type: HealthCheckType,

    /// Initial delay before first check (seconds)
    #[serde(default = "default_initial_delay")]
    pub initial_delay: u32,

    /// Interval between checks (seconds)
    #[serde(default = "default_interval")]
    pub interval: u32,

    /// Number of consecutive successes required to be considered healthy
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,

    /// Number of consecutive failures required to be considered unhealthy
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Timeout for each check (seconds)
    #[serde(default = "default_timeout")]
    pub timeout: u32,

    /// Maximum number of retries before giving up
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

/// Types of health checks supported
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HealthCheckType {
    /// HTTP GET request to a path
    #[serde(rename = "http")]
    Http {
        /// Path to check (e.g., "/health")
        path: String,
        /// Expected HTTP status code (default: 200)
        #[serde(default = "default_http_status")]
        expected_status: u16,
    },

    /// TCP port connectivity check
    #[serde(rename = "tcp")]
    Tcp {
        /// Port to check
        port: u16,
    },

    /// Execute command in container
    #[serde(rename = "exec")]
    Exec {
        /// Command to execute (must exit with 0 for success)
        command: Vec<String>,
    },

    /// DNS resolution check (useful for ingress)
    #[serde(rename = "dns")]
    Dns {
        /// List of hostnames that must resolve
        hosts: Vec<String>,
    },
}

// Default values for health check configuration
fn default_initial_delay() -> u32 { 0 }
fn default_interval() -> u32 { 10 }
fn default_success_threshold() -> u32 { 1 }
fn default_failure_threshold() -> u32 { 3 }
fn default_timeout() -> u32 { 5 }
fn default_max_retries() -> u32 { 30 }
fn default_http_status() -> u16 { 200 }

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            check_type: HealthCheckType::Tcp { port: 80 },
            initial_delay: default_initial_delay(),
            interval: default_interval(),
            success_threshold: default_success_threshold(),
            failure_threshold: default_failure_threshold(),
            timeout: default_timeout(),
            max_retries: default_max_retries(),
        }
    }
}

impl HealthCheckConfig {
    /// Create a new HTTP health check configuration
    pub fn http(path: impl Into<String>) -> Self {
        Self {
            check_type: HealthCheckType::Http {
                path: path.into(),
                expected_status: 200,
            },
            ..Default::default()
        }
    }

    /// Create a new TCP health check configuration
    pub fn tcp(port: u16) -> Self {
        Self {
            check_type: HealthCheckType::Tcp { port },
            ..Default::default()
        }
    }

    /// Create a new exec health check configuration
    pub fn exec(command: Vec<String>) -> Self {
        Self {
            check_type: HealthCheckType::Exec { command },
            ..Default::default()
        }
    }

    /// Create a new DNS health check configuration
    pub fn dns(hosts: Vec<String>) -> Self {
        Self {
            check_type: HealthCheckType::Dns { hosts },
            ..Default::default()
        }
    }

    /// Set the initial delay before first check
    pub fn with_initial_delay(mut self, seconds: u32) -> Self {
        self.initial_delay = seconds;
        self
    }

    /// Set the interval between checks
    pub fn with_interval(mut self, seconds: u32) -> Self {
        self.interval = seconds;
        self
    }

    /// Set the success threshold
    pub fn with_success_threshold(mut self, threshold: u32) -> Self {
        self.success_threshold = threshold;
        self
    }

    /// Set the failure threshold
    pub fn with_failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold;
        self
    }

    /// Set the timeout for each check
    pub fn with_timeout(mut self, seconds: u32) -> Self {
        self.timeout = seconds;
        self
    }

    /// Set the maximum retries
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }
}

/// Parse health check configuration from YAML value
pub fn parse_health_check(value: &serde_yaml::Value) -> Option<HealthCheckConfig> {
    if value.is_null() {
        return None;
    }

    // Handle simplified syntax for common cases
    if let Some(type_str) = value.get("type").and_then(|v| v.as_str()) {
        match type_str {
            "http" => {
                let path = value.get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("/health")
                    .to_string();
                let expected_status = value.get("expected_status")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(200) as u16;

                let mut config = HealthCheckConfig::http(path);
                if let HealthCheckType::Http { expected_status: ref mut status, .. } = config.check_type {
                    *status = expected_status;
                }

                parse_common_fields(value, config)
            }
            "tcp" => {
                let port = value.get("port")
                    .and_then(|v| v.as_u64())
                    .expect("port is required for tcp health check") as u16;

                let config = HealthCheckConfig::tcp(port);
                parse_common_fields(value, config)
            }
            "exec" => {
                let command = value.get("command")
                    .and_then(|v| v.as_sequence())
                    .expect("command is required for exec health check")
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                let config = HealthCheckConfig::exec(command);
                parse_common_fields(value, config)
            }
            "dns" => {
                let hosts = value.get("hosts")
                    .and_then(|v| v.as_sequence())
                    .expect("hosts is required for dns health check")
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                let config = HealthCheckConfig::dns(hosts);
                parse_common_fields(value, config)
            }
            _ => None,
        }
    } else {
        None
    }
}

/// Parse common health check fields from YAML
fn parse_common_fields(value: &serde_yaml::Value, mut config: HealthCheckConfig) -> Option<HealthCheckConfig> {
    if let Some(initial_delay) = value.get("initial_delay").and_then(|v| v.as_u64()) {
        config.initial_delay = initial_delay as u32;
    }
    if let Some(interval) = value.get("interval").and_then(|v| v.as_u64()) {
        config.interval = interval as u32;
    }
    if let Some(success_threshold) = value.get("success_threshold").and_then(|v| v.as_u64()) {
        config.success_threshold = success_threshold as u32;
    }
    if let Some(failure_threshold) = value.get("failure_threshold").and_then(|v| v.as_u64()) {
        config.failure_threshold = failure_threshold as u32;
    }
    if let Some(timeout) = value.get("timeout").and_then(|v| v.as_u64()) {
        config.timeout = timeout as u32;
    }
    if let Some(max_retries) = value.get("max_retries").and_then(|v| v.as_u64()) {
        config.max_retries = max_retries as u32;
    }

    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check_defaults() {
        let config = HealthCheckConfig::default();
        assert_eq!(config.initial_delay, 0);
        assert_eq!(config.interval, 10);
        assert_eq!(config.success_threshold, 1);
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.timeout, 5);
        assert_eq!(config.max_retries, 30);
    }

    #[test]
    fn test_http_health_check() {
        let config = HealthCheckConfig::http("/health")
            .with_initial_delay(5)
            .with_interval(3);

        assert_eq!(config.initial_delay, 5);
        assert_eq!(config.interval, 3);

        if let HealthCheckType::Http { path, expected_status } = config.check_type {
            assert_eq!(path, "/health");
            assert_eq!(expected_status, 200);
        } else {
            panic!("Expected HTTP health check type");
        }
    }

    #[test]
    fn test_tcp_health_check() {
        let config = HealthCheckConfig::tcp(8080)
            .with_max_retries(60);

        assert_eq!(config.max_retries, 60);

        if let HealthCheckType::Tcp { port } = config.check_type {
            assert_eq!(port, 8080);
        } else {
            panic!("Expected TCP health check type");
        }
    }

    #[test]
    fn test_dns_health_check() {
        let config = HealthCheckConfig::dns(vec![
            "backend.docker".to_string(),
            "frontend.docker".to_string(),
        ]);

        if let HealthCheckType::Dns { hosts } = config.check_type {
            assert_eq!(hosts.len(), 2);
            assert_eq!(hosts[0], "backend.docker");
            assert_eq!(hosts[1], "frontend.docker");
        } else {
            panic!("Expected DNS health check type");
        }
    }
}