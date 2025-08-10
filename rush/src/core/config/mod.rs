//! Configuration management for Rush CLI
//!
//! This module provides utilities for loading, validating, and working with
//! Rush CLI configuration from various sources.

mod loader;
mod types;
mod validator;

// Re-export types and functions needed by other modules
pub use self::loader::{apply_rushd_config, ConfigLoader, RushdConfig};
pub use self::types::{Config, DomainContext};
pub use self::validator::{validate_config, ConfigValidationError};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    #[ignore] // This test changes global directory state and requires environment setup
    fn test_config_lifecycle() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();
        
        // Create the products directory structure that Config::new expects
        let products_dir = temp_dir.path().join("products");
        std::fs::create_dir(&products_dir).unwrap();
        let product_dir = products_dir.join("io.wonop.test-product");
        std::fs::create_dir(&product_dir).unwrap();
        
        // Change to the temp directory so Config::new can find the products dir
        let _dir_guard = crate::utils::Directory::chpath(&temp_dir);

        // Set required environment variables for testing
        std::env::set_var("DEV_CTX", "test-context");
        std::env::set_var("DEV_VAULT", "test-vault");
        std::env::set_var("DEV_DOMAIN", "{{ product_uri }}.dev.example.com");
        std::env::set_var("K8S_ENCODER_DEV", "noop");
        std::env::set_var("K8S_VALIDATOR_DEV", "kubevalidator");
        std::env::set_var("K8S_VERSION_DEV", "v1.24.0");
        std::env::set_var("INFRASTRUCTURE_REPOSITORY", "git@github.com:test/repo.git");

        // Create a ConfigLoader
        let config_loader = ConfigLoader::new(root_path);

        // Load a test configuration
        let config = config_loader
            .load_config("test-product", "dev", "test-registry.io", 8080)
            .expect("Failed to load configuration");
        
        // Directory is automatically restored when _dir_guard is dropped

        // Validate the configuration
        assert_eq!(config.product_name(), "test-product");
        assert_eq!(config.environment(), "dev");
        assert_eq!(config.docker_registry(), "test-registry.io");
        assert_eq!(config.start_port(), 8080);

        // Test domain generation
        let domain = config.domain(Some("api".to_string()));
        assert!(domain.contains("api"));
        assert!(domain.contains("dev.example.com"));
    }
}
