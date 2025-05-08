use crate::rush_cli::vault::{Environment, SecretError, SecretsProvider};
use crate::rush_cli::vault::vault_adapter::{VaultAdapter, create_vault_provider};
use crate::rush_cli::vault::Vault;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;
use async_trait::async_trait;

// Manual implementation of a test SecretsProvider
#[derive(Clone)]
struct TestSecretsProvider {
    get_result: Result<HashMap<String, String>, SecretError>,
    set_result: bool,
    delete_result: bool,
}

// Mark TestSecretsProvider as thread-safe
unsafe impl Send for TestSecretsProvider {}
unsafe impl Sync for TestSecretsProvider {}

impl Debug for TestSecretsProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestSecretsProvider").finish()
    }
}

impl TestSecretsProvider {
    fn new() -> Self {
        Self {
            get_result: Ok(HashMap::new()),
            set_result: true,
            delete_result: true,
        }
    }
    
    fn with_get_result(mut self, result: Result<HashMap<String, String>, SecretError>) -> Self {
        self.get_result = result;
        self
    }
}

#[async_trait]
impl SecretsProvider for TestSecretsProvider {
    async fn get_secrets(
        &self,
        _product_name: &str,
        _component_name: &str,
        _environment: &Environment,
    ) -> Result<HashMap<String, String>, SecretError> {
        self.get_result.clone()
    }
    
    async fn set_secrets(
        &mut self,
        _product_name: &str,
        _component_name: &str,
        _environment: &Environment,
        _secrets: HashMap<String, String>,
    ) -> Result<(), SecretError> {
        if self.set_result {
            Ok(())
        } else {
            Err(SecretError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other, 
                "Set failed"
            ))))
        }
    }
    
    async fn delete_all_secrets(
        &mut self,
        _product_name: &str,
        _component_name: &str,
        _environment: &Environment,
    ) -> Result<(), SecretError> {
        if self.delete_result {
            Ok(())
        } else {
            Err(SecretError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other, 
                "Delete failed"
            ))))
        }
    }
}

// A simple in-memory implementation of the Vault trait for testing
struct InMemoryVault {
    secrets: HashMap<String, HashMap<String, String>>,
}

impl InMemoryVault {
    fn new() -> Self {
        Self {
            secrets: HashMap::new(),
        }
    }
    
    fn get_key(&self, product: &str, component: &str, env: &str) -> String {
        format!("{}:{}:{}", product, component, env)
    }
}

#[async_trait]
impl Vault for InMemoryVault {
    async fn get(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let key = self.get_key(product_name, component_name, environment);
        match self.secrets.get(&key) {
            Some(secrets) => Ok(secrets.clone()),
            None => Ok(HashMap::new()), // Return empty map for non-existent keys
        }
    }
    
    async fn set(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
        secrets: HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let key = self.get_key(product_name, component_name, environment);
        self.secrets.insert(key, secrets);
        Ok(())
    }
    
    async fn create_vault(&mut self, _product_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // No-op for in-memory implementation
        Ok(())
    }
    
    async fn remove(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let key = self.get_key(product_name, component_name, environment);
        self.secrets.remove(&key);
        Ok(())
    }
    
    async fn check_if_vault_exists(&self, _product_name: &str) -> Result<bool, Box<dyn std::error::Error>> {
        // Always exists for in-memory implementation
        Ok(true)
    }
}

#[test]
fn test_environment_conversions() {
    // Test string to Environment conversion
    assert_eq!(Environment::from("dev"), Environment::Development);
    assert_eq!(Environment::from("development"), Environment::Development);
    assert_eq!(Environment::from("test"), Environment::Testing);
    assert_eq!(Environment::from("testing"), Environment::Testing);
    assert_eq!(Environment::from("stage"), Environment::Staging);
    assert_eq!(Environment::from("staging"), Environment::Staging);
    assert_eq!(Environment::from("prod"), Environment::Production);
    assert_eq!(Environment::from("production"), Environment::Production);
    assert_eq!(Environment::from("custom"), Environment::Custom("custom".to_string()));
    
    // Test Environment to string conversion
    assert_eq!(Environment::Development.to_string(), "development");
    assert_eq!(Environment::Testing.to_string(), "testing");
    assert_eq!(Environment::Staging.to_string(), "staging");
    assert_eq!(Environment::Production.to_string(), "production");
    assert_eq!(Environment::Custom("custom".to_string()).to_string(), "custom");
}

