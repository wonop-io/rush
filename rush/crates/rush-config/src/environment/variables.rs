use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use log::{debug, trace};
use serde::{Deserialize, Serialize};

/// Stores environment variable definitions for different environments
#[derive(Debug, Serialize, Deserialize)]
pub struct VariablesFile {
    /// Variables for the development environment
    pub dev: HashMap<String, String>,
    /// Variables for the staging environment
    pub staging: HashMap<String, String>,
    /// Variables for the production environment
    pub prod: HashMap<String, String>,
    /// Variables for the local environment
    pub local: HashMap<String, String>,
}

/// Manages environment variables for a specific environment
#[derive(Debug, Serialize, Deserialize)]
pub struct Variables {
    /// All environment variables from the variables file
    pub values: VariablesFile,
    /// The current environment (dev, staging, prod, local)
    pub env: String,
}

impl Variables {
    /// Creates a new Variables instance by loading from a file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the variables YAML file
    /// * `env` - The environment to use (dev, staging, prod, local)
    ///
    /// # Returns
    ///
    /// An Arc-wrapped Variables instance
    pub fn new<P: AsRef<Path>>(path: P, env: &str) -> Arc<Self> {
        trace!(
            "Loading variables from {} for {} environment",
            path.as_ref().display(),
            env
        );

        let contents = match std::fs::read_to_string(&path) {
            Ok(content) => {
                debug!("Successfully read variables file");
                content
            }
            Err(e) => {
                debug!("Failed to read variables file: {}", e);
                return Arc::new(Variables {
                    values: VariablesFile {
                        dev: HashMap::new(),
                        staging: HashMap::new(),
                        prod: HashMap::new(),
                        local: HashMap::new(),
                    },
                    env: env.to_lowercase(),
                });
            }
        };

        let variables: VariablesFile = match serde_yaml::from_str(&contents) {
            Ok(vars) => {
                debug!("Successfully parsed variables file");
                vars
            }
            Err(e) => {
                debug!("Failed to parse variables file: {}", e);
                VariablesFile {
                    dev: HashMap::new(),
                    staging: HashMap::new(),
                    prod: HashMap::new(),
                    local: HashMap::new(),
                }
            }
        };

        Arc::new(Variables {
            values: variables,
            env: env.to_lowercase(),
        })
    }

    /// Gets a variable value for the current environment
    ///
    /// # Arguments
    ///
    /// * `key` - The variable name to lookup
    ///
    /// # Returns
    ///
    /// The variable value as a string, or None if not found
    pub fn get(&self, key: &str) -> Option<String> {
        trace!("Looking up variable {} in {} environment", key, self.env);

        match self.env.as_str() {
            "dev" => self.values.dev.get(key).cloned(),
            "staging" => self.values.staging.get(key).cloned(),
            "prod" => self.values.prod.get(key).cloned(),
            "local" => self.values.local.get(key).cloned(),
            _ => {
                debug!("Unknown environment: {}", self.env);
                None
            }
        }
    }

    /// Gets all variables for the current environment
    ///
    /// # Returns
    ///
    /// A reference to the hashmap of environment variables
    pub fn get_all(&self) -> Option<&HashMap<String, String>> {
        match self.env.as_str() {
            "dev" => Some(&self.values.dev),
            "staging" => Some(&self.values.staging),
            "prod" => Some(&self.values.prod),
            "local" => Some(&self.values.local),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_load_variables_from_file() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"
dev:
  APP_URL: "http://localhost:8080"
  DEBUG: "true"
staging:
  APP_URL: "https://staging.example.com"
  DEBUG: "true"
prod:
  APP_URL: "https://example.com"
  DEBUG: "false"
local:
  APP_URL: "http://127.0.0.1:3000"
  DEBUG: "true"
"#
        )
        .unwrap();

        let vars = Variables::new(file.path(), "dev");

        assert_eq!(
            vars.get("APP_URL"),
            Some("http://localhost:8080".to_string())
        );
        assert_eq!(vars.get("DEBUG"), Some("true".to_string()));
        assert_eq!(vars.get("NONEXISTENT"), None);
    }

    #[test]
    fn test_empty_variables_file() {
        let file = NamedTempFile::new().unwrap();

        let vars = Variables::new(file.path(), "dev");

        assert_eq!(vars.get("ANY_KEY"), None);
    }

    #[test]
    fn test_nonexistent_file() {
        let vars = Variables::new("/path/that/does/not/exist.yaml", "prod");

        assert_eq!(vars.get("ANY_KEY"), None);
    }

    #[test]
    fn test_get_all_variables() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"
dev:
  KEY1: "value1"
  KEY2: "value2"
staging:
  KEY1: "staging1"
prod:
  KEY1: "prod1"
local:
  KEY1: "local1"
"#
        )
        .unwrap();

        let vars = Variables::new(file.path(), "dev");

        let all_vars = vars.get_all().unwrap();
        assert_eq!(all_vars.len(), 2);
        assert_eq!(all_vars.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(all_vars.get("KEY2"), Some(&"value2".to_string()));
    }
}
