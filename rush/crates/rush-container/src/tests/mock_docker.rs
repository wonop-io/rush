//! Mock Docker client for testing

use crate::docker::{ContainerStatus, DockerClient};
use rush_core::error::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Mock Docker client for testing
#[derive(Debug, Clone)]
pub struct MockDockerClient {
    /// State tracking for containers
    pub containers: Arc<Mutex<HashMap<String, MockContainer>>>,
    /// State tracking for networks
    pub networks: Arc<Mutex<Vec<String>>>,
    /// State tracking for images
    pub images: Arc<Mutex<HashMap<String, MockImage>>>,
    /// Configurable responses
    pub responses: Arc<Mutex<MockResponses>>,
    /// Call history for assertions
    pub call_history: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug, Clone)]
pub struct MockContainer {
    pub id: String,
    pub name: String,
    pub status: ContainerStatus,
    pub image: String,
    pub network: String,
    pub env_vars: Vec<String>,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MockImage {
    pub tag: String,
    pub architecture: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Default)]
pub struct MockResponses {
    pub should_fail_network_create: bool,
    pub should_fail_container_run: bool,
    pub should_fail_container_stop: bool,
    pub should_fail_image_build: bool,
    pub should_fail_image_push: bool,
    pub container_exit_code: Option<i32>,
    pub container_crash_after_ms: Option<u64>,
    pub startup_logs: Vec<String>,
}

