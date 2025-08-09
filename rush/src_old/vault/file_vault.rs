use crate::vault::vault_trait::Vault;
use async_trait::async_trait;
use log::{debug, trace};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

pub struct FileVault {
    directory: PathBuf,
    encryption_key: Option<String>,
}

impl FileVault {
    pub fn new(directory: PathBuf, encryption_key: Option<String>) -> Self {
        FileVault {
            directory,
            encryption_key,
        }
    }

    fn get_vault_path(&self, product_name: &str, environment: &str) -> PathBuf {
        self.directory
            .join(product_name)
            .join(format!("{}.json", environment))
    }

    fn load_secrets(&self, path: &Path) -> Result<Value, Box<dyn Error>> {
        if !path.exists() {
            return Ok(json!({}));
        }

        let content = fs::read_to_string(path)?;
        let value: Value = serde_json::from_str(&content)?;
        Ok(value)
    }

    fn save_secrets(&self, path: &Path, secrets: Value) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&secrets)?;
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
        trace!(
            "Getting secrets for {}/{}/{}",
            product_name,
            component_name,
            environment
        );

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

        Ok(result)
    }

    async fn set(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
        secrets: HashMap<String, String>,
    ) -> Result<(), Box<dyn Error>> {
        trace!(
            "Setting secrets for {}/{}/{}",
            product_name,
            component_name,
            environment
        );

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
        Ok(())
    }

    async fn create_vault(&mut self, product_name: &str) -> Result<(), Box<dyn Error>> {
        trace!("Creating vault directory for {}", product_name);
        fs::create_dir_all(self.directory.join(product_name))?;
        Ok(())
    }

    async fn remove(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<(), Box<dyn Error>> {
        trace!(
            "Removing secrets for {}/{}/{}",
            product_name,
            component_name,
            environment
        );

        let path = self.get_vault_path(product_name, environment);
        if !path.exists() {
            return Ok(());
        }

        let mut current_secrets = self.load_secrets(&path)?;
        if let Some(obj) = current_secrets.as_object_mut() {
            obj.remove(component_name);
        }

        self.save_secrets(&path, current_secrets)?;
        Ok(())
    }

    async fn check_if_vault_exists(&self, product_name: &str) -> Result<bool, Box<dyn Error>> {
        let vault_dir = self.directory.join(product_name);
        Ok(vault_dir.exists() && vault_dir.is_dir())
    }
}
