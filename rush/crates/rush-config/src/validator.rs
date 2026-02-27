use std::fmt;
use std::path::Path;

use log::{debug, trace};

use crate::types::Config;

/// Represents a validation error for a configuration
#[derive(Debug)]
pub struct ConfigValidationError {
    message: String,
    field: Option<String>,
}

impl ConfigValidationError {
    /// Creates a new validation error
    pub fn new<S: Into<String>>(message: S) -> Self {
        ConfigValidationError {
            message: message.into(),
            field: None,
        }
    }

    /// Creates a new validation error with a field name
    pub fn with_field<S: Into<String>, F: Into<String>>(message: S, field: F) -> Self {
        ConfigValidationError {
            message: message.into(),
            field: Some(field.into()),
        }
    }
}

impl fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(field) = &self.field {
            write!(f, "Configuration error in '{}': {}", field, self.message)
        } else {
            write!(f, "Configuration error: {}", self.message)
        }
    }
}

impl std::error::Error for ConfigValidationError {}

/// Validates a configuration object to ensure all required values are present and valid
pub fn validate_config(config: &Config) -> Result<(), ConfigValidationError> {
    trace!("Validating configuration");

    // Validate environment
    let valid_environments = ["local", "dev", "prod", "staging"];
    if !valid_environments.contains(&config.environment()) {
        return Err(ConfigValidationError::with_field(
            format!(
                "Invalid environment: '{}'. Valid values are: {:?}",
                config.environment(),
                valid_environments
            ),
            "environment",
        ));
    }

    // Validate product path exists
    if !Path::new(config.product_path()).exists() {
        return Err(ConfigValidationError::with_field(
            format!(
                "Product path does not exist: '{}'",
                config.product_path().display()
            ),
            "product_path",
        ));
    }

    // Validate docker registry is not empty
    if config.docker_registry().is_empty() {
        return Err(ConfigValidationError::with_field(
            "Docker registry cannot be empty",
            "docker_registry",
        ));
    }

    // Validate start port is in valid range
    let start_port = config.start_port();
    if start_port < 1024 {
        return Err(ConfigValidationError::with_field(
            format!("Invalid start port: {start_port}. Port must be at least 1024"),
            "start_port",
        ));
    }

    // Validate root path
    if !Path::new(config.root_path()).exists() {
        return Err(ConfigValidationError::with_field(
            format!("Root path does not exist: '{}'", config.root_path()),
            "root_path",
        ));
    }

    // Validate Kubernetes version is in valid format (e.g., "v1.24.0")
    let k8s_version = config.k8s_version();
    if !k8s_version.starts_with('v')
        || !k8s_version[1..]
            .split('.')
            .all(|s| s.parse::<u32>().is_ok())
    {
        return Err(ConfigValidationError::with_field(
            format!(
                "Invalid Kubernetes version format: '{k8s_version}'. Expected format: 'vX.Y.Z'"
            ),
            "k8s_version",
        ));
    }

    debug!("Configuration validation successful");
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::types::Config;

    #[test]
    #[ignore] // This test changes global directory state and requires environment setup
    fn test_validate_valid_config() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path().to_str().unwrap();

        // Create the products directory structure that Config::new expects
        let products_dir = temp_dir.path().join("products");
        std::fs::create_dir(&products_dir).unwrap();
        let product_dir = products_dir.join("io.wonop.test-product");
        std::fs::create_dir(&product_dir).unwrap();

        // Change to the temp directory so Config::new can find the products dir
        let _dir_guard = rush_utils::Directory::chpath(temp_dir.path());

        // Set required environment variables for the test
        std::env::set_var("DEV_CTX", "test-context");
        std::env::set_var("DEV_VAULT", "test-vault");
        std::env::set_var("K8S_ENCODER_DEV", "noop");
        std::env::set_var("K8S_VALIDATOR_DEV", "kubevalidator");
        std::env::set_var("K8S_VERSION_DEV", "v1.24.0");
        std::env::set_var("DEV_DOMAIN", "test.domain");
        std::env::set_var("INFRASTRUCTURE_REPOSITORY", "git@github.com:test/repo.git");

        // Create a valid config
        let config = Config::new(root_path, "test-product", "dev", "test-registry", 8080).unwrap();

        // Directory is automatically restored when _dir_guard is dropped

        // Validation should pass
        let result = validate_config(&config);
        assert!(result.is_ok());
    }

    #[test]
    #[should_panic(expected = "Invalid environment")]
    fn test_validate_invalid_environment() {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path().to_str().unwrap();

        // Create the products directory structure that Config::new expects
        let products_dir = temp_dir.path().join("products");
        std::fs::create_dir(&products_dir).unwrap();
        let product_dir = products_dir.join("io.wonop.test-product");
        std::fs::create_dir(&product_dir).unwrap();

        // Change to the temp directory so Config::new can find the products dir
        let _dir_guard = rush_utils::Directory::chpath(temp_dir.path());

        // Set required environment variables
        std::env::set_var("INVALID_CTX", "test-context");
        std::env::set_var("INVALID_VAULT", "test-vault");
        std::env::set_var("K8S_ENCODER_INVALID", "noop");
        std::env::set_var("K8S_VALIDATOR_INVALID", "kubevalidator");
        std::env::set_var("K8S_VERSION_INVALID", "v1.24.0");
        std::env::set_var("INVALID_DOMAIN", "test.domain");
        std::env::set_var("INFRASTRUCTURE_REPOSITORY", "git@github.com:test/repo.git");

        // This should panic with "Invalid environment"
        let _config =
            Config::new(root_path, "test-product", "invalid", "test-registry", 8080).unwrap();
    }
}
