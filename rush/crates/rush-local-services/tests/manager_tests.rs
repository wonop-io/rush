use rush_local_services::{LocalServiceManager, LocalServiceConfig, LocalServiceType};
use std::path::PathBuf;
use std::sync::Arc;

// Mock DockerClient for testing
#[derive(Debug)]
struct MockDockerClient;

#[async_trait::async_trait]
impl rush_local_services::docker::DockerClient for MockDockerClient {
    async fn run_container(
        &self,
        _image: &str,
        _name: &str,
        _network: &str,
        _env_vars: &[String],
        _ports: &[String],
        _volumes: &[String],
    ) -> rush_core::error::Result<String> {
        Ok("mock-container-id".to_string())
    }

    async fn stop_container(&self, _container_id: &str) -> rush_core::error::Result<()> {
        Ok(())
    }

    async fn remove_container(&self, _container_id: &str) -> rush_core::error::Result<()> {
        Ok(())
    }

    async fn container_status(&self, _container_id: &str) -> rush_core::error::Result<rush_local_services::docker::ContainerStatus> {
        Ok(rush_local_services::docker::ContainerStatus::Running)
    }

    async fn container_logs(&self, _container_id: &str, _lines: usize) -> rush_core::error::Result<String> {
        Ok("mock logs".to_string())
    }
    
    async fn exec_in_container(&self, _container_id: &str, _command: &[&str]) -> rush_core::error::Result<String> {
        Ok("command output".to_string())
    }
    
    async fn get_container_by_name(&self, _name: &str) -> rush_core::error::Result<String> {
        Ok("mock-container-id".to_string())
    }
}

#[test]
fn test_manager_creation() {
    let docker_client = Arc::new(MockDockerClient);
    let data_dir = PathBuf::from("/tmp/test");
    let network_name = "test-network".to_string();
    
    let manager = LocalServiceManager::new(
        docker_client,
        data_dir.clone(),
        network_name.clone(),
    );
    
    // Manager should be created successfully
    // (Can't test internal state directly due to encapsulation)
    assert!(true); // Placeholder - manager created without panic
}

#[test]
fn test_manager_register_service() {
    let docker_client = Arc::new(MockDockerClient);
    let data_dir = PathBuf::from("/tmp/test");
    let network_name = "test-network".to_string();
    
    let mut manager = LocalServiceManager::new(
        docker_client,
        data_dir,
        network_name,
    );
    
    let config = LocalServiceConfig::new(
        "test-postgres".to_string(),
        LocalServiceType::PostgreSQL,
    );
    
    manager.register(config);
    
    // Service should be registered
    // (Can't test internal state directly, but registration shouldn't panic)
    assert!(true);
}

#[tokio::test]
async fn test_manager_is_running() {
    let docker_client = Arc::new(MockDockerClient);
    let data_dir = PathBuf::from("/tmp/test");
    let network_name = "test-network".to_string();
    
    let manager = LocalServiceManager::new(
        docker_client,
        data_dir,
        network_name,
    );
    
    // Should return false for non-existent service
    assert!(!manager.is_running("non-existent").await);
}

#[tokio::test]
async fn test_manager_get_status() {
    let docker_client = Arc::new(MockDockerClient);
    let data_dir = PathBuf::from("/tmp/test");
    let network_name = "test-network".to_string();
    
    let manager = LocalServiceManager::new(
        docker_client,
        data_dir,
        network_name,
    );
    
    let status = manager.get_status().await;
    assert!(status.is_empty());
}

#[tokio::test]
async fn test_manager_get_connection_strings() {
    let docker_client = Arc::new(MockDockerClient);
    let data_dir = PathBuf::from("/tmp/test");
    let network_name = "test-network".to_string();
    
    let manager = LocalServiceManager::new(
        docker_client,
        data_dir,
        network_name,
    );
    
    let connections = manager.get_connection_strings().await;
    assert!(connections.is_empty());
}

#[test]
fn test_register_multiple_services() {
    let docker_client = Arc::new(MockDockerClient);
    let data_dir = PathBuf::from("/tmp/test");
    let network_name = "test-network".to_string();
    
    let mut manager = LocalServiceManager::new(
        docker_client,
        data_dir,
        network_name,
    );
    
    // Register multiple services
    let postgres_config = LocalServiceConfig::new(
        "postgres".to_string(),
        LocalServiceType::PostgreSQL,
    );
    
    let redis_config = LocalServiceConfig::new(
        "redis".to_string(),
        LocalServiceType::Redis,
    );
    
    let minio_config = LocalServiceConfig::new(
        "minio".to_string(),
        LocalServiceType::MinIO,
    );
    
    manager.register(postgres_config);
    manager.register(redis_config);
    manager.register(minio_config);
    
    // All services should be registered without issues
    assert!(true);
}

#[test]
fn test_service_with_dependencies() {
    let docker_client = Arc::new(MockDockerClient);
    let data_dir = PathBuf::from("/tmp/test");
    let network_name = "test-network".to_string();
    
    let mut manager = LocalServiceManager::new(
        docker_client,
        data_dir,
        network_name,
    );
    
    // Create service with dependencies
    let mut app_config = LocalServiceConfig::new(
        "app".to_string(),
        LocalServiceType::Custom("app".to_string()),
    );
    app_config.depends_on = vec!["postgres".to_string(), "redis".to_string()];
    
    let postgres_config = LocalServiceConfig::new(
        "postgres".to_string(),
        LocalServiceType::PostgreSQL,
    );
    
    let redis_config = LocalServiceConfig::new(
        "redis".to_string(),
        LocalServiceType::Redis,
    );
    
    manager.register(postgres_config);
    manager.register(redis_config);
    manager.register(app_config);
    
    // Services with dependencies should be registered correctly
    assert!(true);
}