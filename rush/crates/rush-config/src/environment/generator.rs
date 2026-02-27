use std::collections::HashMap;
use std::fs;
use std::io::Error;
use std::path::PathBuf;

use log::{debug, trace, warn};

use crate::dotenv::save_dotenv;

/// Simple environment generator that creates dotenv files from environment YAML
///
/// This generator handles both flat YAML format and nested component-based format:
///
/// Flat format (legacy):
/// ```yaml
/// VAR1: value1
/// VAR2: value2
/// ```
///
/// Component-based format (with YAML tags):
/// ```yaml
/// backend:
///   RUST_LOG: !Static "trace"
///   PORT: !Static "8000"
/// frontend:
///   API_URL: !Static "http://localhost"
/// ```
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
            debug!("Base env file not found: {}", self.base_env_path);
            HashMap::new()
        };

        // Load override environment if it exists
        let override_env = if PathBuf::from(&self.override_env_path).exists() {
            load_yaml_env(&self.override_env_path)?
        } else {
            debug!("Override env file not found: {}", self.override_env_path);
            HashMap::new()
        };

        // Merge environments (override takes precedence)
        let mut merged_env = base_env;
        merged_env.extend(override_env);

        if merged_env.is_empty() {
            debug!("No environment variables found to generate");
            return Ok(());
        }

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
///
/// Handles both flat format and nested component-based format with YAML tags
fn load_yaml_env(path: &str) -> Result<HashMap<String, String>, Error> {
    let content = fs::read_to_string(path)?;

    if content.trim().is_empty() {
        return Ok(HashMap::new());
    }

    let yaml: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let mut env_vars = HashMap::new();

    if let Some(map) = yaml.as_mapping() {
        for (key, value) in map {
            if let Some(k) = key.as_str() {
                // Check if value is a simple string (flat format)
                if let Some(v) = value.as_str() {
                    env_vars.insert(k.to_string(), v.to_string());
                }
                // Check if value is a mapping (component-based format)
                else if let Some(component_vars) = value.as_mapping() {
                    // This is a component - extract its variables
                    for (var_key, var_value) in component_vars {
                        if let Some(var_name) = var_key.as_str() {
                            // Handle tagged values like !Static "value"
                            if let Some(static_value) = extract_tagged_value(var_value) {
                                // Prefix with component name to avoid conflicts
                                // Or just use the var name directly for simplicity
                                env_vars.insert(var_name.to_string(), static_value);
                            }
                            // Handle plain string values
                            else if let Some(v) = var_value.as_str() {
                                env_vars.insert(var_name.to_string(), v.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    debug!("Loaded {} environment variables from {}", env_vars.len(), path);
    Ok(env_vars)
}

/// Extract value from a YAML tagged value like !Static "value"
fn extract_tagged_value(value: &serde_yaml::Value) -> Option<String> {
    // serde_yaml represents tagged values in different ways depending on the structure
    // For !Static "value", it might be represented as a tagged scalar or a mapping

    // Try to get it as a tagged scalar (common for simple tags)
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }

    // If it's a mapping, look for common tag keys
    if let Some(map) = value.as_mapping() {
        // Handle format like { Static: "value" }
        if let Some(static_val) = map.get(&serde_yaml::Value::String("Static".to_string())) {
            if let Some(s) = static_val.as_str() {
                return Some(s.to_string());
            }
        }
        // Handle other common variants
        for tag_name in &["Ask", "AskWithDefault", "Timestamp"] {
            if let Some(val) = map.get(&serde_yaml::Value::String(tag_name.to_string())) {
                if let Some(s) = val.as_str() {
                    warn!(
                        "Found {} tag - interactive values not supported in simple generator",
                        tag_name
                    );
                    return None;
                }
            }
        }
    }

    // Try sequence format for tuple variants like !AskWithDefault ["prompt", "default"]
    if let Some(seq) = value.as_sequence() {
        if seq.len() >= 2 {
            // For AskWithDefault, return the default value
            if let Some(default) = seq.get(1).and_then(|v| v.as_str()) {
                return Some(default.to_string());
            }
        }
        if let Some(first) = seq.first().and_then(|v| v.as_str()) {
            return Some(first.to_string());
        }
    }

    None
}