#[test]
fn test_vault_adapter_with_in_memory_vault() {
    let in_memory_vault = InMemoryVault::new();
    let mut adapter = VaultAdapter::new(in_memory_vault);
    
    // Test data
    let product = "test_product";
    let component = "test_component";
    let environment = Environment::Development;
    
    let mut secrets = HashMap::new();
    secrets.insert("key1".to_string(), "value1".to_string());
    secrets.insert("key2".to_string(), "value2".to_string());
    
    // Run test with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Test set_secrets
        adapter.set_secrets(product, component, &environment, secrets.clone())
            .await
            .expect("Failed to set secrets");
        
        // Test get_secrets
        let retrieved = adapter.get_secrets(product, component, &environment)
            .await
            .expect("Failed to get secrets");
        
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved.get("key1"), Some(&"value1".to_string()));
        assert_eq!(retrieved.get("key2"), Some(&"value2".to_string()));
        
        // Test get_secret (default implementation)
        let key1_value = adapter.get_secret(product, component, &environment, "key1")
            .await
            .expect("Failed to get specific secret");
        
        assert_eq!(key1_value, "value1");
        
        // Test has_secret (default implementation)
        let has_key1 = adapter.has_secret(product, component, &environment, "key1")
            .await
            .expect("Failed to check if secret exists");
        
        assert!(has_key1);
        
        // Test non-existent key
        let has_key3 = adapter.has_secret(product, component, &environment, "key3")
            .await
            .expect("Failed to check if secret exists");
        
        assert!(!has_key3);
        
        // Test set_secret (default implementation)
        adapter.set_secret(product, component, &environment, "key3", "value3")
            .await
            .expect("Failed to set specific secret");
        
        // Verify the new secret was added
        let has_key3_after = adapter.has_secret(product, component, &environment, "key3")
            .await
            .expect("Failed to check if secret exists");
        
        assert!(has_key3_after);
        
        // Test delete_secret (default implementation)
        adapter.delete_secret(product, component, &environment, "key1")
            .await
            .expect("Failed to delete secret");
        
        // Verify key1 was deleted
        let has_key1_after = adapter.has_secret(product, component, &environment, "key1")
            .await
            .expect("Failed to check if secret exists");
        
        assert!(!has_key1_after);
        
        // Test delete_all_secrets
        adapter.delete_all_secrets(product, component, &environment)
            .await
            .expect("Failed to delete all secrets");
        
        // Verify all secrets were deleted
        let empty_secrets = adapter.get_secrets(product, component, &environment)
            .await
            .expect("Failed to get secrets after deletion");
        
        assert_eq!(empty_secrets.len(), 0);
    });
}

#[test]
fn test_vault_adapter_factory() {
    let in_memory_vault = InMemoryVault::new();
    let provider = create_vault_provider(in_memory_vault);
    
    // Run test with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Just verify we can call methods on the Arc<dyn SecretsProvider> returned by factory
        let secrets = provider.get_secrets("test", "test", &Environment::Development)
            .await
            .expect("Failed to get secrets from provider created by factory");
        
        assert_eq!(secrets.len(), 0);
    });
}

#[test]
fn test_test_secrets_provider() {
    // Create a test provider with predefined responses
    let mut return_secrets = HashMap::new();
    return_secrets.insert("mock_key".to_string(), "mock_value".to_string());
    
    let test_provider = TestSecretsProvider::new()
        .with_get_result(Ok(return_secrets.clone()));
    
    // Run test with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Test our provider returns expected values
        let retrieved = test_provider.get_secrets("test", "test", &Environment::Development)
            .await
            .expect("Failed to get secrets from test provider");
        
        assert_eq!(retrieved.get("mock_key"), Some(&"mock_value".to_string()));
        
        // Create a mutable clone of our test provider for set operations
        let mut mutable_provider = test_provider.clone();
        
        // Set some secrets
        let mut new_secrets = HashMap::new();
        new_secrets.insert("new_key".to_string(), "new_value".to_string());
        
        mutable_provider.set_secrets("test", "test", &Environment::Development, new_secrets)
            .await
            .expect("Failed to set secrets in test provider");
            
        // Our test provider will still return what we configured it to return
        let still_same = test_provider.get_secrets("test", "test", &Environment::Development)
            .await
            .expect("Failed to get secrets from test provider after setting");
            
        assert_eq!(still_same.get("mock_key"), Some(&"mock_value".to_string()));
    });
}

#[test]
fn test_error_handling() {
    // Create test providers that return errors
    let not_found_provider = TestSecretsProvider::new()
        .with_get_result(Err(SecretError::NotFound("Test error".to_string())));
        
    let access_denied_provider = TestSecretsProvider::new()
        .with_get_result(Err(SecretError::AccessDenied("No permission".to_string())));
    
    // Run test with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Test NotFound error
        let result = not_found_provider.get_secrets("error", "test", &Environment::Development).await;
        assert!(result.is_err());
        match result {
            Err(SecretError::NotFound(_)) => (),
            _ => panic!("Expected NotFound error"),
        }
        
        // Test AccessDenied error
        let result = access_denied_provider.get_secrets("access_denied", "test", &Environment::Development).await;
        assert!(result.is_err());
        match result {
            Err(SecretError::AccessDenied(_)) => (),
            _ => panic!("Expected AccessDenied error"),
        }
        
        // Test default implementation with errors
        let result = not_found_provider.get_secret("error", "test", &Environment::Development, "key").await;
        assert!(result.is_err());
        
        let result = not_found_provider.has_secret("error", "test", &Environment::Development, "key").await;
        assert!(result.is_err());
    });
}