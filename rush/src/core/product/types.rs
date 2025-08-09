use log::trace;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Represents a product in the Rush CLI ecosystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    /// The name of the product
    name: String,

    /// The URI-friendly identifier for the product
    uri: String,

    /// The directory name where the product is stored
    dirname: String,

    /// The absolute path to the product's root directory
    path: PathBuf,

    /// The components that make up this product
    components: HashMap<String, ProductComponent>,

    /// The configuration values specific to this product
    config: ProductConfig,
}

/// Represents a component within a product.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductComponent {
    /// The name of the component
    pub name: String,

    /// The type of build for this component
    pub build_type: String,

    /// The relative location of the component within the product directory
    pub location: String,

    /// The path to the component's Dockerfile, if applicable
    pub dockerfile_path: Option<String>,

    /// Optional port this component exposes
    pub port: Option<u16>,

    /// Optional target port for container/k8s configuration
    pub target_port: Option<u16>,

    /// Dependencies of this component
    pub depends_on: Vec<String>,

    /// Custom environment variables for this component
    pub env: Option<HashMap<String, String>>,

    /// Path to Kubernetes manifests, if applicable
    pub k8s_path: Option<String>,

    /// Priority for deployment ordering
    pub priority: u64,
}

/// Configuration settings specific to a product.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductConfig {
    /// Default environment for this product
    default_environment: String,

    /// Start port number for auto-assigned component ports
    start_port: u16,

    /// Docker registry for container images
    docker_registry: String,

    /// Domain templates for various environments
    domains: HashMap<String, String>,
}

impl Product {
    /// Creates a new Product instance from a directory path
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Arc<Self>, String> {
        let path = path.as_ref();
        trace!("Loading product from path: {}", path.display());

        let stack_spec_path = path.join("stack.spec.yaml");
        if !stack_spec_path.exists() {
            return Err(format!("stack.spec.yaml not found in {}", path.display()));
        }

        // In a real implementation, this would parse the stack.spec.yaml file
        // and construct a Product instance with components

        // For now, we'll create a placeholder implementation
        let product_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();

        let uri = product_name.to_lowercase().replace('.', "-");

        let product = Self {
            name: product_name.clone(),
            uri,
            dirname: product_name,
            path: path.to_path_buf(),
            components: HashMap::new(), // Would be populated from stack.spec.yaml
            config: ProductConfig {
                default_environment: "local".to_string(),
                start_port: 8080,
                docker_registry: "docker.io".to_string(),
                domains: HashMap::new(),
            },
        };

        trace!("Successfully loaded product: {}", product.name);
        Ok(Arc::new(product))
    }

    // Getters

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn components(&self) -> &HashMap<String, ProductComponent> {
        &self.components
    }

    pub fn config(&self) -> &ProductConfig {
        &self.config
    }
}

impl ProductConfig {
    pub fn default_environment(&self) -> &str {
        &self.default_environment
    }

    pub fn start_port(&self) -> u16 {
        self.start_port
    }

    pub fn docker_registry(&self) -> &str {
        &self.docker_registry
    }

    pub fn domains(&self) -> &HashMap<String, String> {
        &self.domains
    }
}

impl ProductComponent {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn build_type(&self) -> &str {
        &self.build_type
    }

    pub fn location(&self) -> &str {
        &self.location
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn target_port(&self) -> Option<u16> {
        self.target_port
    }

    pub fn depends_on(&self) -> &[String] {
        &self.depends_on
    }

    pub fn k8s_path(&self) -> Option<&String> {
        self.k8s_path.as_ref()
    }

    pub fn priority(&self) -> u64 {
        self.priority
    }
}
