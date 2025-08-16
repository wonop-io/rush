use crate::vault::vault_trait::Vault;
use async_trait::async_trait;
use log::{debug, warn};
use rush_core::dotenv::{load_dotenv, save_dotenv};
use serde_yaml::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

/// Vault implementation that stores secrets in .env.secrets files
#[derive(Debug)]
pub struct DotenvVault {
    components: HashMap<String, PathBuf>,
}

impl DotenvVault {
    /// Creates a new DotenvVault that stores secrets in .env.secrets files
    pub fn new(product_dir: PathBuf) -> Self {
        // Read the stack spec to find component locations
        let stack_yaml_path = product_dir.join("stack.spec.yaml");
        let stack_yaml_content =
            fs::read_to_string(&stack_yaml_path).expect("Unable to read stack.spec.yaml");
        let stack_yaml: Value =
            serde_yaml::from_str(&stack_yaml_content).expect("Unable to parse stack.spec.yaml");

        let mut components = HashMap::new();
        if let Some(components_map) = stack_yaml.as_mapping() {
            for (component_name, component_info) in components_map {
                if let (Some(component_name), Some(location)) = (
                    component_name.as_str(),
                    component_info.get("location").and_then(|v| v.as_str()),
                ) {
                    let absolute_path =
                        product_dir
                            .join(location)
                            .canonicalize()
                            .unwrap_or_else(|_| {
                                panic!(
                                    "Failed to get absolute path for component: {component_name}"
                                )
                            });
                    components.insert(component_name.to_string(), absolute_path);
                }
            }
        }

        Self { components }
    }

    /// Gets the path to the component's .env.secrets file
    fn get_env_path(&self, component_name: &str) -> Option<PathBuf> {
        self.components
            .get(component_name)
            .map(|path| path.join(".env.secrets"))
    }
}

#[async_trait]
impl Vault for DotenvVault {
    async fn get(
        &self,
        _product_name: &str,
        component_name: &str,
        _environment: &str,
    ) -> Result<HashMap<String, String>, Box<dyn Error>> {
        debug!("Getting secrets for component: {}", component_name);

        if let Some(env_path) = self.get_env_path(component_name) {
            if env_path.exists() {
                let env_map = load_dotenv(&env_path)?;
                debug!("Loaded {} secrets for {}", env_map.len(), component_name);
                Ok(env_map)
            } else {
                debug!("No .env.secrets file found for {}", component_name);
                Ok(HashMap::new())
            }
        } else {
            warn!(
                "Component '{}' not found. Available components are: {:#?}",
                component_name,
                self.components.keys()
            );
            Ok(HashMap::new())
        }
    }

    async fn set(
        &mut self,
        _product_name: &str,
        component_name: &str,
        _environment: &str,
        secrets: HashMap<String, String>,
    ) -> Result<(), Box<dyn Error>> {
        debug!(
            "Setting {} secrets for component: {}",
            secrets.len(),
            component_name
        );

        if let Some(env_path) = self.get_env_path(component_name) {
            save_dotenv(&env_path, secrets)?;
            debug!("Saved secrets to {}", env_path.display());
            Ok(())
        } else {
            warn!(
                "Component '{}' not found. Available components are: {:#?}",
                component_name,
                self.components.keys()
            );
            Ok(())
        }
    }

    async fn create_vault(&mut self, _product_name: &str) -> Result<(), Box<dyn Error>> {
        // No-op for dotenv vault as it doesn't require initialization
        debug!("DotenvVault doesn't require explicit vault creation");
        Ok(())
    }

    async fn remove(
        &mut self,
        _product_name: &str,
        component_name: &str,
        _environment: &str,
    ) -> Result<(), Box<dyn Error>> {
        debug!("Removing secrets for component: {}", component_name);

        if let Some(env_path) = self.get_env_path(component_name) {
            if env_path.exists() {
                fs::remove_file(&env_path)?;
                debug!("Removed secrets file: {}", env_path.display());
            } else {
                debug!("No secrets file to remove for {}", component_name);
            }
            Ok(())
        } else {
            warn!(
                "Component '{}' not found. Available components are: {:#?}",
                component_name,
                self.components.keys()
            );
            Ok(())
        }
    }

    async fn check_if_vault_exists(&self, _product_name: &str) -> Result<bool, Box<dyn Error>> {
        // DotenvVault always exists as it's file-based
        debug!("DotenvVault always exists");
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_dotenv_vault_get_nonexistent() {
        let temp_dir = TempDir::new().unwrap();

        // Create a minimal stack.spec.yaml
        let stack_yaml = r#"
component1:
  location: "component1"
"#;
        fs::create_dir(temp_dir.path().join("component1")).unwrap();
        fs::write(temp_dir.path().join("stack.spec.yaml"), stack_yaml).unwrap();

        let vault = DotenvVault::new(temp_dir.path().to_path_buf());
        let result = vault.get("test", "component1", "dev").await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_dotenv_vault_set_and_get() {
        let temp_dir = TempDir::new().unwrap();

        // Create a minimal stack.spec.yaml
        let stack_yaml = r#"
component1:
  location: "component1"
"#;
        fs::create_dir(temp_dir.path().join("component1")).unwrap();
        fs::write(temp_dir.path().join("stack.spec.yaml"), stack_yaml).unwrap();

        let mut vault = DotenvVault::new(temp_dir.path().to_path_buf());

        // Set some secrets
        let mut secrets = HashMap::new();
        secrets.insert("API_KEY".to_string(), "secret_value".to_string());
        secrets.insert("DB_PASSWORD".to_string(), "another_secret".to_string());

        vault
            .set("test", "component1", "dev", secrets.clone())
            .await
            .unwrap();

        // Verify the secrets file was created
        assert!(temp_dir.path().join("component1/.env.secrets").exists());

        // Get the secrets back
        let result = vault.get("test", "component1", "dev").await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("API_KEY"), Some(&"secret_value".to_string()));
        assert_eq!(
            result.get("DB_PASSWORD"),
            Some(&"another_secret".to_string())
        );
    }

    #[tokio::test]
    async fn test_dotenv_vault_remove() {
        let temp_dir = TempDir::new().unwrap();

        // Create a minimal stack.spec.yaml
        let stack_yaml = r#"
component1:
  location: "component1"
"#;
        fs::create_dir(temp_dir.path().join("component1")).unwrap();
        fs::write(temp_dir.path().join("stack.spec.yaml"), stack_yaml).unwrap();

        // Create a .env.secrets file
        let env_secrets_path = temp_dir.path().join("component1/.env.secrets");
        let mut file = File::create(&env_secrets_path).unwrap();
        writeln!(file, "API_KEY=\"secret_value\"").unwrap();

        let mut vault = DotenvVault::new(temp_dir.path().to_path_buf());

        // Verify it exists and has the secret
        let result = vault.get("test", "component1", "dev").await.unwrap();
        assert_eq!(result.len(), 1);

        // Remove the secrets
        vault.remove("test", "component1", "dev").await.unwrap();

        // Verify the file was removed
        assert!(!env_secrets_path.exists());
    }
}
