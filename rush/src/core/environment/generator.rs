use crate::core::dotenv::save_dotenv;
use log::trace;
use std::collections::HashMap;
use std::fs;
use std::io::Error;
use std::path::PathBuf;

/// Simple environment generator that creates dotenv files from environment YAML
pub struct EnvironmentGenerator {
    product_name: String,
    base_env_path: String,
    override_env_path: String,
}

impl EnvironmentGenerator {
    pub fn new(product_name: String, base_env_path: &str, override_env_path: &str) -> Self {
        Self {
            product_name,
            base_env_path: base_env_path.to_string(),
            override_env_path: override_env_path.to_string(),
        }
    }

    /// Generate .env files based on environment definitions
    pub fn generate_dotenv_files(&self) -> Result<(), Error> {
        trace!("Generating dotenv files for product: {}", self.product_name);

        // Load base environment if it exists
        let base_env = if PathBuf::from(&self.base_env_path).exists() {
            load_yaml_env(&self.base_env_path)?
        } else {
            HashMap::new()
        };

        // Load override environment if it exists
        let override_env = if PathBuf::from(&self.override_env_path).exists() {
            load_yaml_env(&self.override_env_path)?
        } else {
            HashMap::new()
        };

        // Merge environments (override takes precedence)
        let mut merged_env = base_env;
        merged_env.extend(override_env);

        // Save to .env file in product directory
        let product_dir = PathBuf::from(&self.base_env_path)
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .to_path_buf();

        let env_file = product_dir.join(".env");
        save_dotenv(&env_file, merged_env)?;

        trace!("Generated .env file at: {}", env_file.display());
        Ok(())
    }
}

/// Load environment variables from a YAML file
fn load_yaml_env(path: &str) -> Result<HashMap<String, String>, Error> {
    let content = fs::read_to_string(path)?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let mut env_vars = HashMap::new();

    if let Some(map) = yaml.as_mapping() {
        for (key, value) in map {
            if let (Some(k), Some(v)) = (key.as_str(), value.as_str()) {
                env_vars.insert(k.to_string(), v.to_string());
            }
        }
    }

    Ok(env_vars)
}
