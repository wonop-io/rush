use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::{debug, error, trace, warn};
use serde_yaml::Value;

use crate::core::config::Config;
use crate::core::product::{Product, ProductComponent};
use crate::utils::read_to_string;

/// Loads product definitions from a product directory
pub struct ProductLoader {
    product_path: PathBuf,
}

impl ProductLoader {
    /// Creates a new ProductLoader for the specified product path
    pub fn new<P: AsRef<Path>>(product_path: P) -> Self {
        Self {
            product_path: product_path.as_ref().to_path_buf(),
        }
    }

    /// Loads a product definition from the stack.spec.yaml file
    pub fn load_product(&self, config: Arc<Config>) -> Result<Product, String> {
        trace!("Loading product from {}", self.product_path.display());

        let stack_spec_path = self.product_path.join("stack.spec.yaml");
        if !stack_spec_path.exists() {
            let err_msg = format!(
                "Stack specification file not found at {}",
                stack_spec_path.display()
            );
            error!("{}", err_msg);
            return Err(err_msg);
        }

        let spec_content = match read_to_string(&stack_spec_path) {
            Ok(content) => content,
            Err(e) => {
                let err_msg = format!("Failed to read stack specification: {}", e);
                error!("{}", err_msg);
                return Err(err_msg);
            }
        };

        let spec_value: Value = match serde_yaml::from_str(&spec_content) {
            Ok(value) => value,
            Err(e) => {
                let err_msg = format!("Failed to parse stack specification: {}", e);
                error!("{}", err_msg);
                return Err(err_msg);
            }
        };

        let components = self.parse_components(&spec_value, &config)?;
        debug!(
            "Loaded {} components for product {}",
            components.len(),
            config.product_name()
        );

        // Unwrap the Arc<Product> to get Product
        Ok((*Product::load(&self.product_path).unwrap()).clone())
    }

    fn parse_components(
        &self,
        spec: &Value,
        config: &Config,
    ) -> Result<HashMap<String, ProductComponent>, String> {
        let mut components = HashMap::new();

        if let Some(mapping) = spec.as_mapping() {
            for (name_value, component_spec) in mapping {
                if let Some(name) = name_value.as_str() {
                    trace!("Parsing component: {}", name);

                    // Extract component fields from YAML
                    let location = match component_spec.get("location") {
                        Some(loc) => match loc.as_str() {
                            Some(path) => PathBuf::from(path),
                            None => {
                                warn!("Component {} has invalid location, skipping", name);
                                continue;
                            }
                        },
                        None => {
                            warn!("Component {} has no location, skipping", name);
                            continue;
                        }
                    };

                    // Create ProductComponent object
                    let component = ProductComponent {
                        name: name.to_string(),
                        location: self
                            .product_path
                            .join(&location)
                            .to_str()
                            .unwrap()
                            .to_string(),
                        build_type: "RustBinary".to_string(),
                        dockerfile_path: None,
                        port: None,
                        target_port: None,
                        depends_on: Vec::new(),
                        env: None,
                        k8s_path: None,
                        priority: 100,
                    };

                    components.insert(name.to_string(), component);
                    debug!("Added component: {}", name);
                }
            }
        }

        Ok(components)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_load_product() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().unwrap();
        let product_path = temp_dir.path();

        // Create a sample stack.spec.yaml
        let spec_content = r#"
component1:
  location: "src/component1"
  build_type: "RustBinary"

component2:
  location: "src/component2"
  build_type: "TrunkWasm"
"#;

        let spec_path = product_path.join("stack.spec.yaml");
        let mut file = File::create(&spec_path).unwrap();
        file.write_all(spec_content.as_bytes()).unwrap();

        // Create the component directories
        fs::create_dir_all(product_path.join("src/component1")).unwrap();
        fs::create_dir_all(product_path.join("src/component2")).unwrap();

        // Create a mock config
        let config = Config::new(
            product_path.to_str().unwrap(),
            "test-product",
            "dev",
            "test-registry",
            8080,
        )
        .unwrap();

        // Use the ProductLoader
        let loader = ProductLoader::new(product_path);
        let product = loader.load_product(config).unwrap();

        // Verify results
        assert_eq!(product.name(), "test-product");
        assert_eq!(product.components().len(), 2);
        assert!(product.components().contains_key("component1"));
        assert!(product.components().contains_key("component2"));

        let component1 = &product.components()["component1"];
        assert_eq!(component1.name(), "component1");
        assert_eq!(
            component1.location(),
            &format!("{}/src/component1", product_path.to_str().unwrap())
        );
    }

    #[test]
    fn test_missing_spec_file() {
        let temp_dir = TempDir::new().unwrap();
        let product_path = temp_dir.path();

        let config = Config::new(
            product_path.to_str().unwrap(),
            "test-product",
            "dev",
            "test-registry",
            8080,
        )
        .unwrap();

        let loader = ProductLoader::new(product_path);
        let result = loader.load_product(config);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Stack specification file not found"));
    }
}
