use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use log::{debug, trace};
use serde_json::{json, Value};

use crate::vault::vault_trait::Vault;

/// Implements the Vault trait using a filesystem-based storage mechanism
#[derive(Debug)]
pub struct FileVault {
    /// Directory where vault files are stored
    directory: PathBuf,
}

impl FileVault {
    /// Creates a new FileVault instance
    ///
    /// # Arguments
    ///
    /// * `directory` - The base directory where vault files will be stored
    /// * `_encryption_key` - Optional key for encrypting stored secrets (not yet implemented)
    pub fn new(directory: PathBuf, _encryption_key: Option<String>) -> Self {
        FileVault { directory }
    }

    /// Constructs the path to a specific vault file
    ///
    /// # Arguments
    ///
    /// * `product_name` - The product name used to organize vault files
    /// * `environment` - The environment (dev, prod, etc.)
    fn get_vault_path(&self, product_name: &str, environment: &str) -> PathBuf {
        self.directory
            .join(product_name)
            .join(format!("{environment}.json"))
    }

    /// Loads secrets from a vault file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the vault file
    fn load_secrets(&self, path: &Path) -> Result<Value, Box<dyn Error>> {
        if !path.exists() {
            trace!(
                "Vault file does not exist at {}, returning empty object",
                path.display()
            );
            return Ok(json!({}));
        }

        trace!("Loading secrets from {}", path.display());

        // TODO: Handle decryption if encryption_key is present
        let value: Value = rush_core::config_loader::ConfigLoader::load_json(path)
            .map_err(|e| Box::new(e) as Box<dyn Error>)?;
        Ok(value)
    }

    /// Saves secrets to a vault file
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the vault file should be stored
    /// * `secrets` - The secrets data to store
    fn save_secrets(&self, path: &Path, secrets: Value) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = path.parent() {
            trace!(
                "Creating parent directories for vault file: {}",
                parent.display()
            );
            fs::create_dir_all(parent)?;
        }

        // TODO: Handle encryption if encryption_key is present
        let content = serde_json::to_string_pretty(&secrets)?;
        trace!("Saving secrets to {}", path.display());
        fs::write(path, content)?;
        Ok(())
    }
}

#[async_trait]
impl Vault for FileVault {
    async fn get(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<HashMap<String, String>, Box<dyn Error>> {
        trace!("Getting secrets for {product_name}/{component_name}/{environment}");

        let path = self.get_vault_path(product_name, environment);
        let secrets = self.load_secrets(&path)?;

        let mut result = HashMap::new();
        if let Some(component) = secrets.get(component_name) {
            if let Some(obj) = component.as_object() {
                for (key, value) in obj {
                    if let Some(value_str) = value.as_str() {
                        result.insert(key.clone(), value_str.to_string());
                    }
                }
            }
        }

        debug!("Retrieved {} secrets for {}", result.len(), component_name);
        Ok(result)
    }

    async fn set(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
        secrets: HashMap<String, String>,
    ) -> Result<(), Box<dyn Error>> {
        trace!("Setting secrets for {product_name}/{component_name}/{environment}");

        let path = self.get_vault_path(product_name, environment);
        let mut current_secrets = self.load_secrets(&path)?;

        let mut component_secrets = json!({});
        if let Some(obj) = component_secrets.as_object_mut() {
            for (key, value) in &secrets {
                obj.insert(key.clone(), json!(value));
            }
        }

        if let Some(obj) = current_secrets.as_object_mut() {
            obj.insert(component_name.to_string(), component_secrets);
        }

        self.save_secrets(&path, current_secrets)?;
        debug!("Saved {} secrets for {}", secrets.len(), component_name);
        Ok(())
    }

    async fn create_vault(&mut self, product_name: &str) -> Result<(), Box<dyn Error>> {
        trace!("Creating vault directory for {product_name}");
        let vault_dir = self.directory.join(product_name);
        fs::create_dir_all(vault_dir)?;
        debug!("Created vault directory for {product_name}");
        Ok(())
    }

    async fn remove(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<(), Box<dyn Error>> {
        trace!("Removing secrets for {product_name}/{component_name}/{environment}");

        let path = self.get_vault_path(product_name, environment);
        if !path.exists() {
            debug!("Vault file does not exist, nothing to remove");
            return Ok(());
        }

        let mut current_secrets = self.load_secrets(&path)?;
        if let Some(obj) = current_secrets.as_object_mut() {
            obj.remove(component_name);
        }

        self.save_secrets(&path, current_secrets)?;
        debug!("Removed secrets for {component_name}");
        Ok(())
    }

    async fn check_if_vault_exists(&self, product_name: &str) -> Result<bool, Box<dyn Error>> {
        let vault_dir = self.directory.join(product_name);
        let exists = vault_dir.exists() && vault_dir.is_dir();
        trace!("Checking if vault exists for {product_name}: {exists}");
        Ok(exists)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_create_vault() {
        let temp_dir = TempDir::new().unwrap();
        let mut vault = FileVault::new(temp_dir.path().to_path_buf(), None);

        let result = vault.create_vault("test_product").await;
        assert!(result.is_ok());

        let vault_dir = temp_dir.path().join("test_product");
        assert!(vault_dir.exists());
        assert!(vault_dir.is_dir());
    }

    #[tokio::test]
    async fn test_set_and_get_secrets() {
        let temp_dir = TempDir::new().unwrap();
        let mut vault = FileVault::new(temp_dir.path().to_path_buf(), None);

        // Create vault
        vault.create_vault("test_product").await.unwrap();

        // Set secrets
        let mut secrets = HashMap::new();
        secrets.insert("KEY1".to_string(), "value1".to_string());
        secrets.insert("KEY2".to_string(), "value2".to_string());

        let result = vault
            .set("test_product", "test_component", "dev", secrets.clone())
            .await;
        assert!(result.is_ok());

        // Get secrets
        let retrieved = vault
            .get("test_product", "test_component", "dev")
            .await
            .unwrap();
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(retrieved.get("KEY2"), Some(&"value2".to_string()));
    }

    #[tokio::test]
    async fn test_remove_secrets() {
        let temp_dir = TempDir::new().unwrap();
        let mut vault = FileVault::new(temp_dir.path().to_path_buf(), None);

        // Create vault and add secrets
        vault.create_vault("test_product").await.unwrap();

        let mut secrets = HashMap::new();
        secrets.insert("KEY1".to_string(), "value1".to_string());
        vault
            .set("test_product", "test_component", "dev", secrets)
            .await
            .unwrap();

        // Remove secrets
        vault
            .remove("test_product", "test_component", "dev")
            .await
            .unwrap();

        // Verify secrets are removed
        let retrieved = vault
            .get("test_product", "test_component", "dev")
            .await
            .unwrap();
        assert!(retrieved.is_empty());
    }

    #[tokio::test]
    async fn test_vault_existence() {
        let temp_dir = TempDir::new().unwrap();
        let mut vault = FileVault::new(temp_dir.path().to_path_buf(), None);

        // Check non-existent vault
        assert!(!vault.check_if_vault_exists("nonexistent").await.unwrap());

        // Create vault
        vault.create_vault("test_product").await.unwrap();

        // Check existent vault
        assert!(vault.check_if_vault_exists("test_product").await.unwrap());
    }
}
