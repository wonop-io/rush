use crate::*;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_file_vault_create() {
        let temp_dir = TempDir::new().unwrap();
        let mut vault = vault::FileVault::new(temp_dir.path().to_path_buf(), None);
        
        // Test creating a vault
        let product_name = "test_product";
        vault.create_vault(product_name).await.unwrap();
        
        // Verify vault directory exists
        let vault_dir = temp_dir.path().join(product_name);
        assert!(vault_dir.exists());
        assert!(vault_dir.is_dir());
        
        // Test check_if_vault_exists
        let exists = vault.check_if_vault_exists(product_name).await.unwrap();
        assert!(exists);
        
        // Test non-existent vault
        let non_existent = vault.check_if_vault_exists("non_existent").await.unwrap();
        assert!(!non_existent);
    }

    #[tokio::test]
    async fn test_file_vault_set_get() {
        let temp_dir = TempDir::new().unwrap();
        let mut vault = vault::FileVault::new(temp_dir.path().to_path_buf(), None);
        
        // Test data
        let product_name = "test_product";
        let component_name = "test_component";
        let environment = "test_env";
        let mut secrets = HashMap::new();
        secrets.insert("key1".to_string(), "value1".to_string());
        secrets.insert("key2".to_string(), "value2".to_string());
        
        // Create vault and set secrets
        vault.create_vault(product_name).await.unwrap();
        vault.set(product_name, component_name, environment, secrets.clone()).await.unwrap();
        
        // Verify secrets file exists
        let secrets_file = temp_dir.path().join(product_name).join(format!("{}.json", environment));
        assert!(secrets_file.exists());
        
        // Get secrets and verify values
        let retrieved_secrets = vault.get(product_name, component_name, environment).await.unwrap();
        assert_eq!(retrieved_secrets.len(), 2);
        assert_eq!(retrieved_secrets.get("key1"), Some(&"value1".to_string()));
        assert_eq!(retrieved_secrets.get("key2"), Some(&"value2".to_string()));
    }

    #[tokio::test]
    async fn test_file_vault_remove() {
        let temp_dir = TempDir::new().unwrap();
        let mut vault = vault::FileVault::new(temp_dir.path().to_path_buf(), None);
        
        // Test data
        let product_name = "test_product";
        let component_name = "test_component";
        let environment = "test_env";
        let mut secrets = HashMap::new();
        secrets.insert("key1".to_string(), "value1".to_string());
        
        // Create vault and set secrets
        vault.create_vault(product_name).await.unwrap();
        vault.set(product_name, component_name, environment, secrets.clone()).await.unwrap();
        
        // Verify secrets exist initially
        let retrieved_secrets = vault.get(product_name, component_name, environment).await.unwrap();
        assert_eq!(retrieved_secrets.len(), 1);
        
        // Remove secrets
        vault.remove(product_name, component_name, environment).await.unwrap();
        
        // Verify secrets no longer exist for the component
        let empty_secrets = vault.get(product_name, component_name, environment).await.unwrap();
        assert_eq!(empty_secrets.len(), 0);
    }

    #[tokio::test]
    async fn test_file_vault_multiple_components() {
        let temp_dir = TempDir::new().unwrap();
        let mut vault = vault::FileVault::new(temp_dir.path().to_path_buf(), None);
        
        // Test data
        let product_name = "test_product";
        let component1 = "component1";
        let component2 = "component2";
        let environment = "test_env";
        
        // Create component1 secrets
        let mut secrets1 = HashMap::new();
        secrets1.insert("key1".to_string(), "value1".to_string());
        
        // Create component2 secrets
        let mut secrets2 = HashMap::new();
        secrets2.insert("key2".to_string(), "value2".to_string());
        
        // Create vault and set secrets for both components
        vault.create_vault(product_name).await.unwrap();
        vault.set(product_name, component1, environment, secrets1.clone()).await.unwrap();
        vault.set(product_name, component2, environment, secrets2.clone()).await.unwrap();
        
        // Verify secrets for component1
        let retrieved_secrets1 = vault.get(product_name, component1, environment).await.unwrap();
        assert_eq!(retrieved_secrets1.len(), 1);
        assert_eq!(retrieved_secrets1.get("key1"), Some(&"value1".to_string()));
        
        // Verify secrets for component2
        let retrieved_secrets2 = vault.get(product_name, component2, environment).await.unwrap();
        assert_eq!(retrieved_secrets2.len(), 1);
        assert_eq!(retrieved_secrets2.get("key2"), Some(&"value2".to_string()));
        
        // Remove component1 secrets
        vault.remove(product_name, component1, environment).await.unwrap();
        
        // Verify component1 secrets are gone but component2 secrets remain
        let empty_secrets = vault.get(product_name, component1, environment).await.unwrap();
        assert_eq!(empty_secrets.len(), 0);
        
        let remaining_secrets = vault.get(product_name, component2, environment).await.unwrap();
        assert_eq!(remaining_secrets.len(), 1);
        assert_eq!(remaining_secrets.get("key2"), Some(&"value2".to_string()));
    }

    #[tokio::test]
    async fn test_file_vault_multiple_environments() {
        let temp_dir = TempDir::new().unwrap();
        let mut vault = vault::FileVault::new(temp_dir.path().to_path_buf(), None);
        
        // Test data
        let product_name = "test_product";
        let component_name = "test_component";
        let env_dev = "dev";
        let env_prod = "prod";
        
        // Create dev secrets
        let mut dev_secrets = HashMap::new();
        dev_secrets.insert("key".to_string(), "dev_value".to_string());
        
        // Create prod secrets
        let mut prod_secrets = HashMap::new();
        prod_secrets.insert("key".to_string(), "prod_value".to_string());
        
        // Create vault and set secrets for both environments
        vault.create_vault(product_name).await.unwrap();
        vault.set(product_name, component_name, env_dev, dev_secrets.clone()).await.unwrap();
        vault.set(product_name, component_name, env_prod, prod_secrets.clone()).await.unwrap();
        
        // Verify dev secrets
        let retrieved_dev = vault.get(product_name, component_name, env_dev).await.unwrap();
        assert_eq!(retrieved_dev.get("key"), Some(&"dev_value".to_string()));
        
        // Verify prod secrets
        let retrieved_prod = vault.get(product_name, component_name, env_prod).await.unwrap();
        assert_eq!(retrieved_prod.get("key"), Some(&"prod_value".to_string()));
        
        // Ensure file structure is correct
        let dev_file = temp_dir.path().join(product_name).join(format!("{}.json", env_dev));
        let prod_file = temp_dir.path().join(product_name).join(format!("{}.json", env_prod));
        assert!(dev_file.exists());
        assert!(prod_file.exists());
    }
}