use crate::rush_cli::vault::{DotenvVault, FileVault, OnePassword, Vault};
use futures::future::join_all;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::runtime::Runtime;

// Manual implementation of a test vault that can be configured with expected responses
#[derive(Clone)]
struct TestVault {
    get_result: Option<HashMap<String, String>>,
    set_result: bool,  // Just indicates if set operation succeeds
    create_result: bool, // Just indicates if create operation succeeds
    remove_result: bool, // Just indicates if remove operation succeeds
    exists_result: Option<bool>, // None means error, Some means success with value
}

// Mark TestVault as thread-safe
unsafe impl Send for TestVault {}
unsafe impl Sync for TestVault {}

impl TestVault {
    fn new() -> Self {
        Self {
            get_result: Some(HashMap::new()),
            set_result: true,
            create_result: true,
            remove_result: true,
            exists_result: Some(true),
        }
    }
    
    fn with_get_result(mut self, result: Option<HashMap<String, String>>) -> Self {
        self.get_result = result;
        self
    }
    
    fn with_exists_result(mut self, result: Option<bool>) -> Self {
        self.exists_result = result;
        self
    }
}

#[async_trait::async_trait]
impl Vault for TestVault {
    async fn get(
        &self,
        _product_name: &str,
        _component_name: &str,
        _environment: &str,
    ) -> Result<HashMap<String, String>, Box<dyn Error>> {
        match &self.get_result {
            Some(map) => Ok(map.clone()),
            None => Err(Box::new(std::io::Error::new(std::io::ErrorKind::NotFound, "Not found"))),
        }
    }
    
    async fn set(
        &mut self,
        _product_name: &str,
        _component_name: &str,
        _environment: &str,
        _secrets: HashMap<String, String>,
    ) -> Result<(), Box<dyn Error>> {
        if self.set_result {
            Ok(())
        } else {
            Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Set failed")))
        }
    }
    
    async fn create_vault(&mut self, _product_name: &str) -> Result<(), Box<dyn Error>> {
        if self.create_result {
            Ok(())
        } else {
            Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Create failed")))
        }
    }
    
    async fn remove(
        &mut self,
        _product_name: &str,
        _component_name: &str,
        _environment: &str,
    ) -> Result<(), Box<dyn Error>> {
        if self.remove_result {
            Ok(())
        } else {
            Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Remove failed")))
        }
    }
    
    async fn check_if_vault_exists(&self, _product_name: &str) -> Result<bool, Box<dyn Error>> {
        match self.exists_result {
            Some(exists) => Ok(exists),
            None => Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Check failed")))
        }
    }
}

// Helper function to create a stack.spec.yaml file for DotenvVault tests
fn create_stack_spec_yaml(dir: &Path, components: &[&str]) -> io::Result<()> {
    let yaml_path = dir.join("stack.spec.yaml");
    let mut yaml_content = String::new();
    
    for component in components {
        yaml_content.push_str(&format!(
            r#"{}:
  path: {}
"#,
            component,
            dir.join(component).to_string_lossy()
        ));
        
        // Create component directory
        fs::create_dir_all(dir.join(component))?;
    }
    
    let mut file = File::create(yaml_path)?;
    file.write_all(yaml_content.as_bytes())?;
    Ok(())
}

// Helper function to create a .env file with specific content
fn create_env_file(path: &Path, env_vars: &HashMap<String, String>) -> io::Result<()> {
    let mut content = String::new();
    for (key, value) in env_vars {
        content.push_str(&format!("{}={}\n", key, value));
    }
    
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

#[test]
fn test_test_vault() {
    // This test demonstrates how to use the TestVault
    let mut secrets = HashMap::new();
    secrets.insert("KEY1".to_string(), "VALUE1".to_string());
    
    let test_vault = TestVault::new()
        .with_get_result(Some(secrets.clone()))
        .with_exists_result(Some(true));
    
    let product = "test_product";
    let component = "test_component";
    let env = "test_env";
    
    // Run the test with our prepared test vault
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        let exists = test_vault.check_if_vault_exists(product).await.unwrap();
        assert!(exists);
        
        let result = test_vault.get(product, component, env).await.unwrap();
        assert_eq!(result.get("KEY1"), Some(&"VALUE1".to_string()));
    });
}

