//! Variables management for build configurations
//!
//! This module provides a way to manage environment-specific variables used in build processes.
//! It supports different sets of variables for different environments (dev, staging, prod, local).

use log::{debug, trace};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// Contains environment-specific variable sets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariablesFile {
    /// Variables for development environment
    pub dev: HashMap<String, String>,
    /// Variables for staging environment
    pub staging: HashMap<String, String>,
    /// Variables for production environment
    pub prod: HashMap<String, String>,
    /// Variables for local environment
    pub local: HashMap<String, String>,
}

/// A wrapper around VariablesFile that identifies the current environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variables {
    /// All environment variables from the variables file
    pub values: VariablesFile,
    /// The current environment (dev, staging, prod, local)
    pub env: String,
}

impl Variables {
    pub fn empty() -> Arc<Self> {
        Arc::new(Variables {
            values: VariablesFile {
                dev: HashMap::new(),
                staging: HashMap::new(),
                prod: HashMap::new(),
                local: HashMap::new(),
            },
            env: String::new(),
        })
    }

    /// Creates a new Variables instance from a file and specified environment
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
            "Loading variables from {} for environment {}",
            path.as_ref().display(),
            env
        );

        // Read and parse the variables file
        let contents = match fs::read_to_string(&path) {
            Ok(content) => {
                debug!(
                    "Successfully read variables file from {}",
                    path.as_ref().display()
                );
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

        // Parse YAML content
        let variables: VariablesFile = match serde_yaml::from_str(&contents) {
            Ok(vars) => {
                debug!("Successfully parsed variables YAML file");
                vars
            }
            Err(e) => {
                debug!("Failed to parse variables YAML file: {}", e);
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
        match self.env.as_str() {
            "dev" => self.values.dev.get(key).cloned(),
            "staging" => self.values.staging.get(key).cloned(),
            "prod" => self.values.prod.get(key).cloned(),
            "local" => self.values.local.get(key).cloned(),
            _ => None,
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
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_variables() {
        // Create a temporary file with test variables
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "dev:").unwrap();
        writeln!(file, "  TEST_VAR: \"dev_value\"").unwrap();
        writeln!(file, "staging:").unwrap();
        writeln!(file, "  TEST_VAR: \"staging_value\"").unwrap();
        writeln!(file, "prod:").unwrap();
        writeln!(file, "  TEST_VAR: \"prod_value\"").unwrap();
        writeln!(file, "local:").unwrap();
        writeln!(file, "  TEST_VAR: \"local_value\"").unwrap();

        // Test loading for different environments
        let dev_vars = Variables::new(file.path(), "dev");
        assert_eq!(dev_vars.get("TEST_VAR"), Some("dev_value".to_string()));

        let staging_vars = Variables::new(file.path(), "staging");
        assert_eq!(
            staging_vars.get("TEST_VAR"),
            Some("staging_value".to_string())
        );

        let prod_vars = Variables::new(file.path(), "prod");
        assert_eq!(prod_vars.get("TEST_VAR"), Some("prod_value".to_string()));

        let local_vars = Variables::new(file.path(), "local");
        assert_eq!(local_vars.get("TEST_VAR"), Some("local_value".to_string()));
    }

    #[test]
    fn test_nonexistent_variables() {
        // Create a temporary file with test variables
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "dev:").unwrap();
        writeln!(file, "  TEST_VAR: \"dev_value\"").unwrap();

        // Test loading for different environments
        let vars = Variables::new(file.path(), "dev");
        assert_eq!(vars.get("NONEXISTENT"), None);
    }

    #[test]
    fn test_invalid_environment() {
        // Create a temporary file with test variables
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "dev:").unwrap();
        writeln!(file, "  TEST_VAR: \"dev_value\"").unwrap();

        // Test loading for invalid environment
        let vars = Variables::new(file.path(), "invalid");
        assert_eq!(vars.get("TEST_VAR"), None);
    }

    #[test]
    fn test_empty_file() {
        // Create an empty temporary file
        let file = NamedTempFile::new().unwrap();

        // Test loading from empty file
        let vars = Variables::new(file.path(), "dev");
        assert_eq!(vars.get("TEST_VAR"), None);
    }

    #[test]
    fn test_get_all_variables() {
        // Create a temporary file with test variables
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "dev:").unwrap();
        writeln!(file, "  VAR1: \"value1\"").unwrap();
        writeln!(file, "  VAR2: \"value2\"").unwrap();

        // Test getting all variables
        let vars = Variables::new(file.path(), "dev");
        let all_vars = vars.get_all().unwrap();
        assert_eq!(all_vars.len(), 2);
        assert_eq!(all_vars.get("VAR1"), Some(&"value1".to_string()));
        assert_eq!(all_vars.get("VAR2"), Some(&"value2".to_string()));
    }
}
