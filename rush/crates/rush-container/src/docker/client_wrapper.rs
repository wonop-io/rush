//! Enhanced Docker client wrapper with retry logic and monitoring
//!
//! This module provides an improved Docker client with automatic retries,
//! connection pooling, and monitoring capabilities.

use crate::{
    docker::{DockerClient, ContainerStatus},
    events::{Event, EventBus},
};
use async_trait::async_trait;
use rush_core::error::{Error, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use log::{debug, warn};

/// Configuration for the Docker client wrapper
#[derive(Debug, Clone)]
pub struct DockerWrapperConfig {
    /// Maximum number of retries for operations
    pub max_retries: u32,
    /// Initial retry delay
    pub initial_retry_delay: Duration,
    /// Maximum retry delay
    pub max_retry_delay: Duration,
    /// Timeout for individual operations
    pub operation_timeout: Duration,
    /// Maximum concurrent Docker operations
    pub max_concurrent_operations: usize,
    /// Enable detailed logging
    pub verbose: bool,
}

impl Default for DockerWrapperConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay: Duration::from_millis(500),
            max_retry_delay: Duration::from_secs(10),
            operation_timeout: Duration::from_secs(30),
            max_concurrent_operations: 10,
            verbose: false,
        }
    }
}

/// Statistics for Docker operations
#[derive(Debug, Clone, Default)]
pub struct DockerStats {
    /// Total operations attempted
    pub total_operations: u64,
    /// Successful operations
    pub successful_operations: u64,
    /// Failed operations
    pub failed_operations: u64,
    /// Operations that succeeded after retry
    pub retried_operations: u64,
    /// Total retry attempts
    pub total_retries: u64,
    /// Average operation duration
    pub avg_operation_duration: Duration,
    /// Current active operations
    pub active_operations: usize,
}

/// Enhanced Docker client wrapper
#[derive(Debug)]
pub struct DockerClientWrapper {
    /// Underlying Docker client
    inner: Arc<dyn DockerClient>,
    /// Configuration
    config: DockerWrapperConfig,
    /// Event bus for publishing events
    event_bus: Option<EventBus>,
    /// Semaphore for limiting concurrent operations
    semaphore: Arc<Semaphore>,
    /// Statistics
    stats: Arc<RwLock<DockerStats>>,
    /// Connection health status
    healthy: Arc<RwLock<bool>>,
}

