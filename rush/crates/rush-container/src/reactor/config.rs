//! Configuration for the Container Reactor
//!
//! This module defines the configuration structure for the ContainerReactor,
//! including all settings needed for container lifecycle management.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::watcher::WatcherConfig;

/// Configuration for the ContainerReactor
#[derive(Debug, Clone)]
pub struct ContainerReactorConfig {
    /// Product name
    pub product_name: String,

    /// Root directory for the product
    pub product_dir: PathBuf,

    /// Docker network name to use
    pub network_name: String,

    /// Environment (dev, staging, prod)
    pub environment: String,

    /// Docker registry to use for images
    pub docker_registry: String,

    /// Components to redirect to external services
    pub redirected_components: HashMap<String, (String, u16)>,

    /// Components whose output should be silenced
    pub silenced_components: HashSet<String>,

    /// Whether to run in verbose mode
    pub verbose: bool,

    /// File watch configuration
    pub watch_config: WatcherConfig,

    /// Git hash for tagging images
    pub git_hash: String,

    /// Starting port number for services
    pub start_port: u16,
}

impl ContainerReactorConfig {
    /// Create a new configuration with required fields
    pub fn new(
        product_name: impl Into<String>,
        product_dir: PathBuf,
        network_name: impl Into<String>,
        environment: impl Into<String>,
    ) -> Self {
        Self {
            product_name: product_name.into(),
            product_dir,
            network_name: network_name.into(),
            environment: environment.into(),
            docker_registry: String::new(),
            redirected_components: HashMap::new(),
            silenced_components: HashSet::new(),
            verbose: false,
            watch_config: WatcherConfig::default(),
            git_hash: String::from("latest"),
            start_port: 3000,
        }
    }

    /// Builder method to set the Docker registry
    pub fn with_registry(mut self, registry: impl Into<String>) -> Self {
        self.docker_registry = registry.into();
        self
    }

    /// Builder method to set verbose mode
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Builder method to set the git hash
    pub fn with_git_hash(mut self, hash: impl Into<String>) -> Self {
        self.git_hash = hash.into();
        self
    }

    /// Builder method to set the starting port
    pub fn with_start_port(mut self, port: u16) -> Self {
        self.start_port = port;
        self
    }

    /// Builder method to set watch configuration
    pub fn with_watch_config(mut self, config: WatcherConfig) -> Self {
        self.watch_config = config;
        self
    }

    /// Add a redirected component
    pub fn add_redirect(
        mut self,
        component: impl Into<String>,
        host: impl Into<String>,
        port: u16,
    ) -> Self {
        self.redirected_components
            .insert(component.into(), (host.into(), port));
        self
    }

    /// Add a silenced component
    pub fn add_silenced(mut self, component: impl Into<String>) -> Self {
        self.silenced_components.insert(component.into());
        self
    }

    /// Check if a component is redirected
    pub fn is_redirected(&self, component: &str) -> bool {
        self.redirected_components.contains_key(component)
    }

    /// Check if a component is silenced
    pub fn is_silenced(&self, component: &str) -> bool {
        self.silenced_components.contains(component)
    }

    /// Get redirect configuration for a component
    pub fn get_redirect(&self, component: &str) -> Option<&(String, u16)> {
        self.redirected_components.get(component)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn test_config_creation() {
        let config = ContainerReactorConfig::new(
            "test-product",
            PathBuf::from("/test/path"),
            "test-network",
            "dev",
        );

        assert_eq!(config.product_name, "test-product");
        assert_eq!(config.product_dir, Path::new("/test/path"));
        assert_eq!(config.network_name, "test-network");
        assert_eq!(config.environment, "dev");
        assert_eq!(config.start_port, 3000);
        assert!(!config.verbose);
    }

    #[test]
    fn test_builder_methods() {
        let config = ContainerReactorConfig::new("test", PathBuf::from("/test"), "network", "prod")
            .with_registry("registry.example.com")
            .with_verbose(true)
            .with_git_hash("abc123")
            .with_start_port(8080);

        assert_eq!(config.docker_registry, "registry.example.com");
        assert!(config.verbose);
        assert_eq!(config.git_hash, "abc123");
        assert_eq!(config.start_port, 8080);
    }

    #[test]
    fn test_redirects() {
        let config = ContainerReactorConfig::new("test", PathBuf::from("/test"), "network", "dev")
            .add_redirect("frontend", "localhost", 3000)
            .add_redirect("backend", "localhost", 8080);

        assert!(config.is_redirected("frontend"));
        assert!(config.is_redirected("backend"));
        assert!(!config.is_redirected("other"));

        assert_eq!(
            config.get_redirect("frontend"),
            Some(&("localhost".to_string(), 3000))
        );
        assert_eq!(
            config.get_redirect("backend"),
            Some(&("localhost".to_string(), 8080))
        );
        assert_eq!(config.get_redirect("other"), None);
    }

    #[test]
    fn test_silenced_components() {
        let config = ContainerReactorConfig::new("test", PathBuf::from("/test"), "network", "dev")
            .add_silenced("noisy-service")
            .add_silenced("debug-service");

        assert!(config.is_silenced("noisy-service"));
        assert!(config.is_silenced("debug-service"));
        assert!(!config.is_silenced("normal-service"));
    }
}

impl Default for ContainerReactorConfig {
    fn default() -> Self {
        Self {
            product_name: String::new(),
            product_dir: PathBuf::new(),
            network_name: "rush".to_string(),
            environment: "dev".to_string(),
            docker_registry: "localhost".to_string(),
            redirected_components: HashMap::new(),
            silenced_components: HashSet::new(),
            verbose: false,
            watch_config: WatcherConfig::default(),
            git_hash: String::new(),
            start_port: 8000,
        }
    }
}
