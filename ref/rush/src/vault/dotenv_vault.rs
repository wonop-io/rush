use crate::dotenv_utils::{load_dotenv, save_dotenv};
use crate::vault::vault_trait::Vault;
use async_trait::async_trait;
use log::warn;
use serde_yaml::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

pub struct DotenvVault {
    product_dir: PathBuf,
    components: HashMap<String, PathBuf>,
}

impl DotenvVault {
    pub fn new(product_dir: PathBuf) -> Self {
        // TODO: It shouldn't read that here, but rather et it from the config
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
                    let absolute_path = product_dir.join(location).canonicalize().expect(&format!(
                        "Failed to get absolute path for component: {}",
                        component_name
                    ));
                    components.insert(component_name.to_string(), absolute_path);
                }
            }
        }

        Self {
            product_dir: product_dir
                .canonicalize()
                .expect("Failed to get absolute path for product_dir"),
            components,
        }
    }

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
        if let Some(env_path) = self.get_env_path(component_name) {
            if env_path.exists() {
                let env_map = load_dotenv(&env_path)?;
                Ok(env_map)
            } else {
                Ok(HashMap::new())
            }
        } else {
            warn!(
                "Component '{}' not found. Choices are: {:#?}",
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
        if let Some(env_path) = self.get_env_path(component_name) {
            save_dotenv(&env_path, secrets)?;
            Ok(())
        } else {
            warn!(
                "Component '{}' not found. Choices are: {:#?}",
                component_name,
                self.components.keys()
            );
            Ok(())
        }
    }

    async fn create_vault(&mut self, _product_name: &str) -> Result<(), Box<dyn Error>> {
        // No-op for dotenv vault
        Ok(())
    }

    async fn remove(
        &mut self,
        _product_name: &str,
        component_name: &str,
        _environment: &str,
    ) -> Result<(), Box<dyn Error>> {
        if let Some(env_path) = self.get_env_path(component_name) {
            if env_path.exists() {
                fs::remove_file(env_path)?;
            }
            Ok(())
        } else {
            warn!(
                "Component '{}' not found. Choices are: {:#?}",
                component_name,
                self.components.keys()
            );
            Ok(())
        }
    }

    async fn check_if_vault_exists(&self, _product_name: &str) -> Result<bool, Box<dyn Error>> {
        // No-op for dotenv vault
        Ok(true)
    }
}