#[test]
fn test_dotenv_vault_basic() {
    let temp_dir = TempDir::new().unwrap();
    let components = ["component1", "component2"];
    
    // Create a stack.spec.yaml file
    create_stack_spec_yaml(temp_dir.path(), &components).unwrap();
    
    // Create test .env files for component1
    let mut env_vars = HashMap::new();
    env_vars.insert("DB_HOST".to_string(), "localhost".to_string());
    env_vars.insert("DB_PORT".to_string(), "5432".to_string());
    
    create_env_file(&temp_dir.path().join("component1").join(".env"), &env_vars).unwrap();
    
    // Create the DotenvVault instance
    let dotenv_vault = DotenvVault::new(temp_dir.path().to_path_buf());
    
    // Test with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Test getting existing secrets
        let result = dotenv_vault.get("any", "component1", "any").await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("DB_HOST"), Some(&"localhost".to_string()));
        assert_eq!(result.get("DB_PORT"), Some(&"5432".to_string()));
        
        // Test getting non-existent component
        let result = dotenv_vault.get("any", "nonexistent", "any").await;
        assert!(result.is_err());
    });
}

#[test]
fn test_dotenv_vault_set_get_cycle() {
    let temp_dir = TempDir::new().unwrap();
    let components = ["component1"];
    
    // Create a stack.spec.yaml file
    create_stack_spec_yaml(temp_dir.path(), &components).unwrap();
    
    // Create the DotenvVault instance
    let mut dotenv_vault = DotenvVault::new(temp_dir.path().to_path_buf());
    
    // Test with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Set some secrets
        let mut secrets = HashMap::new();
        secrets.insert("API_KEY".to_string(), "12345".to_string());
        secrets.insert("SECRET_TOKEN".to_string(), "abcdef".to_string());
        
        // Set the secrets
        dotenv_vault.set("any", "component1", "any", secrets.clone()).await.unwrap();
        
        // Verify the .env file was created
        let env_path = temp_dir.path().join("component1").join(".env");
        assert!(env_path.exists());
        
        // Retrieve and verify the secrets
        let retrieved = dotenv_vault.get("any", "component1", "any").await.unwrap();
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved.get("API_KEY"), Some(&"12345".to_string()));
        assert_eq!(retrieved.get("SECRET_TOKEN"), Some(&"abcdef".to_string()));
    });
}

#[test]
fn test_file_vault_comprehensive() {
    let temp_dir = TempDir::new().unwrap();
    let mut vault = FileVault::new(temp_dir.path().to_path_buf(), None);
    
    // Test data
    let product_name = "test_product";
    let component_names = ["component1", "component2", "component3"];
    let environments = ["dev", "staging", "prod"];
    
    // Setup test environment with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Test vault creation
        let create_result = vault.create_vault(product_name).await;
        assert!(create_result.is_ok());
        
        // Verify vault exists
        let exists = vault.check_if_vault_exists(product_name).await.unwrap();
        assert!(exists);
        
        // Add secrets for different components and environments
        let mut futures = Vec::new();
        
        for &component in component_names.iter() {
            for &env in environments.iter() {
                let mut secrets = HashMap::new();
                secrets.insert(
                    format!("KEY_{}", component),
                    format!("VALUE_{}_{}", component, env),
                );
                
                let future = vault.set(
                    product_name,
                    component,
                    env,
                    secrets.clone(),
                );
                futures.push(future);
            }
        }
        
        // Wait for all set operations to complete
        let results = join_all(futures).await;
        for result in results {
            assert!(result.is_ok());
        }
        
        // Verify secrets for each component and environment
        for &component in component_names.iter() {
            for &env in environments.iter() {
                let retrieved = vault.get(product_name, component, env).await.unwrap();
                assert_eq!(retrieved.len(), 1);
                assert_eq!(
                    retrieved.get(&format!("KEY_{}", component)),
                    Some(&format!("VALUE_{}_{}", component, env))
                );
            }
        }
        
        // Test removing secrets for one component
        vault.remove(product_name, "component1", "dev").await.unwrap();
        
        // Verify that specific secret is gone
        let empty = vault.get(product_name, "component1", "dev").await.unwrap();
        assert_eq!(empty.len(), 0);
        
        // But other secrets still exist
        let still_exist = vault.get(product_name, "component1", "staging").await.unwrap();
        assert_eq!(still_exist.len(), 1);
    });
}

