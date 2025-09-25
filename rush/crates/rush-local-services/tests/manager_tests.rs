use std::collections::HashMap;

use async_trait::async_trait;
use rush_local_services::{LocalService, LocalServiceManager, LocalServiceType};

// Mock LocalService for testing
#[derive(Debug)]
struct MockLocalService {
    name: String,
    service_type: LocalServiceType,
    is_running: bool,
    is_healthy: bool,
}

impl MockLocalService {
    fn new(name: String, service_type: LocalServiceType) -> Self {
        Self {
            name,
            service_type,
            is_running: false,
            is_healthy: false,
        }
    }
}

#[async_trait]
impl LocalService for MockLocalService {
    async fn start(&mut self) -> rush_core::error::Result<()> {
        self.is_running = true;
        Ok(())
    }

    async fn stop(&mut self) -> rush_core::error::Result<()> {
        self.is_running = false;
        self.is_healthy = false;
        Ok(())
    }

    async fn is_healthy(&self) -> rush_core::error::Result<bool> {
        Ok(self.is_healthy)
    }

    async fn generated_env_vars(&self) -> rush_core::error::Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    async fn generated_env_secrets(&self) -> rush_core::error::Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    async fn run_post_startup_tasks(&mut self) -> rush_core::error::Result<()> {
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn service_type(&self) -> LocalServiceType {
        self.service_type.clone()
    }

    fn is_running(&self) -> bool {
        self.is_running
    }
}

#[test]
fn test_manager_creation() {
    let _manager = LocalServiceManager::new();
    // Test passes if manager is created without panic
}

#[test]
fn test_manager_register_service() {
    let mut manager = LocalServiceManager::new();

    let service = MockLocalService::new("test-postgres".to_string(), LocalServiceType::PostgreSQL);
    manager.register(Box::new(service));

    // Test passes if service is registered without panic
}

#[test]
fn test_manager_is_running() {
    let manager = LocalServiceManager::new();

    // Should return false for non-existent service
    assert!(!manager.is_service_running("non-existent"));
}

#[tokio::test]
async fn test_manager_get_status() {
    let mut manager = LocalServiceManager::new();

    let service = MockLocalService::new("test-postgres".to_string(), LocalServiceType::PostgreSQL);
    manager.register(Box::new(service));

    let status = manager.get_status().await;
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].0, "test-postgres");
    assert!(!status[0].1); // Not healthy yet
}

#[test]
fn test_register_multiple_services() {
    let mut manager = LocalServiceManager::new();

    let postgres = MockLocalService::new("postgres".to_string(), LocalServiceType::PostgreSQL);
    let redis = MockLocalService::new("redis".to_string(), LocalServiceType::Redis);
    let minio = MockLocalService::new("minio".to_string(), LocalServiceType::MinIO);

    manager.register(Box::new(postgres));
    manager.register(Box::new(redis));
    manager.register(Box::new(minio));

    // Test passes if all services are registered without panic
}

#[test]
fn test_service_with_dependencies() {
    let mut manager = LocalServiceManager::new();

    // Create services with dependencies
    let postgres = MockLocalService::new("postgres".to_string(), LocalServiceType::PostgreSQL);
    let redis = MockLocalService::new("redis".to_string(), LocalServiceType::Redis);
    let app = MockLocalService::new(
        "app".to_string(),
        LocalServiceType::Custom("app".to_string()),
    );

    manager.register(Box::new(postgres));
    manager.register(Box::new(redis));
    manager.register(Box::new(app));

    // Test passes if services with dependencies are registered
}

#[test]
fn test_manager_get_connection_strings() {
    let manager = LocalServiceManager::new();

    // Get environment variables (should be empty initially)
    let env_vars = manager.get_env_vars();
    assert_eq!(env_vars.len(), 0);

    // Get secrets (should be empty initially)
    let secrets = manager.get_env_secrets();
    assert_eq!(secrets.len(), 0);
}
