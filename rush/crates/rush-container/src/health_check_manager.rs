//! Health check manager for container readiness verification
//!
//! This module provides the implementation for executing health checks
//! within running containers and determining when they are ready to
//! receive traffic or serve as dependencies for other containers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, info, trace, warn};
use rush_build::{HealthCheckConfig, HealthCheckType};
use rush_core::error::{Error, Result};
use tokio::time::sleep;

use crate::docker::DockerClient;

/// Result of a health check attempt
#[derive(Debug, Clone, PartialEq)]
pub enum HealthCheckResult {
    /// Check passed successfully
    Healthy,
    /// Check failed but should retry
    Unhealthy(String),
    /// Check failed and should not retry
    Fatal(String),
}

/// Manager for executing health checks on containers
pub struct HealthCheckManager {
    /// Docker client for executing commands
    docker_client: Arc<dyn DockerClient>,
}

impl HealthCheckManager {
    /// Create a new health check manager
    pub fn new(docker_client: Arc<dyn DockerClient>) -> Self {
        Self { docker_client }
    }

    /// Wait for a container to become healthy according to its health check configuration
    pub async fn wait_for_healthy(
        &self,
        container_id: &str,
        component_name: &str,
        config: &HealthCheckConfig,
    ) -> Result<()> {
        info!("🏥 Starting health checks for {component_name}");

        // Apply initial delay if configured
        if config.initial_delay > 0 {
            info!(
                "⏱️  {} waiting {}s before first health check",
                component_name, config.initial_delay
            );
            sleep(Duration::from_secs(config.initial_delay as u64)).await;
        }

        let start_time = Instant::now();
        let mut consecutive_successes = 0u32;
        let mut consecutive_failures = 0u32;
        let mut total_attempts = 0u32;

        loop {
            total_attempts += 1;
            trace!(
                "Health check attempt {}/{} for {}",
                total_attempts,
                config.max_retries,
                component_name
            );

            // Perform the health check
            let result = self
                .perform_health_check(container_id, &config.check_type, config.timeout)
                .await;

            match result {
                HealthCheckResult::Healthy => {
                    consecutive_successes += 1;
                    consecutive_failures = 0;

                    debug!(
                        "✓ {} health check passed ({}/{})",
                        component_name, consecutive_successes, config.success_threshold
                    );

                    if consecutive_successes >= config.success_threshold {
                        let elapsed = start_time.elapsed();
                        info!(
                            "✅ {component_name} is healthy after {elapsed:?} ({total_attempts} checks)"
                        );
                        return Ok(());
                    }
                }
                HealthCheckResult::Unhealthy(reason) => {
                    consecutive_failures += 1;
                    consecutive_successes = 0;

                    debug!(
                        "✗ {} health check failed: {} ({}/{})",
                        component_name, reason, consecutive_failures, config.failure_threshold
                    );

                    if consecutive_failures >= config.failure_threshold {
                        warn!(
                            "⚠️  {component_name} failed {consecutive_failures} consecutive health checks"
                        );
                        // Reset failure count to allow continued retries
                        consecutive_failures = 0;
                    }
                }
                HealthCheckResult::Fatal(reason) => {
                    error!("❌ {component_name} health check encountered fatal error: {reason}");
                    return Err(Error::HealthCheckFailed(
                        component_name.to_string(),
                        format!("Fatal error: {reason}"),
                    ));
                }
            }

            // Check if we've exceeded max retries
            if total_attempts >= config.max_retries {
                let elapsed = start_time.elapsed();
                error!(
                    "❌ {component_name} health check timeout after {total_attempts} attempts ({elapsed:?})"
                );
                return Err(Error::HealthCheckFailed(
                    component_name.to_string(),
                    format!("Failed after {} attempts", config.max_retries),
                ));
            }

            // Wait before next check
            trace!("Waiting {}s before next health check", config.interval);
            sleep(Duration::from_secs(config.interval as u64)).await;
        }
    }

    /// Perform a single health check
    async fn perform_health_check(
        &self,
        container_id: &str,
        check_type: &HealthCheckType,
        timeout: u32,
    ) -> HealthCheckResult {
        let timeout_duration = Duration::from_secs(timeout as u64);

        // Execute the check with timeout
        let result = tokio::time::timeout(
            timeout_duration,
            self.execute_check(container_id, check_type),
        )
        .await;

        match result {
            Ok(Ok(true)) => HealthCheckResult::Healthy,
            Ok(Ok(false)) => HealthCheckResult::Unhealthy("Check returned false".to_string()),
            Ok(Err(e)) => {
                // Determine if error is retryable
                if self.is_retryable_error(&e) {
                    HealthCheckResult::Unhealthy(e.to_string())
                } else {
                    HealthCheckResult::Fatal(e.to_string())
                }
            }
            Err(_) => HealthCheckResult::Unhealthy(format!("Check timed out after {timeout}s")),
        }
    }