impl DockerClientWrapper {
    /// Create a new Docker client wrapper
    pub fn new(
        inner: Arc<dyn DockerClient>,
        config: DockerWrapperConfig,
    ) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_operations));
        
        Self {
            inner,
            config,
            event_bus: None,
            semaphore,
            stats: Arc::new(RwLock::new(DockerStats::default())),
            healthy: Arc::new(RwLock::new(true)),
        }
    }

    /// Set the event bus
    pub fn set_event_bus(&mut self, event_bus: EventBus) {
        self.event_bus = Some(event_bus);
    }

    /// Get current statistics
    pub async fn get_stats(&self) -> DockerStats {
        self.stats.read().await.clone()
    }

    /// Check if Docker connection is healthy
    pub async fn is_healthy(&self) -> bool {
        *self.healthy.read().await
    }

    /// Execute an operation with retry logic
    async fn execute_with_retry<F, T>(
        &self,
        operation_name: &str,
        operation: F,
    ) -> Result<T>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>> + Send,
        T: Send,
    {
        let start_time = Instant::now();
        let _permit = self.semaphore.acquire().await.unwrap();
        
        // Update active operations count
        {
            let mut stats = self.stats.write().await;
            stats.active_operations += 1;
            stats.total_operations += 1;
        }
        
        let mut delay = self.config.initial_retry_delay;
        let mut attempts = 0;
        let mut last_error = None;
        
        while attempts <= self.config.max_retries {
            if attempts > 0 {
                if self.config.verbose {
                    debug!(
                        "Retrying {} operation (attempt {}/{})",
                        operation_name,
                        attempts + 1,
                        self.config.max_retries + 1
                    );
                }
                
                // Update retry stats
                {
                    let mut stats = self.stats.write().await;
                    stats.total_retries += 1;
                }
                
                // Wait with exponential backoff
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(self.config.max_retry_delay);
            }
            
            // Execute with timeout
            let result = tokio::time::timeout(
                self.config.operation_timeout,
                operation()
            ).await;
            
            match result {
                Ok(Ok(value)) => {
                    // Update stats for success
                    let duration = start_time.elapsed();
                    {
                        let mut stats = self.stats.write().await;
                        stats.successful_operations += 1;
                        stats.active_operations -= 1;
                        if attempts > 0 {
                            stats.retried_operations += 1;
                        }
                        // Update average duration (simple moving average)
                        let total = stats.successful_operations + stats.failed_operations;
                        if total > 0 {
                            let current_avg = stats.avg_operation_duration.as_millis() as u64;
                            let new_avg = (current_avg * (total - 1) + duration.as_millis() as u64) / total;
                            stats.avg_operation_duration = Duration::from_millis(new_avg);
                        }
                    }
                    
                    // Mark as healthy
                    *self.healthy.write().await = true;
                    
                    if self.config.verbose {
                        debug!("{} operation succeeded in {:?}", operation_name, duration);
                    }
                    
                    return Ok(value);
                }
                Ok(Err(e)) => {
                    last_error = Some(e);
                    attempts += 1;
                    
                    if attempts > self.config.max_retries {
                        warn!(
                            "{} operation failed after {} attempts: {:?}",
                            operation_name,
                            attempts,
                            last_error
                        );
                    }
                }
                Err(_) => {
                    last_error = Some(Error::Docker(format!(
                        "{} operation timed out after {:?}",
                        operation_name,
                        self.config.operation_timeout
                    )));
                    attempts += 1;
                    
                    warn!("{} operation timed out", operation_name);
                }
            }
        }
        
        // Update stats for failure
        {
            let mut stats = self.stats.write().await;
            stats.failed_operations += 1;
            stats.active_operations -= 1;
        }
        
        // Mark as unhealthy if critical operation fails
        if operation_name == "health_check" {
            *self.healthy.write().await = false;
        }
        
        // Publish error event if we have event bus
        if let Some(event_bus) = &self.event_bus {
            let _ = event_bus.publish(Event::error(
                "docker",
                format!("{} operation failed: {:?}", operation_name, last_error),
                false,
            )).await;
        }
        
        Err(last_error.unwrap_or_else(|| {
            Error::Docker(format!("{} operation failed", operation_name))
        }))
    }

    /// Health check for Docker connection
    pub async fn health_check(&self) -> Result<()> {
        // Use a simple operation to check health
        self.execute_with_retry("health_check", || {
            let client = self.inner.clone();
            Box::pin(async move {
                // Check if we can list networks as a health check
                client.network_exists("bridge").await.map(|_| ())
            })
        }).await
    }
}

#[async_trait]
impl DockerClient for DockerClientWrapper {
    async fn create_network(&self, name: &str) -> Result<()> {
        let name = name.to_string();
        
        self.execute_with_retry("create_network", || {
            let client = self.inner.clone();
            let name = name.clone();
            Box::pin(async move {
                client.create_network(&name).await
            })
        }).await
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        let name = name.to_string();
        
        self.execute_with_retry("delete_network", || {
            let client = self.inner.clone();
            let name = name.clone();
            Box::pin(async move {
                client.delete_network(&name).await
            })
        }).await
    }

    async fn network_exists(&self, name: &str) -> Result<bool> {
        // Call underlying client directly - network checks should be immediate
        // and not retried as this can interfere with container startup logic
        self.inner.network_exists(name).await
    }

    async fn build_image(
        &self,
        tag: &str,
        dockerfile: &str,
        context: &str,
    ) -> Result<()> {
        let tag = tag.to_string();
        let dockerfile = dockerfile.to_string();
        let context = context.to_string();
        
        self.execute_with_retry("build_image", || {
            let client = self.inner.clone();
            let tag = tag.clone();
            let dockerfile = dockerfile.clone();
            let context = context.clone();
            Box::pin(async move {
                client.build_image(&tag, &dockerfile, &context).await
            })
        }).await
    }

    async fn run_container(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
    ) -> Result<String> {
        // Call underlying client directly - don't retry container creation
        // as the lifecycle manager handles retries with proper cleanup
        self.inner.run_container(image, name, network, env_vars, ports, volumes).await
    }

    async fn run_container_with_command(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
        command: Option<&[String]>,
    ) -> Result<String> {
        // Call underlying client directly - don't retry container creation
        // as the lifecycle manager handles retries with proper cleanup
        self.inner.run_container_with_command(image, name, network, env_vars, ports, volumes, command).await
    }