#[test]
fn test_one_password_simulator() {
    // This test simulates the OnePassword implementation since we can't actually
    // call the 1Password CLI in tests
    
    // Create a test implementation instead of the actual OP command
    let mut secrets = HashMap::new();
    secrets.insert("OP_KEY".to_string(), "OP_VALUE".to_string());
    
    let test_vault = TestVault::new()
        .with_get_result(Some(secrets))
        .with_exists_result(Some(true));
    
    // Run with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Verify our test vault works as expected
        assert!(test_vault.check_if_vault_exists("any").await.unwrap());
        
        let retrieved = test_vault.get("any", "any", "any").await.unwrap();
        assert_eq!(retrieved.get("OP_KEY"), Some(&"OP_VALUE".to_string()));
    });
    
    // In reality, we would test OnePassword with:
    // let one_password = OnePassword::new("account_email");
    // But this requires the 1Password CLI to be installed and configured
}

// Test all vault implementations with the same scenarios
#[test]
fn test_all_vault_implementations_basic_operations() {
    let temp_dir = TempDir::new().unwrap();
    let components = ["test_component"];
    
    // Create test structures for DotenvVault
    create_stack_spec_yaml(temp_dir.path(), &components).unwrap();
    
    // Create test .env files
    let env_dir = temp_dir.path().join("test_component");
    
    // Create vault implementations
    let mut dotenv_vault = DotenvVault::new(temp_dir.path().to_path_buf());
    let mut file_vault = FileVault::new(temp_dir.path().join("secrets"), None);
    
    // Define test data
    let product = "test_product";
    let component = "test_component";
    let env = "test_env";
    let mut secrets = HashMap::new();
    secrets.insert("TEST_KEY".to_string(), "TEST_VALUE".to_string());
    
    // Test with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Test FileVault
        file_vault.create_vault(product).await.unwrap();
        file_vault.set(product, component, env, secrets.clone()).await.unwrap();
        let file_result = file_vault.get(product, component, env).await.unwrap();
        assert_eq!(file_result.get("TEST_KEY"), Some(&"TEST_VALUE".to_string()));
        
        // Test DotenvVault
        dotenv_vault.set(product, component, env, secrets.clone()).await.unwrap();
        let dotenv_result = dotenv_vault.get(product, component, env).await.unwrap();
        assert_eq!(dotenv_result.get("TEST_KEY"), Some(&"TEST_VALUE".to_string()));
    });
}

// Test error handling scenarios
#[test]
fn test_vault_error_handling() {
    let temp_dir = TempDir::new().unwrap();
    
    // Deliberately NOT creating the stack.spec.yaml file to test error handling
    let dotenv_vault = DotenvVault::new(temp_dir.path().to_path_buf());
    
    // Test with runtime
    let rt = Runtime::new().unwrap();
    
    rt.block_on(async {
        // Should error due to missing stack.spec.yaml
        let result = dotenv_vault.get("any", "component1", "any").await;
        assert!(result.is_err());
    });
    
    // Test FileVault with non-existent paths
    let file_vault = FileVault::new(PathBuf::from("/non/existent/path"), None);
    
    rt.block_on(async {
        let result = file_vault.get("any", "any", "any").await;
        // This should succeed with empty map since it creates empty JSON if file doesn't exist
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    });
}