    /// Execute the specific type of health check
    async fn execute_check(
        &self,
        container_id: &str,
        check_type: &HealthCheckType,
    ) -> Result<bool> {
        match check_type {
            HealthCheckType::Http {
                path,
                expected_status,
            } => self.check_http(container_id, path, *expected_status).await,
            HealthCheckType::Tcp { port } => self.check_tcp(container_id, *port).await,
            HealthCheckType::Exec { command } => self.check_exec(container_id, command).await,
            HealthCheckType::Dns { hosts } => self.check_dns(container_id, hosts).await,
        }
    }

    /// Perform HTTP health check
    async fn check_http(
        &self,
        container_id: &str,
        path: &str,
        expected_status: u16,
    ) -> Result<bool> {
        trace!("Performing HTTP health check on {container_id} path: {path}");

        // Try curl first, then wget as fallback
        let cmd_string = format!(
            "curl -f -s -o /dev/null -w '%{{http_code}}' http://localhost{path} 2>/dev/null || \
             wget -q -O /dev/null --server-response http://localhost{path} 2>&1 | \
             awk '/^  HTTP/{{print $2}}' | tail -1"
        );
        let curl_command = vec!["sh", "-c", &cmd_string];

        match self
            .docker_client
            .exec_in_container(container_id, &curl_command)
            .await
        {
            Ok(output) => {
                let status_code = output.trim().parse::<u16>().unwrap_or(0);
                trace!("HTTP check returned status: {status_code} (expected: {expected_status})");
                Ok(status_code == expected_status)
            }
            Err(e) => {
                debug!("HTTP health check failed: {e}");
                Ok(false)
            }
        }
    }

    /// Perform TCP port check
    async fn check_tcp(&self, container_id: &str, port: u16) -> Result<bool> {
        trace!("Performing TCP health check on {container_id} port: {port}");

        // Try multiple tools for better compatibility
        let cmd_string = format!(
            "nc -z localhost {port} 2>/dev/null || \
             nc -zv localhost {port} 2>/dev/null || \
             (echo > /dev/tcp/localhost/{port}) 2>/dev/null"
        );
        let command = vec!["sh", "-c", &cmd_string];

        match self
            .docker_client
            .exec_in_container(container_id, &command)
            .await
        {
            Ok(_) => {
                trace!("TCP port {port} is open");
                Ok(true)
            }
            Err(_) => {
                trace!("TCP port {port} is not reachable");
                Ok(false)
            }
        }
    }

    /// Perform command execution health check
    async fn check_exec(&self, container_id: &str, command: &[String]) -> Result<bool> {
        trace!("Performing exec health check on {container_id}: {command:?}");

        // Convert &[String] to Vec<&str>
        let cmd_refs: Vec<&str> = command.iter().map(|s| s.as_str()).collect();

        match self
            .docker_client
            .exec_in_container(container_id, &cmd_refs)
            .await
        {
            Ok(output) => {
                trace!("Exec health check succeeded with output: {}", output.trim());
                Ok(true)
            }
            Err(e) => {
                debug!("Exec health check failed: {e}");
                Ok(false)
            }
        }
    }

    /// Perform DNS resolution check
    async fn check_dns(&self, container_id: &str, hosts: &[String]) -> Result<bool> {
        trace!("Performing DNS health check on {container_id} for hosts: {hosts:?}");

        for host in hosts {
            // Try multiple DNS resolution methods
            let cmd_string = format!(
                "nslookup {host} 2>/dev/null | grep -q 'Address' || \
                 getent hosts {host} >/dev/null 2>&1 || \
                 host {host} 2>/dev/null | grep -q 'has address' || \
                 ping -c 1 -W 1 {host} >/dev/null 2>&1"
            );
            let command = vec!["sh", "-c", &cmd_string];

            match self
                .docker_client
                .exec_in_container(container_id, &command)
                .await
            {
                Ok(_) => {
                    trace!("DNS resolution successful for {host}");
                }
                Err(_) => {
                    debug!("DNS resolution failed for {host}");
                    return Ok(false);
                }
            }
        }

        trace!("All DNS resolutions successful");
        Ok(true)
    }

    /// Determine if an error is retryable
    fn is_retryable_error(&self, error: &Error) -> bool {
        // Fatal errors that should not be retried
        match error {
            Error::Docker(msg) if msg.contains("No such container") => false,
            Error::Docker(msg) if msg.contains("is not running") => false,
            Error::Internal(_) => false,
            _ => true, // Most errors are retryable
        }
    }
}