    async fn stop_container(&self, id: &str) -> Result<()> {
        let id = id.to_string();

        self.execute_with_retry("stop_container", || {
            let client = self.inner.clone();
            let id = id.clone();
            Box::pin(async move {
                client.stop_container(&id).await
            })
        }).await
    }

    async fn kill_container(&self, id: &str) -> Result<()> {
        let id = id.to_string();

        self.execute_with_retry("kill_container", || {
            let client = self.inner.clone();
            let id = id.clone();
            Box::pin(async move {
                client.kill_container(&id).await
            })
        }).await
    }

    async fn remove_container(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        
        self.execute_with_retry("remove_container", || {
            let client = self.inner.clone();
            let id = id.clone();
            Box::pin(async move {
                client.remove_container(&id).await
            })
        }).await
    }

    async fn container_status(&self, id: &str) -> Result<ContainerStatus> {
        let id = id.to_string();
        
        self.execute_with_retry("container_status", || {
            let client = self.inner.clone();
            let id = id.clone();
            Box::pin(async move {
                client.container_status(&id).await
            })
        }).await
    }

    async fn container_logs(&self, id: &str, lines: usize) -> Result<String> {
        let id = id.to_string();
        
        self.execute_with_retry("container_logs", || {
            let client = self.inner.clone();
            let id = id.clone();
            Box::pin(async move {
                client.container_logs(&id, lines).await
            })
        }).await
    }

    async fn container_exists(&self, name: &str) -> Result<bool> {
        let name = name.to_string();
        
        self.execute_with_retry("container_exists", || {
            let client = self.inner.clone();
            let name = name.clone();
            Box::pin(async move {
                client.container_exists(&name).await
            })
        }).await
    }

    async fn get_container_by_name(&self, name: &str) -> Result<String> {
        let name = name.to_string();
        
        self.execute_with_retry("get_container_by_name", || {
            let client = self.inner.clone();
            let name = name.clone();
            Box::pin(async move {
                client.get_container_by_name(&name).await
            })
        }).await
    }

    async fn pull_image(&self, name: &str) -> Result<()> {
        let name = name.to_string();
        
        self.execute_with_retry("pull_image", || {
            let client = self.inner.clone();
            let name = name.clone();
            Box::pin(async move {
                client.pull_image(&name).await
            })
        }).await
    }

    async fn follow_container_logs(
        &self,
        container_id: &str,
        label: String,
        color: &str,
    ) -> Result<()> {
        let container_id = container_id.to_string();
        let color = color.to_string();
        
        self.execute_with_retry("follow_container_logs", || {
            let client = self.inner.clone();
            let container_id = container_id.clone();
            let label = label.clone();
            let color = color.clone();
            Box::pin(async move {
                client.follow_container_logs(&container_id, label, &color).await
            })
        }).await
    }

    async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()> {
        let container_id = container_id.to_string();
        
        self.execute_with_retry("send_signal_to_container", || {
            let client = self.inner.clone();
            let container_id = container_id.clone();
            Box::pin(async move {
                client.send_signal_to_container(&container_id, signal).await
            })
        }).await
    }

    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
        let container_id = container_id.to_string();
        let command: Vec<String> = command.iter().map(|s| s.to_string()).collect();
        
        self.execute_with_retry("exec_in_container", || {
            let client = self.inner.clone();
            let container_id = container_id.clone();
            let command = command.clone();
            Box::pin(async move {
                let cmd_refs: Vec<&str> = command.iter().map(|s| s.as_str()).collect();
                client.exec_in_container(&container_id, &cmd_refs).await
            })
        }).await
    }
    
    async fn push_image(&self, image: &str) -> Result<()> {
        let image = image.to_string();

        // Docker push can take longer, so we might want fewer retries
        // but let's use the standard retry logic for consistency
        self.execute_with_retry("push_image", || {
            let client = self.inner.clone();
            let image = image.clone();
            Box::pin(async move {
                client.push_image(&image).await
            })
        }).await
    }

    async fn image_exists(&self, image: &str) -> Result<bool> {
        let image = image.to_string();

        self.execute_with_retry("image_exists", || {
            let client = self.inner.clone();
            let image = image.clone();
            Box::pin(async move {
                client.image_exists(&image).await
            })
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wrapper_config_default() {
        let config = DockerWrapperConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_retry_delay, Duration::from_millis(500));
        assert_eq!(config.max_concurrent_operations, 10);
    }
}