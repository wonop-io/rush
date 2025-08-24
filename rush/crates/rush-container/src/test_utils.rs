//! Test utilities for the container crate
//!
//! This module provides mock implementations and test helpers
//! for testing container lifecycle management.

#[cfg(test)]
pub mod mocks {
    use async_trait::async_trait;
    use mockall::mock;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    
    use crate::docker::{ContainerStatus, DockerClient, DockerService};
    use rush_core::error::Result;
    
    // Mock Docker client
    mock! {
        pub DockerClient {}
        
        #[async_trait]
        impl DockerClient for DockerClient {
            async fn container_exists(&self, name: &str) -> Result<bool>;
            async fn create_container(
                &self,
                name: &str,
                image: &str,
                env: Vec<String>,
                volumes: Vec<String>,
                network: Option<String>,
                ports: Vec<(u16, u16)>,
            ) -> Result<String>;
            async fn start_container(&self, id: &str) -> Result<()>;
            async fn stop_container(&self, id: &str, timeout: Option<u64>) -> Result<()>;
            async fn remove_container(&self, id: &str, force: bool) -> Result<()>;
            async fn container_status(&self, id: &str) -> Result<ContainerStatus>;
            async fn container_logs(&self, id: &str, follow: bool) -> Result<String>;
            async fn network_exists(&self, name: &str) -> Result<bool>;
            async fn create_network(&self, name: &str) -> Result<()>;
            async fn remove_network(&self, name: &str) -> Result<()>;
            async fn build_image(
                &self,
                context: &str,
                dockerfile: &str,
                tag: &str,
                build_args: Vec<(String, String)>,
            ) -> Result<()>;
            async fn image_exists(&self, tag: &str) -> Result<bool>;
            async fn pull_image(&self, image: &str) -> Result<()>;
            async fn push_image(&self, image: &str) -> Result<()>;
        }
    }
    
    // Mock file system watcher
    mock! {
        pub FileWatcher {}
        
        impl Clone for FileWatcher {
            fn clone(&self) -> Self;
        }
    }
    
    // Mock kubectl client
    mock! {
        pub KubectlClient {}
        
        impl Clone for KubectlClient {
            fn clone(&self) -> Self;
        }
        
        #[async_trait]
        impl crate::kubernetes::KubernetesClient for KubectlClient {
            async fn apply_manifest(&self, manifest: &str) -> Result<()>;
            async fn delete_manifest(&self, manifest: &str) -> Result<()>;
            async fn get_pods(&self, namespace: &str) -> Result<Vec<String>>;
            async fn get_services(&self, namespace: &str) -> Result<Vec<String>>;
            async fn rollout_status(&self, deployment: &str, namespace: &str) -> Result<String>;
            async fn set_context(&self, context: &str) -> Result<()>;
            async fn current_context(&self) -> Result<String>;
        }
    }
    
    // Test container for integration tests
    pub struct TestContainer {
        pub name: String,
        pub image: String,
        pub status: ContainerStatus,
        pub env: Vec<String>,
        pub ports: Vec<(u16, u16)>,
    }
    
    impl TestContainer {
        pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
            Self {
                name: name.into(),
                image: image.into(),
                status: ContainerStatus::Created,
                env: Vec::new(),
                ports: Vec::new(),
            }
        }
        
        pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
            self.env.push(format!("{}={}", key.into(), value.into()));
            self
        }
        
        pub fn with_port(mut self, host: u16, container: u16) -> Self {
            self.ports.push((host, container));
            self
        }
        
        pub fn running(mut self) -> Self {
            self.status = ContainerStatus::Running;
            self
        }
    }
    
    // In-memory Docker client for testing
    pub struct InMemoryDockerClient {
        containers: Arc<Mutex<HashMap<String, TestContainer>>>,
        networks: Arc<Mutex<Vec<String>>>,
        images: Arc<Mutex<Vec<String>>>,
    }
    
    impl InMemoryDockerClient {
        pub fn new() -> Self {
            Self {
                containers: Arc::new(Mutex::new(HashMap::new())),
                networks: Arc::new(Mutex::new(Vec::new())),
                images: Arc::new(Mutex::new(Vec::new())),
            }
        }
        
        pub async fn add_container(&self, container: TestContainer) {
            let mut containers = self.containers.lock().await;
            containers.insert(container.name.clone(), container);
        }
        
        pub async fn add_image(&self, tag: impl Into<String>) {
            let mut images = self.images.lock().await;
            images.push(tag.into());
        }
    }
    
    #[async_trait]
    impl DockerClient for InMemoryDockerClient {
        async fn container_exists(&self, name: &str) -> Result<bool> {
            let containers = self.containers.lock().await;
            Ok(containers.contains_key(name))
        }
        
        async fn create_container(
            &self,
            name: &str,
            image: &str,
            env: Vec<String>,
            _volumes: Vec<String>,
            _network: Option<String>,
            ports: Vec<(u16, u16)>,
        ) -> Result<String> {
            let mut containers = self.containers.lock().await;
            let container = TestContainer {
                name: name.to_string(),
                image: image.to_string(),
                status: ContainerStatus::Created,
                env,
                ports,
            };
            containers.insert(name.to_string(), container);
            Ok(format!("container_{}", name))
        }
        
        async fn start_container(&self, id: &str) -> Result<()> {
            let mut containers = self.containers.lock().await;
            let name = id.strip_prefix("container_").unwrap_or(id);
            if let Some(container) = containers.get_mut(name) {
                container.status = ContainerStatus::Running;
            }
            Ok(())
        }
        
        async fn stop_container(&self, id: &str, _timeout: Option<u64>) -> Result<()> {
            let mut containers = self.containers.lock().await;
            let name = id.strip_prefix("container_").unwrap_or(id);
            if let Some(container) = containers.get_mut(name) {
                container.status = ContainerStatus::Exited(0);
            }
            Ok(())
        }
        
        async fn remove_container(&self, id: &str, _force: bool) -> Result<()> {
            let mut containers = self.containers.lock().await;
            let name = id.strip_prefix("container_").unwrap_or(id);
            containers.remove(name);
            Ok(())
        }
        
        async fn container_status(&self, id: &str) -> Result<ContainerStatus> {
            let containers = self.containers.lock().await;
            let name = id.strip_prefix("container_").unwrap_or(id);
            containers
                .get(name)
                .map(|c| c.status.clone())
                .ok_or_else(|| rush_core::error::Error::Docker(format!("Container {} not found", id)))
        }
        
        async fn container_logs(&self, _id: &str, _follow: bool) -> Result<String> {
            Ok("Test logs".to_string())
        }
        
        async fn network_exists(&self, name: &str) -> Result<bool> {
            let networks = self.networks.lock().await;
            Ok(networks.contains(&name.to_string()))
        }
        
        async fn create_network(&self, name: &str) -> Result<()> {
            let mut networks = self.networks.lock().await;
            networks.push(name.to_string());
            Ok(())
        }
        
        async fn remove_network(&self, name: &str) -> Result<()> {
            let mut networks = self.networks.lock().await;
            networks.retain(|n| n != name);
            Ok(())
        }
        
        async fn build_image(
            &self,
            _context: &str,
            _dockerfile: &str,
            tag: &str,
            _build_args: Vec<(String, String)>,
        ) -> Result<()> {
            let mut images = self.images.lock().await;
            images.push(tag.to_string());
            Ok(())
        }
        
        async fn image_exists(&self, tag: &str) -> Result<bool> {
            let images = self.images.lock().await;
            Ok(images.contains(&tag.to_string()))
        }
        
        async fn pull_image(&self, image: &str) -> Result<()> {
            let mut images = self.images.lock().await;
            images.push(image.to_string());
            Ok(())
        }
        
        async fn push_image(&self, _image: &str) -> Result<()> {
            Ok(())
        }
    }
}

#[cfg(test)]
pub mod fixtures {
    use rush_build::{BuildType, ComponentBuildSpec};
    use std::collections::HashMap;
    use std::path::PathBuf;
    
    /// Create a test component spec
    pub fn test_component_spec(name: &str, build_type: BuildType) -> ComponentBuildSpec {
        ComponentBuildSpec {
            build_type,
            product_name: "test-product".to_string(),
            component_name: name.to_string(),
            color: None,
            depends_on: vec![],
            artifact_dir: None,
            dockerfile: None,
            dockerfile_prod: None,
            location: format!("test/{}", name),
            dockerignore: None,
            artifacts: vec![],
            artifact_files: vec![],
            env_file: None,
            env_file_prod: None,
            environment: HashMap::new(),
            environment_prod: HashMap::new(),
            inject_stripe_secrets: false,
            local_ports: vec![],
            mount_point: None,
            mount_point_source: None,
            mount_point_port: None,
            params: HashMap::new(),
            restart: None,
            silenced: false,
            user: None,
            wait_for: None,
            cache_from: vec![],
        }
    }
    
    /// Create a frontend component spec
    pub fn frontend_spec() -> ComponentBuildSpec {
        test_component_spec("frontend", BuildType::TrunkWasm)
    }
    
    /// Create a backend component spec
    pub fn backend_spec() -> ComponentBuildSpec {
        test_component_spec("backend", BuildType::RustBinary {
            binary_name: "server".to_string(),
        })
    }
    
    /// Create test configuration
    pub fn test_config() -> crate::reactor::config::ContainerReactorConfig {
        crate::reactor::config::ContainerReactorConfig::new(
            "test-product",
            PathBuf::from("/test/path"),
            "test-network",
            "dev",
        )
        .with_verbose(true)
        .with_git_hash("test123")
    }
}

#[cfg(test)]
pub mod helpers {
    use std::time::Duration;
    
    /// Wait for a condition to become true
    pub async fn wait_for<F>(mut condition: F, timeout: Duration) -> bool
    where
        F: FnMut() -> bool,
    {
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            if condition() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }
    
    /// Assert that an async operation completes within a timeout
    pub async fn assert_completes_within<F, T>(future: F, timeout: Duration) -> T
    where
        F: std::future::Future<Output = T>,
    {
        tokio::time::timeout(timeout, future)
            .await
            .expect("Operation timed out")
    }
}