/// Builder for HealthCheckManager with custom configuration
pub struct HealthCheckManagerBuilder {
    docker_client: Option<Arc<dyn DockerClient>>,
}

impl HealthCheckManagerBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            docker_client: None,
        }
    }

    /// Set the Docker client
    pub fn with_docker_client(mut self, client: Arc<dyn DockerClient>) -> Self {
        self.docker_client = Some(client);
        self
    }

    /// Build the HealthCheckManager
    pub fn build(self) -> Result<HealthCheckManager> {
        let docker_client = self
            .docker_client
            .ok_or_else(|| Error::Internal("Docker client is required".to_string()))?;

        Ok(HealthCheckManager::new(docker_client))
    }
}

impl Default for HealthCheckManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::docker::DockerClient;

    /// Mock Docker client for testing
    #[derive(Debug)]
    struct MockDockerClient {
        exec_responses: Mutex<Vec<Result<String>>>,
        exec_calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl MockDockerClient {
        fn new(responses: Vec<Result<String>>) -> Self {
            Self {
                exec_responses: Mutex::new(responses),
                exec_calls: Mutex::new(Vec::new()),
            }
        }

        fn get_exec_calls(&self) -> Vec<(String, Vec<String>)> {
            self.exec_calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl DockerClient for MockDockerClient {
        async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
            // Record the call
            self.exec_calls.lock().unwrap().push((
                container_id.to_string(),
                command.iter().map(|s| s.to_string()).collect(),
            ));

            // Return the next response
            self.exec_responses
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(Err(Error::Docker("No more responses".to_string())))
        }

        // Implement other required methods with defaults
        async fn create_network(&self, _name: &str) -> Result<()> {
            Ok(())
        }
        async fn delete_network(&self, _name: &str) -> Result<()> {
            Ok(())
        }
        async fn network_exists(&self, _name: &str) -> Result<bool> {
            Ok(true)
        }
        async fn pull_image(&self, _image: &str) -> Result<()> {
            Ok(())
        }
        async fn build_image(&self, _tag: &str, _dockerfile: &str, _context: &str) -> Result<()> {
            Ok(())
        }
        async fn run_container(
            &self,
            _image: &str,
            _name: &str,
            _network: &str,
            _env_vars: &[String],
            _ports: &[String],
            _volumes: &[String],
        ) -> Result<String> {
            Ok("container-id".to_string())
        }
        async fn run_container_with_command(
            &self,
            _image: &str,
            _name: &str,
            _network: &str,
            _env_vars: &[String],
            _ports: &[String],
            _volumes: &[String],
            _command: Option<&[String]>,
        ) -> Result<String> {
            Ok("container-id".to_string())
        }
        async fn stop_container(&self, _container_id: &str) -> Result<()> {
            Ok(())
        }
        async fn kill_container(&self, _container_id: &str) -> Result<()> {
            Ok(())
        }
        async fn remove_container(&self, _container_id: &str) -> Result<()> {
            Ok(())
        }
        async fn container_status(
            &self,
            _container_id: &str,
        ) -> Result<rush_docker::ContainerStatus> {
            Ok(rush_docker::ContainerStatus::Running)
        }
        async fn container_exists(&self, _name: &str) -> Result<bool> {
            Ok(true)
        }
        async fn container_logs(&self, _container_id: &str, _lines: usize) -> Result<String> {
            Ok("logs".to_string())
        }
        async fn follow_container_logs(
            &self,
            _container_id: &str,
            _label: String,
            _color: &str,
        ) -> Result<()> {
            Ok(())
        }
        async fn send_signal_to_container(&self, _container_id: &str, _signal: i32) -> Result<()> {
            Ok(())
        }
        async fn get_container_by_name(&self, _name: &str) -> Result<String> {
            Ok("container-id".to_string())
        }
        async fn push_image(&self, _image: &str) -> Result<()> {
            Ok(())
        }
        async fn image_exists(&self, _image: &str) -> Result<bool> {
            Ok(true)
        }
        async fn build_image_with_platform(
            &self,
            _tag: &str,
            _dockerfile: &str,
            _context: &str,
            _platform: &str,
        ) -> Result<()> {
            Ok(())
        }
        async fn run_container_with_platform(
            &self,
            _image: &str,
            _name: &str,
            _network: &str,
            _env_vars: &[String],
            _ports: &[String],
            _volumes: &[String],
            _command: Option<&[String]>,
            _platform: &str,
        ) -> Result<String> {
            Ok("container-id".to_string())
        }
    }

    #[tokio::test]
    async fn test_http_health_check_success() {
        let responses = vec![Ok("200".to_string())];
        let client = Arc::new(MockDockerClient::new(responses));
        let manager = HealthCheckManager::new(client.clone());

        let config = HealthCheckConfig::http("/health")
            .with_initial_delay(0)
            .with_interval(1)
            .with_success_threshold(1);

        let result = manager
            .wait_for_healthy("test-container", "test", &config)
            .await;
        assert!(result.is_ok());

        let calls = client.get_exec_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "test-container");
    }

    #[tokio::test]
    async fn test_tcp_health_check_success() {
        let responses = vec![Ok("".to_string())]; // TCP check just needs success
        let client = Arc::new(MockDockerClient::new(responses));
        let manager = HealthCheckManager::new(client.clone());

        let config = HealthCheckConfig::tcp(8080)
            .with_initial_delay(0)
            .with_interval(1)
            .with_success_threshold(1);

        let result = manager
            .wait_for_healthy("test-container", "test", &config)
            .await;
        assert!(result.is_ok());

        let calls = client.get_exec_calls();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].1.join(" ").contains("8080"));
    }

    #[tokio::test]
    async fn test_dns_health_check_success() {
        let responses = vec![
            Ok("".to_string()), // Second host
            Ok("".to_string()), // First host
        ];
        let client = Arc::new(MockDockerClient::new(responses));
        let manager = HealthCheckManager::new(client.clone());

        let config = HealthCheckConfig::dns(vec![
            "backend.docker".to_string(),
            "frontend.docker".to_string(),
        ])
        .with_initial_delay(0)
        .with_interval(1)
        .with_success_threshold(1);

        let result = manager
            .wait_for_healthy("test-container", "test", &config)
            .await;
        assert!(result.is_ok());

        let calls = client.get_exec_calls();
        assert_eq!(calls.len(), 2); // One for each host
    }

    #[tokio::test]
    async fn test_retry_on_failure() {
        let responses = vec![
            Ok("200".to_string()),                                // Third attempt succeeds
            Err(Error::Docker("Connection refused".to_string())), // Second attempt fails
            Err(Error::Docker("Connection refused".to_string())), // First attempt fails
        ];
        let client = Arc::new(MockDockerClient::new(responses));
        let manager = HealthCheckManager::new(client.clone());

        let config = HealthCheckConfig::http("/health")
            .with_initial_delay(0)
            .with_interval(0) // No delay for test speed
            .with_success_threshold(1)
            .with_max_retries(5);

        let result = manager
            .wait_for_healthy("test-container", "test", &config)
            .await;
        assert!(result.is_ok());

        let calls = client.get_exec_calls();
        assert_eq!(calls.len(), 3); // Three attempts total
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        let responses = vec![
            Err(Error::Docker("Connection refused".to_string())),
            Err(Error::Docker("Connection refused".to_string())),
        ];
        let client = Arc::new(MockDockerClient::new(responses));
        let manager = HealthCheckManager::new(client.clone());

        let config = HealthCheckConfig::http("/health")
            .with_initial_delay(0)
            .with_interval(0)
            .with_success_threshold(1)
            .with_max_retries(2);

        let result = manager
            .wait_for_healthy("test-container", "test", &config)
            .await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed after 2 attempts"));
    }

    #[tokio::test]
    async fn test_success_threshold() {
        let responses = vec![
            Ok("200".to_string()), // Third success
            Ok("200".to_string()), // Second success
            Ok("200".to_string()), // First success
        ];
        let client = Arc::new(MockDockerClient::new(responses));
        let manager = HealthCheckManager::new(client.clone());

        let config = HealthCheckConfig::http("/health")
            .with_initial_delay(0)
            .with_interval(0)
            .with_success_threshold(3); // Require 3 consecutive successes

        let result = manager
            .wait_for_healthy("test-container", "test", &config)
            .await;
        assert!(result.is_ok());

        let calls = client.get_exec_calls();
        assert_eq!(calls.len(), 3);
    }

    #[tokio::test]
    async fn test_exec_health_check() {
        let responses = vec![Ok("ready".to_string())];
        let client = Arc::new(MockDockerClient::new(responses));
        let manager = HealthCheckManager::new(client.clone());

        let config = HealthCheckConfig::exec(vec![
            "pg_isready".to_string(),
            "-U".to_string(),
            "postgres".to_string(),
        ])
        .with_initial_delay(0)
        .with_interval(1)
        .with_success_threshold(1);

        let result = manager
            .wait_for_healthy("test-container", "test", &config)
            .await;
        assert!(result.is_ok());

        let calls = client.get_exec_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, vec!["pg_isready", "-U", "postgres"]);
    }
}
