use log::{debug, error, trace};
use rush_core::constants::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::types::Config;
use rush_utils::{find_project_root, read_to_string};

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
        let project_root =
            find_project_root().ok_or_else(|| "Failed to find project root".to_string())?;

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
        if !VALID_ENVIRONMENTS.contains(&environment) {
            let err_msg = format!("Invalid environment: {environment}");
            error!("{}", err_msg);
            error!("Valid environments: {:?}", VALID_ENVIRONMENTS);
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
        let config_path = self.root_path.join(RUSHD_CONFIG_FILE);
        trace!("Loading rushd config from: {}", config_path.display());

        let content =
            read_to_string(&config_path).map_err(|e| format!("Failed to read rushd.yaml: {e}"))?;

        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse rushd.yaml: {e}"))
    }
}

/// Configuration loaded from rushd.yaml
#[derive(Debug, Deserialize, Serialize)]
pub struct RushdConfig {
    pub env: std::collections::HashMap<String, String>,
    /// Cross-compilation method: "native" (default) or "cross-rs"
    #[serde(default = "default_cross_compile")]
    pub cross_compile: String,
    /// Development output configuration
    #[serde(default)]
    pub dev_output: DevOutputConfig,
}

/// Development output configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DevOutputConfig {
    /// Output mode
    #[serde(default = "default_output_mode")]
    pub mode: String,

    /// Component filtering
    #[serde(default)]
    pub components: ComponentFilterConfig,

    /// Phase filtering
    #[serde(default)]
    pub phases: PhaseFilterConfig,

    /// Log level filtering
    #[serde(default = "default_log_level")]
    pub log_level: String,

    /// Color configuration
    #[serde(default)]
    pub colors: ColorConfig,

    /// File logging configuration
    #[serde(default)]
    pub file_log: Option<FileLogConfig>,

    /// Web view configuration
    #[serde(default)]
    pub web: WebConfig,
}

impl Default for DevOutputConfig {
    fn default() -> Self {
        Self {
            mode: default_output_mode(),
            components: ComponentFilterConfig::default(),
            phases: PhaseFilterConfig::default(),
            log_level: default_log_level(),
            colors: ColorConfig::default(),
            file_log: None,
            web: WebConfig::default(),
        }
    }
}

/// Component filter configuration
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ComponentFilterConfig {
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

/// Phase filter configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PhaseFilterConfig {
    #[serde(default = "default_true")]
    pub show_build: bool,
    #[serde(default = "default_true")]
    pub show_runtime: bool,
    #[serde(default = "default_true")]
    pub show_system: bool,
}

impl Default for PhaseFilterConfig {
    fn default() -> Self {
        Self {
            show_build: true,
            show_runtime: true,
            show_system: true,
        }
    }
}

/// Color configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ColorConfig {
    #[serde(default = "default_color_enabled")]
    pub enabled: String,
    #[serde(default = "default_color_theme")]
    pub theme: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            enabled: default_color_enabled(),
            theme: default_color_theme(),
        }
    }
}

/// File logging configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FileLogConfig {
    pub enabled: bool,
    pub path: String,
}

/// Web view configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WebConfig {
    #[serde(default = "default_web_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub open_browser: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            port: default_web_port(),
            open_browser: true,
        }
    }
}

fn default_output_mode() -> String {
    "auto".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_color_enabled() -> String {
    "auto".to_string()
}

fn default_color_theme() -> String {
    "default".to_string()
}

fn default_web_port() -> u16 {
    8080
}

fn default_true() -> bool {
    true
}

fn default_cross_compile() -> String {
    "native".to_string()
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

#[cfg(test)]
mod rushd_config_tests {
    use super::*;

    #[test]
    fn test_rushd_config_default_cross_compile() {
        let yaml_str = r#"
env:
  TEST_VAR: test_value
"#;
        let config: RushdConfig = serde_yaml::from_str(yaml_str).unwrap();
        assert_eq!(config.cross_compile, "native");
    }

    #[test]
    fn test_rushd_config_cross_rs() {
        let yaml_str = r#"
env:
  TEST_VAR: test_value
cross_compile: cross-rs
"#;
        let config: RushdConfig = serde_yaml::from_str(yaml_str).unwrap();
        assert_eq!(config.cross_compile, "cross-rs");
    }

    #[test]
    fn test_rushd_config_native_explicit() {
        let yaml_str = r#"
env:
  TEST_VAR: test_value
cross_compile: native
"#;
        let config: RushdConfig = serde_yaml::from_str(yaml_str).unwrap();
        assert_eq!(config.cross_compile, "native");
    }
}
