extern crate rush_cli;

#[cfg(test)]
mod tests {
    use rush_cli::vault::{
        create_vault_provider, Environment, FileVault, SecretsProvider, 
        SecretError, VaultAdapter
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::fs;

    fn setup_test_dir() -> PathBuf {
        let test_dir = PathBuf::from("target/test_vault");
        // Clean up from previous tests if directory exists
        if test_dir.exists() {
            fs::remove_dir_all(&test_dir).expect("Failed to clean up test directory");
        }
        fs::create_dir_all(&test_dir).expect("Failed to create test directory");
        test_dir
    }

    fn teardown_test_dir(dir: &PathBuf) {
        if dir.exists() {
            fs::remove_dir_all(dir).expect("Failed to clean up test directory");
        }
    }

    #[tokio::test]
    async fn test_vault_adapter_with_file_vault() {
        let test_dir = setup_test_dir();
        
        // Test constants
        let product_name = "test-product";
        let component_name = "test-component";
        let env = Environment::Development;
        
        // Create vault instance and adapter
        let file_vault = FileVault::new(test_dir.clone(), None);
        let mut provider = VaultAdapter::new(file_vault);
        
        // Test creating and setting secrets
        let mut test_secrets = HashMap::new();
        test_secrets.insert("API_KEY".to_string(), "test-api-key-value".to_string());
        test_secrets.insert("DATABASE_URL".to_string(), "postgres://localhost:5432/testdb".to_string());
        
        // Set secrets
        let result = provider.set_secrets(
            product_name,
            component_name,
            &env,
            test_secrets.clone()
        ).await;
        assert!(result.is_ok(), "Failed to set secrets: {:?}", result);
        
        // Get all secrets
        let retrieved_secrets = provider.get_secrets(
            product_name,
            component_name,
            &env
        ).await;
        assert!(retrieved_secrets.is_ok(), "Failed to get secrets: {:?}", retrieved_secrets);
        let retrieved_secrets = retrieved_secrets.unwrap();
        
        // Verify retrieved secrets match what we set
        assert_eq!(retrieved_secrets.len(), 2);
        assert_eq!(retrieved_secrets.get("API_KEY"), Some(&"test-api-key-value".to_string()));
        assert_eq!(retrieved_secrets.get("DATABASE_URL"), Some(&"postgres://localhost:5432/testdb".to_string()));
        
        // Test get_secret (individual secret)
        let api_key = provider.get_secret(
            product_name,
            component_name,
            &env,
            "API_KEY"
        ).await;
        assert!(api_key.is_ok(), "Failed to get individual secret: {:?}", api_key);
        assert_eq!(api_key.unwrap(), "test-api-key-value");
        
        // Test has_secret
        let has_api_key = provider.has_secret(
            product_name,
            component_name,
            &env,
            "API_KEY"
        ).await;
        assert!(has_api_key.is_ok(), "Failed to check if secret exists: {:?}", has_api_key);
        assert!(has_api_key.unwrap());
        
        let has_nonexistent = provider.has_secret(
            product_name,
            component_name,
            &env,
            "NONEXISTENT_KEY"
        ).await;
        assert!(has_nonexistent.is_ok());
        assert!(!has_nonexistent.unwrap());
        
        // Test set_secret (individual secret)
        let set_result = provider.set_secret(
            product_name,
            component_name,
            &env,
            "NEW_SECRET",
            "new-secret-value"
        ).await;
        assert!(set_result.is_ok(), "Failed to set individual secret: {:?}", set_result);
        
        // Verify new secret exists
        let new_secret = provider.get_secret(
            product_name,
            component_name,
            &env,
            "NEW_SECRET"
        ).await;
        assert!(new_secret.is_ok());
        assert_eq!(new_secret.unwrap(), "new-secret-value");
        
        // Test delete_secret
        let delete_result = provider.delete_secret(
            product_name,
            component_name,
            &env,
            "API_KEY"
        ).await;
        assert!(delete_result.is_ok(), "Failed to delete secret: {:?}", delete_result);
        
        // Verify secret was deleted
        let deleted_secret = provider.get_secret(
            product_name,
            component_name,
            &env,
            "API_KEY"
        ).await;
        assert!(deleted_secret.is_err(), "Secret should be deleted");
        
        // Test delete_all_secrets
        let delete_all_result = provider.delete_all_secrets(
            product_name,
            component_name,
            &env
        ).await;
        assert!(delete_all_result.is_ok(), "Failed to delete all secrets: {:?}", delete_all_result);
        
        // Verify all secrets are gone
        let empty_secrets = provider.get_secrets(
            product_name,
            component_name,
            &env
        ).await;
        assert!(empty_secrets.is_ok());
        assert!(empty_secrets.unwrap().is_empty());
        
        // Test factory function
        let arc_provider = create_vault_provider(FileVault::new(test_dir.clone(), None));
        // Test that we received a valid Arc
        assert!(arc_provider.has_secret(
            product_name,
            component_name,
            &env,
            "NONEXISTENT_KEY"
        ).await.is_ok());
        
        // Clean up
        teardown_test_dir(&test_dir);
    }
    
    #[tokio::test]
    async fn test_environment_conversions() {
        // Test Environment enum conversions
        let dev = Environment::from("development");
        assert!(matches!(dev, Environment::Development));
        
        let dev_short = Environment::from("dev");
        assert!(matches!(dev_short, Environment::Development));
        
        let test = Environment::from("testing");
        assert!(matches!(test, Environment::Testing));
        
        let test_short = Environment::from("test");
        assert!(matches!(test_short, Environment::Testing));
        
        let staging = Environment::from("staging");
        assert!(matches!(staging, Environment::Staging));
        
        let prod = Environment::from("production");
        assert!(matches!(prod, Environment::Production));
        
        let prod_short = Environment::from("prod");
        assert!(matches!(prod_short, Environment::Production));
        
        let custom = Environment::from("custom-environment");
        assert!(matches!(custom, Environment::Custom(name) if name == "custom-environment"));
        
        // Test ToString implementation
        assert_eq!(Environment::Development.to_string(), "development");
        assert_eq!(Environment::Testing.to_string(), "testing");
        assert_eq!(Environment::Staging.to_string(), "staging");
        assert_eq!(Environment::Production.to_string(), "production");
        assert_eq!(Environment::Custom("custom".to_string()).to_string(), "custom");
    }
}