impl Default for MockDockerClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockDockerClient {
    pub fn new() -> Self {
        Self {
            containers: Arc::new(Mutex::new(HashMap::new())),
            networks: Arc::new(Mutex::new(Vec::new())),
            images: Arc::new(Mutex::new(HashMap::new())),
            responses: Arc::new(Mutex::new(MockResponses::default())),
            call_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn set_response(&self, responses: MockResponses) {
        *self.responses.lock().await = responses;
    }

    pub async fn add_image(&self, tag: &str, architecture: &str) {
        let mut images = self.images.lock().await;
        images.insert(
            tag.to_string(),
            MockImage {
                tag: tag.to_string(),
                architecture: architecture.to_string(),
                exists: true,
            },
        );
    }

    pub async fn set_container_status(&self, container_id: &str, status: ContainerStatus) {
        let mut containers = self.containers.lock().await;
        if let Some(container) = containers.get_mut(container_id) {
            container.status = status;
        }
    }

    pub async fn add_container_logs(&self, container_id: &str, logs: Vec<String>) {
        let mut containers = self.containers.lock().await;
        if let Some(container) = containers.get_mut(container_id) {
            container.logs.extend(logs);
        }
    }

    pub async fn get_call_history(&self) -> Vec<String> {
        self.call_history.lock().await.clone()
    }

    async fn record_call(&self, call: String) {
        self.call_history.lock().await.push(call);
    }

    pub async fn simulate_container_crash(&self, container_id: &str) {
        self.set_container_status(container_id, ContainerStatus::Exited(1))
            .await;
    }
}

#[async_trait::async_trait]
impl DockerClient for MockDockerClient {
    async fn create_network(&self, name: &str) -> Result<()> {
        self.record_call(format!("create_network({name})")).await;

        let responses = self.responses.lock().await;
        if responses.should_fail_network_create {
            return Err(Error::Docker("Failed to create network".to_string()));
        }

        let mut networks = self.networks.lock().await;
        networks.push(name.to_string());
        Ok(())
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        self.record_call(format!("delete_network({name})")).await;

        let mut networks = self.networks.lock().await;
        networks.retain(|n| n != name);
        Ok(())
    }

    async fn network_exists(&self, name: &str) -> Result<bool> {
        self.record_call(format!("network_exists({name})")).await;

        let networks = self.networks.lock().await;
        Ok(networks.contains(&name.to_string()))
    }

    async fn pull_image(&self, image: &str) -> Result<()> {
        self.record_call(format!("pull_image({image})")).await;
        Ok(())
    }

    async fn build_image(&self, tag: &str, dockerfile: &str, context: &str) -> Result<()> {
        self.record_call(format!("build_image({tag}, {dockerfile}, {context})"))
            .await;

        let responses = self.responses.lock().await;
        if responses.should_fail_image_build {
            return Err(Error::Docker("Failed to build image".to_string()));
        }

        let mut images = self.images.lock().await;
        images.insert(
            tag.to_string(),
            MockImage {
                tag: tag.to_string(),
                architecture: "amd64".to_string(),
                exists: true,
            },
        );
        Ok(())
    }

    async fn run_container(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        _ports: &[String],
        _volumes: &[String],
    ) -> Result<String> {
        self.record_call(format!("run_container({image}, {name})"))
            .await;

        let responses = self.responses.lock().await;
        if responses.should_fail_container_run {
            return Err(Error::Docker("Failed to run container".to_string()));
        }

        let container_id = format!("mock_{name}");
        let mut containers = self.containers.lock().await;

        let mut container = MockContainer {
            id: container_id.clone(),
            name: name.to_string(),
            status: ContainerStatus::Running,
            image: image.to_string(),
            network: network.to_string(),
            env_vars: env_vars.to_vec(),
            logs: responses.startup_logs.clone(),
        };

        // Note: Removed automatic crash simulation to avoid hanging tests
        // Use simulate_container_crash() directly in tests instead

        // Set exit code if configured
        if let Some(exit_code) = responses.container_exit_code {
            container.status = ContainerStatus::Exited(exit_code);
        }

        let container_clone = container.clone();
        containers.insert(container_id.clone(), container);
        containers.insert(name.to_string(), container_clone);

        Ok(container_id)
    }

    async fn stop_container(&self, container_id: &str) -> Result<()> {
        self.record_call(format!("stop_container({container_id})"))
            .await;

        let responses = self.responses.lock().await;
        if responses.should_fail_container_stop {
            return Err(Error::Docker("Failed to stop container".to_string()));
        }

        let mut containers = self.containers.lock().await;
        if let Some(container) = containers.get_mut(container_id) {
            container.status = ContainerStatus::Exited(0);
        }
        Ok(())
    }

    async fn kill_container(&self, container_id: &str) -> Result<()> {
        self.record_call(format!("kill_container({container_id})"))
            .await;

        let mut containers = self.containers.lock().await;
        if let Some(container) = containers.get_mut(container_id) {
            container.status = ContainerStatus::Exited(137); // Exit code for SIGKILL
        }
        Ok(())
    }

    async fn remove_container(&self, container_id: &str) -> Result<()> {
        self.record_call(format!("remove_container({container_id})"))
            .await;

        let mut containers = self.containers.lock().await;
        containers.remove(container_id);
        Ok(())
    }

    async fn container_status(&self, container_id: &str) -> Result<ContainerStatus> {
        self.record_call(format!("container_status({container_id})"))
            .await;

        let containers = self.containers.lock().await;
        if let Some(container) = containers.get(container_id) {
            Ok(container.status.clone())
        } else {
            Ok(ContainerStatus::Unknown)
        }
    }

    async fn container_exists(&self, name: &str) -> Result<bool> {
        self.record_call(format!("container_exists({name})")).await;

        let containers = self.containers.lock().await;
        Ok(containers.contains_key(name))
    }

    async fn container_logs(&self, container_id: &str, lines: usize) -> Result<String> {
        self.record_call(format!("container_logs({container_id}, {lines})"))
            .await;

        let containers = self.containers.lock().await;
        if let Some(container) = containers.get(container_id) {
            let logs = container.logs.clone();
            let start = if logs.len() > lines {
                logs.len() - lines
            } else {
                0
            };
            Ok(logs[start..].join("\n"))
        } else {
            Ok(String::new())
        }
    }

    async fn follow_container_logs(
        &self,
        container_id: &str,
        _label: String,
        _color: &str,
    ) -> Result<()> {
        self.record_call(format!("follow_container_logs({container_id})"))
            .await;
        Ok(())
    }

    async fn send_signal_to_container(&self, container_id: &str, signal: i32) -> Result<()> {
        self.record_call(format!(
            "send_signal_to_container({container_id}, {signal})"
        ))
        .await;

        if signal == 15 || signal == 9 {
            // SIGTERM or SIGKILL
            self.stop_container(container_id).await?;
        }
        Ok(())
    }

    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
        self.record_call(format!("exec_in_container({container_id}, {command:?})"))
            .await;
        Ok("Mock output".to_string())
    }

    async fn get_container_by_name(&self, name: &str) -> Result<String> {
        self.record_call(format!("get_container_by_name({name})"))
            .await;

        let containers = self.containers.lock().await;
        if let Some(container) = containers.get(name) {
            Ok(container.id.clone())
        } else {
            Err(Error::Docker(format!("Container {name} not found")))
        }
    }

    async fn run_container_with_command(
        &self,
        image: &str,
        name: &str,
        network: &str,
        env_vars: &[String],
        _ports: &[String],
        _volumes: &[String],
        _command: Option<&[String]>,
    ) -> Result<String> {
        self.record_call(format!("run_container_with_command({image}, {name})"))
            .await;

        let responses = self.responses.lock().await;
        if responses.should_fail_container_run {
            return Err(Error::Docker("Failed to run container".to_string()));
        }

        let container_id = format!("mock_{name}");
        let mut containers = self.containers.lock().await;

        let mut container = MockContainer {
            id: container_id.clone(),
            name: name.to_string(),
            status: ContainerStatus::Running,
            image: image.to_string(),
            network: network.to_string(),
            env_vars: env_vars.to_vec(),
            logs: responses.startup_logs.clone(),
        };

        // Set exit code if configured
        if let Some(exit_code) = responses.container_exit_code {
            container.status = ContainerStatus::Exited(exit_code);
        }

        let container_clone = container.clone();
        containers.insert(container_id.clone(), container);
        containers.insert(name.to_string(), container_clone);

        Ok(container_id)
    }
    
    async fn push_image(&self, image: &str) -> Result<()> {
        self.record_call(format!("push_image({image})")).await;

        let responses = self.responses.lock().await;
        if responses.should_fail_image_push {
            return Err(Error::Docker(format!("Failed to push image: {image}")));
        }

        Ok(())
    }

    async fn image_exists(&self, image: &str) -> Result<bool> {
        self.record_call(format!("image_exists({image})")).await;

        let images = self.images.lock().await;
        // Check if image exists
        Ok(images.contains_key(image))
    }
}
