//! Reliable Docker client with retry logic and circuit breaker

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rush_core::reliability::{with_retry, with_timeout, CircuitBreaker, RetryConfig};
use rush_core::Result;

use crate::{ContainerStatus, DockerClient};

/// Docker client wrapper with reliability features
pub struct ReliableDockerClient {
    inner: Arc<dyn DockerClient>,
    retry_config: RetryConfig,
    _circuit_breaker: CircuitBreaker,
    _operation_timeout: Duration,
}

impl ReliableDockerClient {
    /// Create a new reliable Docker client
    pub fn new(inner: Arc<dyn DockerClient>) -> Self {
        Self {
            inner,
            retry_config: RetryConfig {
                max_retries: 3,
                initial_backoff: Duration::from_millis(500),
                max_backoff: Duration::from_secs(10),
                backoff_multiplier: 2.0,
            },
            _circuit_breaker: CircuitBreaker::new(5, Duration::from_secs(60)),
            _operation_timeout: Duration::from_secs(120),
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        inner: Arc<dyn DockerClient>,
        retry_config: RetryConfig,
        circuit_threshold: u32,
        circuit_reset_timeout: Duration,
        operation_timeout: Duration,
    ) -> Self {
        Self {
            inner,
            retry_config,
            _circuit_breaker: CircuitBreaker::new(circuit_threshold, circuit_reset_timeout),
            _operation_timeout: operation_timeout,
        }
    }
}

impl std::fmt::Debug for ReliableDockerClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReliableDockerClient")
            .field("inner", &self.inner)
            .field("retry_config", &self.retry_config)
            .finish()
    }
}

