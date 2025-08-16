use crate::docker::{ContainerStatus, DockerClient};
use crate::{
    config::LocalServiceConfig,
    error::{Error, Result},
    health::{HealthCheck, HealthStatus},
};
use log::{debug, info};
use std::sync::Arc;
use tokio::time::sleep;

/// Status of a local service
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStatus {
    /// Service is not running
    Stopped,

    /// Service is starting up
    Starting,

    /// Service is running and healthy
    Running,

    /// Service is running but unhealthy
    Unhealthy(String),

    /// Service has failed
    Failed(String),
}

/// Handle to a running local service
pub struct LocalServiceHandle {
    /// Service configuration
    pub config: LocalServiceConfig,

    /// Container ID if running
    container_id: Option<String>,

    /// Docker client
    docker_client: Arc<dyn DockerClient>,

    /// Current status
    status: ServiceStatus,

    /// Health check configuration
    health_check: Option<HealthCheck>,
}

impl LocalServiceHandle {
    /// Create a new service handle
    pub fn new(config: LocalServiceConfig, docker_client: Arc<dyn DockerClient>) -> Self {
        let health_check = config.get_health_check().map(HealthCheck::new);

        Self {
            config,
            container_id: None,
            docker_client,
            status: ServiceStatus::Stopped,
            health_check,
        }
    }

    /// Get the service name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get the current status
    pub fn status(&self) -> ServiceStatus {
        self.status.clone()
    }

    /// Check if the service is running
    pub fn is_running(&self) -> bool {
        matches!(
            self.status,
            ServiceStatus::Running | ServiceStatus::Unhealthy(_)
        )
    }

    /// Get the container ID
    pub fn container_id(&self) -> Option<&str> {
        self.container_id.as_deref()
    }

    /// Start the service
    pub async fn start(&mut self) -> Result<()> {
        if self.is_running() {
            return Err(Error::ServiceAlreadyRunning(self.config.name.clone()));
        }

        info!("Starting local service: {}", self.config.name);
        self.status = ServiceStatus::Starting;

        // Check if container already exists
        let container_name = self.config.get_container_name();
        if let Ok(existing) = self
            .docker_client
            .get_container_by_name(&container_name)
            .await
        {
            info!(
                "Found existing container for {}, removing it",
                self.config.name
            );
            let _ = self.docker_client.stop_container(&existing).await;
            let _ = self.docker_client.remove_container(&existing).await;
        }

        // Prepare Docker run configuration
        let image = self.config.get_image();
        let mut env_vars = Vec::new();
        for (key, value) in &self.config.env {
            env_vars.push(format!("{key}={value}"));
        }

        let mut ports = Vec::new();
        for port in &self.config.ports {
            ports.push(port.to_docker_format());
        }

        let mut volumes = Vec::new();
        for volume in &self.config.volumes {
            volumes.push(volume.to_docker_format());
        }

        // Build Docker run command
        let mut docker_args = vec![
            "--name".to_string(),
            container_name.clone(),
            "--detach".to_string(),
        ];

        // Add network if specified
        if let Some(network) = &self.config.network_mode {
            docker_args.push("--network".to_string());
            docker_args.push(network.clone());
        }

        // Add resource limits if specified
        if let Some(resources) = &self.config.resources {
            if let Some(memory) = &resources.memory {
                docker_args.push("--memory".to_string());
                docker_args.push(memory.clone());
            }
            if let Some(cpus) = &resources.cpus {
                docker_args.push("--cpus".to_string());
                docker_args.push(cpus.clone());
            }
        }

        // Add environment variables
        for env in &env_vars {
            docker_args.push("-e".to_string());
            docker_args.push(env.clone());
        }

        // Add port mappings
        for port in &ports {
            docker_args.push("-p".to_string());
            docker_args.push(port.clone());
        }

        // Add volume mappings
        for volume in &volumes {
            docker_args.push("-v".to_string());
            docker_args.push(volume.clone());
        }

        // Add custom Docker arguments
        docker_args.extend(self.config.docker_args.clone());

        // Add image
        docker_args.push(image.clone());

        // Prepare command if specified
        let command_args = self.config.command.as_ref().map(|cmd| {
            cmd.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>()
        });

        // Run the container using the proper method signature
        let container_id = self
            .docker_client
            .run_container(
                &image,
                &container_name,
                self.config.network_mode.as_deref().unwrap_or("bridge"),
                &env_vars,
                &ports,
                &volumes,
                command_args.as_deref(),
            )
            .await
            .map_err(|e| Error::Docker(format!("Failed to start {}: {}", self.config.name, e)))?;

        self.container_id = Some(container_id);

        // Wait for health check if configured
        if self.health_check.is_some() {
            self.wait_for_healthy().await?;
        }

        self.status = ServiceStatus::Running;
        info!("Local service {} started successfully", self.config.name);

        Ok(())
    }

