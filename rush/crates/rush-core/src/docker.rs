use crate::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt;

/// Container status enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum ContainerStatus {
    Running,
    Stopped,
    Exited(i32),
    Unknown,
}

/// Configuration for building a Docker image
#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub tag: String,
    pub dockerfile: String,
    pub context: String,
    pub platform: Option<String>,
    pub build_args: HashMap<String, String>,
    pub target: Option<String>,
}

/// Configuration for running a container
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub image: String,
    pub name: String,
    pub network: Option<String>,
    pub env_vars: Vec<String>,
    pub ports: Vec<String>,
    pub volumes: Vec<String>,
    pub command: Option<Vec<String>>,
    pub detach: bool,
    pub remove: bool,
    pub privileged: bool,
    pub working_dir: Option<String>,
}

/// Unified Docker client trait combining all Docker operations
#[async_trait]
pub trait DockerClient: Send + Sync + fmt::Debug {
    // Network operations
    async fn create_network(&self, name: &str) -> Result<()>;
    async fn delete_network(&self, name: &str) -> Result<()>;
    async fn network_exists(&self, name: &str) -> Result<bool>;

    // Image operations
    async fn pull_image(&self, image: &str) -> Result<()>;
    async fn build_image(&self, config: BuildConfig) -> Result<()>;
    async fn image_exists(&self, image: &str) -> Result<bool>;
    async fn remove_image(&self, image: &str) -> Result<()>;

    // Container operations
    async fn run_container(&self, config: RunConfig) -> Result<String>;
    async fn stop_container(&self, container_id: &str) -> Result<()>;
    async fn remove_container(&self, container_id: &str) -> Result<()>;
    async fn container_status(&self, container_id: &str) -> Result<ContainerStatus>;
    async fn container_logs(
        &self,
        container_id: &str,
        follow: bool,
        since: Option<&str>,
    ) -> Result<String>;
    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String>;
    async fn get_container_by_name(&self, name: &str) -> Result<Option<String>>;
    async fn list_containers(&self, all: bool) -> Result<Vec<String>>;
    async fn inspect_container(&self, container_id: &str) -> Result<String>;

    // Utility operations
    async fn get_container_exit_code(&self, container_id: &str) -> Result<Option<i32>>;
    async fn wait_for_container(&self, container_id: &str) -> Result<i32>;
}