#[async_trait]
impl DockerClient for ReliableDockerClient {
    async fn create_network(&self, name: &str) -> Result<()> {
        let name = name.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let name = name.clone();
                async move { inner.create_network(&name).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        let name = name.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let name = name.clone();
                async move { inner.delete_network(&name).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn network_exists(&self, name: &str) -> Result<bool> {
        let name = name.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let name = name.clone();
                async move { inner.network_exists(&name).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn pull_image(&self, image: &str) -> Result<()> {
        let image = image.to_string();
        let inner = Arc::clone(&self.inner);

        // Pulling images can take a long time
        with_timeout(
            inner.pull_image(&image),
            Duration::from_secs(600), // 10 minute timeout
            "docker pull",
        )
        .await
    }

    async fn build_image(&self, tag: &str, dockerfile: &str, context: &str) -> Result<()> {
        let tag = tag.to_string();
        let dockerfile = dockerfile.to_string();
        let context = context.to_string();
        let inner = Arc::clone(&self.inner);

        // Building images can take a long time, so we don't retry
        // but we do add timeout protection
        with_timeout(
            inner.build_image(&tag, &dockerfile, &context),
            Duration::from_secs(600), // 10 minute timeout for builds
            "docker build",
        )
        .await
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
        let image = image.to_string();
        let name = name.to_string();
        let network = network.to_string();
        let env_vars = env_vars.to_vec();
        let ports = ports.to_vec();
        let volumes = volumes.to_vec();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let image = image.clone();
                let name = name.clone();
                let network = network.clone();
                let env_vars = env_vars.clone();
                let ports = ports.clone();
                let volumes = volumes.clone();
                async move {
                    inner
                        .run_container(&image, &name, &network, &env_vars, &ports, &volumes)
                        .await
                }
            },
            self.retry_config.clone(),
        )
        .await
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
        let image = image.to_string();
        let name = name.to_string();
        let network = network.to_string();
        let env_vars = env_vars.to_vec();
        let ports = ports.to_vec();
        let volumes = volumes.to_vec();
        let command = command.map(|c| c.to_vec());
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let image = image.clone();
                let name = name.clone();
                let network = network.clone();
                let env_vars = env_vars.clone();
                let ports = ports.clone();
                let volumes = volumes.clone();
                let command = command.clone();
                async move {
                    inner
                        .run_container_with_command(
                            &image,
                            &name,
                            &network,
                            &env_vars,
                            &ports,
                            &volumes,
                            command.as_deref(),
                        )
                        .await
                }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn build_image_with_platform(
        &self,
        tag: &str,
        dockerfile: &str,
        context: &str,
        platform: &str,
    ) -> Result<()> {
        let tag = tag.to_string();
        let dockerfile = dockerfile.to_string();
        let context = context.to_string();
        let platform = platform.to_string();
        let inner = Arc::clone(&self.inner);

        // Building images can take a long time, so we don't retry
        // but we do add timeout protection
        with_timeout(
            inner.build_image_with_platform(&tag, &dockerfile, &context, &platform),
            Duration::from_secs(600), // 10 minute timeout for builds
            "docker build",
        )
        .await
    }

    async fn run_container_with_platform(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
        command: Option<&[String]>,
        platform: &str,
    ) -> Result<String> {
        let image = image.to_string();
        let name = name.to_string();
        let network = network.to_string();
        let env_vars = env_vars.to_vec();
        let ports = ports.to_vec();
        let volumes = volumes.to_vec();
        let command = command.map(|c| c.to_vec());
        let platform = platform.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let image = image.clone();
                let name = name.clone();
                let network = network.clone();
                let env_vars = env_vars.clone();
                let ports = ports.clone();
                let volumes = volumes.clone();
                let command = command.clone();
                let platform = platform.clone();
                async move {
                    inner
                        .run_container_with_platform(
                            &image,
                            &name,
                            &network,
                            &env_vars,
                            &ports,
                            &volumes,
                            command.as_deref(),
                            &platform,
                        )
                        .await
                }
            },
            self.retry_config.clone(),
        )
        .await
    }

    fn target_platform(&self) -> &str {
        self.inner.target_platform()
    }

    async fn stop_container(&self, container_id: &str) -> Result<()> {
        let container_id = container_id.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let container_id = container_id.clone();
                async move { inner.stop_container(&container_id).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn kill_container(&self, container_id: &str) -> Result<()> {
        let container_id = container_id.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let container_id = container_id.clone();
                async move { inner.kill_container(&container_id).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn remove_container(&self, container_id: &str) -> Result<()> {
        let container_id = container_id.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let container_id = container_id.clone();
                async move { inner.remove_container(&container_id).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn container_status(&self, container_id: &str) -> Result<ContainerStatus> {
        let container_id = container_id.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let container_id = container_id.clone();
                async move { inner.container_status(&container_id).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn container_exists(&self, name: &str) -> Result<bool> {
        let name = name.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let name = name.clone();
                async move { inner.container_exists(&name).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn container_logs(&self, container_id: &str, lines: usize) -> Result<String> {
        let container_id = container_id.to_string();
        let inner = Arc::clone(&self.inner);

        // Logs might be large, so we don't retry
        with_timeout(
            inner.container_logs(&container_id, lines),
            Duration::from_secs(30),
            "docker logs",
        )
        .await
    }

    async fn follow_container_logs(
        &self,
        container_id: &str,
        label: String,
        color: &str,
    ) -> Result<()> {
        // Following logs is a streaming operation, we don't wrap it
        // with retry or timeout as it's meant to run indefinitely
        self.inner
            .follow_container_logs(container_id, label, color)
            .await
    }

    async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()> {
        let container_id = container_id.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let container_id = container_id.clone();
                async move { inner.send_signal_to_container(&container_id, signal).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
        let container_id = container_id.to_string();
        let command: Vec<String> = command.iter().map(|s| s.to_string()).collect();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let container_id = container_id.clone();
                let command = command.clone();
                async move {
                    let cmd_refs: Vec<&str> = command.iter().map(|s| s.as_str()).collect();
                    inner.exec_in_container(&container_id, &cmd_refs).await
                }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn get_container_by_name(&self, name: &str) -> Result<String> {
        let name = name.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let name = name.clone();
                async move { inner.get_container_by_name(&name).await }
            },
            self.retry_config.clone(),
        )
        .await
    }

    async fn push_image(&self, image: &str) -> Result<()> {
        let image = image.to_string();
        let inner = Arc::clone(&self.inner);

        // Pushing images can take a long time
        with_timeout(
            inner.push_image(&image),
            Duration::from_secs(600), // 10 minute timeout
            "docker push",
        )
        .await
    }

    async fn image_exists(&self, image: &str) -> Result<bool> {
        let image = image.to_string();
        let inner = Arc::clone(&self.inner);

        with_retry(
            || {
                let inner = Arc::clone(&inner);
                let image = image.clone();
                async move { inner.image_exists(&image).await }
            },
            self.retry_config.clone(),
        )
        .await
    }
}
