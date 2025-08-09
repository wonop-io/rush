use log::{debug, error, trace, warn};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::config::types::Config;
use crate::utils::{canonical_path, find_project_root, read_to_string};

/// Loads configuration files for Rush CLI
pub struct ConfigLoader {
    root_path: PathBuf,
}

impl ConfigLoader {
    /// Creates a new ConfigLoader with the given root path
    pub fn new<P: AsRef<Path>>(root_path: P) -> Self {
        Self {
            root_path: root_path.as_ref().to_path_buf(),
        }
    }

    /// Creates a new ConfigLoader using the project root as the root path
    pub fn from_project_root() -> Result<Self, String> {
        let current_dir = std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?;

        let project_root = find_project_root(current_dir)
            .ok_or_else(|| "Failed to find project root".to_string())?;

        Ok(Self::new(project_root))
    }

    /// Loads the configuration for a specific product and environment
    pub fn load_config(
        &self,
        product_name: &str,
        environment: &str,
        docker_registry: &str,
        start_port: u16,
    ) -> Result<Arc<Config>, String> {
        debug!(
            "Loading configuration for product '{}' in environment '{}'",
            product_name, environment
        );

        // Validate environment
        let valid_environments = ["local", "dev", "prod", "staging"];
        if !valid_environments.contains(&environment) {
            let err_msg = format!("Invalid environment: {}", environment);
            error!("{}", err_msg);
            error!("Valid environments: {:?}", valid_environments);
            return Err(err_msg);
        }

        trace!("Environment '{}' is valid", environment);

        // Create config
        Config::new(
            self.root_path.to_str().unwrap(),
            product_name,
            environment,
            docker_registry,
            start_port,
        )
    }

    /// Loads the rushd.yaml configuration file
    pub fn load_rushd_config(&self) -> Result<RushdConfig, String> {
        let config_path = self.root_path.join("rushd.yaml");
        trace!("Loading rushd config from: {}", config_path.display());

        let content = read_to_string(&config_path)
            .map_err(|e| format!("Failed to read rushd.yaml: {}", e))?;

        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse rushd.yaml: {}", e))
    }
}

/// Configuration loaded from rushd.yaml
#[derive(Debug, Deserialize, Serialize)]
pub struct RushdConfig {
    pub env: std::collections::HashMap<String, String>,
}

/// Applies environment variables from rushd.yaml
pub fn apply_rushd_config(config: &RushdConfig) {
    trace!("Applying rushd configuration");

    for (key, value) in &config.env {
        debug!("Setting environment variable: {}={}", key, value);
        std::env::set_var(key, value);
    }

    trace!("Rushd configuration applied");
}
