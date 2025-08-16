//! Adapter to make rush-container's DockerClient compatible with rush-local-services

use crate::docker as container_docker;
use rush_core::error::Result;
use rush_local_services::docker as local_docker;
use std::sync::Arc;

/// Adapter that wraps rush-container's DockerClient for use with rush-local-services
pub struct LocalServicesDockerAdapter {
    inner: Arc<dyn container_docker::DockerClient>,
}

impl LocalServicesDockerAdapter {
    /// Create a new adapter
    pub fn new(inner: Arc<dyn container_docker::DockerClient>) -> Self {
        Self { inner }
    }
}

impl std::fmt::Debug for LocalServicesDockerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalServicesDockerAdapter").finish()
    }
}

#[async_trait::async_trait]
impl local_docker::DockerClient for LocalServicesDockerAdapter {
    async fn run_container(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        ports: &[String],
        volumes: &[String],
    ) -> Result<String> {
        self.inner
            .run_container(image, name, network, env_vars, ports, volumes)
            .await
    }

    async fn stop_container(&self, container_id: &str) -> Result<()> {
        self.inner.stop_container(container_id).await
    }

    async fn remove_container(&self, container_id: &str) -> Result<()> {
        self.inner.remove_container(container_id).await
    }

    async fn container_status(&self, container_id: &str) -> Result<local_docker::ContainerStatus> {
        let status = self.inner.container_status(container_id).await?;
        Ok(match status {
            container_docker::ContainerStatus::Running => local_docker::ContainerStatus::Running,
            container_docker::ContainerStatus::Exited(code) => {
                local_docker::ContainerStatus::Exited(code)
            }
            container_docker::ContainerStatus::Unknown => local_docker::ContainerStatus::Unknown,
        })
    }

    async fn container_logs(&self, container_id: &str, lines: usize) -> Result<String> {
        self.inner.container_logs(container_id, lines).await
    }

    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
        self.inner.exec_in_container(container_id, command).await
    }

    async fn get_container_by_name(&self, name: &str) -> Result<String> {
        self.inner.get_container_by_name(name).await
    }
}