    /// Stop the service
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(container_id) = &self.container_id {
            info!("Stopping local service: {}", self.config.name);

            self.docker_client
                .stop_container(container_id)
                .await
                .map_err(|e| {
                    Error::Docker(format!("Failed to stop {}: {}", self.config.name, e))
                })?;

            // Only remove if not persisting data
            if !self.config.persist_data {
                self.docker_client
                    .remove_container(container_id)
                    .await
                    .map_err(|e| {
                        Error::Docker(format!("Failed to remove {}: {}", self.config.name, e))
                    })?;
            }

            self.container_id = None;
            self.status = ServiceStatus::Stopped;
            info!("Local service {} stopped", self.config.name);
        }

        Ok(())
    }

    /// Restart the service
    pub async fn restart(&mut self) -> Result<()> {
        self.stop().await?;
        self.start().await
    }

    /// Check the health of the service
    pub async fn check_health(&mut self) -> Result<HealthStatus> {
        if let Some(container_id) = &self.container_id {
            // Check if container is running
            match self.docker_client.container_status(container_id).await {
                Ok(ContainerStatus::Running) => {
                    // If health check is configured, run it
                    if let Some(health_check) = &self.health_check {
                        let result = self
                            .docker_client
                            .exec_in_container(
                                container_id,
                                &health_check.command.split_whitespace().collect::<Vec<_>>(),
                            )
                            .await;

                        match result {
                            Ok(_) => Ok(HealthStatus::Healthy),
                            Err(e) => {
                                Ok(HealthStatus::Unhealthy(format!("Health check failed: {e}")))
                            }
                        }
                    } else {
                        Ok(HealthStatus::Unknown)
                    }
                }
                Ok(ContainerStatus::Exited(code)) => {
                    self.status =
                        ServiceStatus::Failed(format!("Container exited with code {code}"));
                    Ok(HealthStatus::NotRunning)
                }
                _ => Ok(HealthStatus::NotRunning),
            }
        } else {
            Ok(HealthStatus::NotRunning)
        }
    }

    /// Wait for the service to become healthy
    async fn wait_for_healthy(&mut self) -> Result<()> {
        // Clone the health check to avoid borrowing issues
        let health_check = self.health_check.clone();

        if let Some(health_check) = health_check {
            info!("Waiting for {} to become healthy...", self.config.name);

            // Wait for start period
            sleep(health_check.start_period).await;

            let mut retries = 0;
            let max_retries = health_check.retries;
            let interval = health_check.interval;

            loop {
                match self.check_health().await? {
                    HealthStatus::Healthy => {
                        info!("{} is healthy", self.config.name);
                        return Ok(());
                    }
                    HealthStatus::NotRunning => {
                        return Err(Error::HealthCheckFailed(
                            self.config.name.clone(),
                            "Service is not running".to_string(),
                        ));
                    }
                    _ => {
                        retries += 1;
                        if retries >= max_retries {
                            return Err(Error::HealthCheckFailed(
                                self.config.name.clone(),
                                format!("Health check failed after {retries} retries"),
                            ));
                        }

                        debug!(
                            "Health check attempt {}/{} for {}",
                            retries, max_retries, self.config.name
                        );
                        sleep(interval).await;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get logs from the service
    pub async fn logs(&self, lines: usize) -> Result<String> {
        if let Some(container_id) = &self.container_id {
            self.docker_client
                .container_logs(container_id, lines)
                .await
                .map_err(|e| {
                    Error::Docker(format!(
                        "Failed to get logs for {}: {}",
                        self.config.name, e
                    ))
                })
        } else {
            Ok(String::new())
        }
    }

    /// Get the hostname for connecting to this service
    pub fn hostname(&self) -> String {
        // In Docker network mode, use the container name
        // Otherwise use localhost
        if self.config.network_mode.is_some() {
            self.config.get_container_name()
        } else {
            "localhost".to_string()
        }
    }

    /// Get the primary port for this service
    pub fn port(&self) -> Option<u16> {
        self.config.ports.first().map(|p| p.host_port)
    }
}
