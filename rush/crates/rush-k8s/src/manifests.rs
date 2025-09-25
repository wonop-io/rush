//! Kubernetes manifest generation and management
//!
//! This module provides functionality for generating, validating, and manipulating
//! Kubernetes resource manifests for deployment.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::{debug, info, warn};
use rush_build::BuildContext;
use rush_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_yaml::{self, Value};
use tera::{Context, Tera};

use crate::context::KubernetesContext;

/// Represents a Kubernetes manifest file
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Path to the original template file
    pub template_path: PathBuf,
    /// Path where the rendered manifest will be saved
    pub output_path: PathBuf,
    /// Manifest kind (Deployment, Service, etc.)
    pub kind: String,
    /// Manifest name
    pub name: String,
    /// Namespace for the resource
    pub namespace: Option<String>,
    /// Raw YAML content of the manifest
    pub content: String,
}

impl Manifest {
    /// Creates a new Manifest from file paths
    ///
    /// # Arguments
    ///
    /// * `template_path` - Path to the template file
    /// * `output_path` - Path where the rendered manifest will be saved
    pub fn new(template_path: &Path, output_path: &Path) -> Result<Self> {
        let content = fs::read_to_string(template_path)
            .map_err(|e| Error::Filesystem(format!("Failed to read manifest: {e}")))?;

        // Parse YAML to extract kind and name
        let yaml: Value = serde_yaml::from_str(&content)
            .map_err(|e| Error::InvalidInput(format!("Invalid YAML: {e}")))?;

        let kind = yaml
            .get("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::InvalidInput("Manifest missing 'kind' field".into()))?
            .to_string();

        let name = yaml
            .get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .ok_or_else(|| Error::InvalidInput("Manifest missing 'metadata.name' field".into()))?
            .to_string();

        let namespace = yaml
            .get("metadata")
            .and_then(|m| m.get("namespace"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());

        Ok(Manifest {
            template_path: template_path.to_path_buf(),
            output_path: output_path.to_path_buf(),
            kind,
            name,
            namespace,
            content,
        })
    }

    /// Renders the manifest using the provided context
    ///
    /// # Arguments
    ///
    /// * `context` - The build context for template rendering
    pub fn render(&self, context: &BuildContext) -> Result<String> {
        let mut tera = Tera::default();
        let template_name = self
            .template_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap_or("template");

        tera.add_raw_template(template_name, &self.content)
            .map_err(|e| Error::Template(format!("Failed to add template: {e}")))?;

        let tera_context = Context::from_serialize(context)
            .map_err(|e| Error::Template(format!("Failed to create context: {e}")))?;

        tera.render(template_name, &tera_context)
            .map_err(|e| Error::Template(format!("Failed to render template: {e}")))
    }

    /// Renders the manifest and writes it to the output path
    ///
    /// # Arguments
    ///
    /// * `context` - The build context for template rendering
    pub fn render_to_file(&self, context: &BuildContext) -> Result<()> {
        let rendered = self.render(context)?;

        // Create parent directories if they don't exist
        if let Some(parent) = self.output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::Filesystem(format!("Failed to create directories: {e}")))?;
        }

        fs::write(&self.output_path, rendered)
            .map_err(|e| Error::Filesystem(format!("Failed to write manifest: {e}")))?;

        debug!(
            "Rendered manifest {} to {}",
            self.template_path.display(),
            self.output_path.display()
        );
        Ok(())
    }

    /// Checks if the manifest contains sensitive data like Secret kinds
    pub fn contains_secrets(&self) -> bool {
        self.kind == "Secret" || self.content.contains("kind: Secret")
    }
}

/// Manages a collection of Kubernetes manifests
#[derive(Debug)]
pub struct ManifestCollection {
    /// Collection of manifests organized by component name
    manifests: HashMap<String, Vec<Manifest>>,
    /// Kubernetes context to use
    context: Option<Arc<KubernetesContext>>,
}

impl ManifestCollection {
    /// Creates a new empty manifest collection
    pub fn new(context: Option<Arc<KubernetesContext>>) -> Self {
        ManifestCollection {
            manifests: HashMap::new(),
            context,
        }
    }

    /// Adds a manifest to the collection
    ///
    /// # Arguments
    ///
    /// * `component_name` - Name of the component the manifest belongs to
    /// * `manifest` - The manifest to add
    pub fn add_manifest(&mut self, component_name: &str, manifest: Manifest) {
        self.manifests
            .entry(component_name.to_string())
            .or_default()
            .push(manifest);
    }

    /// Loads all manifests from a directory for a component
    ///
    /// # Arguments
    ///
    /// * `component_name` - Name of the component
    /// * `template_dir` - Directory containing manifest templates
    /// * `output_dir` - Directory where rendered manifests will be saved
    pub fn load_from_directory(
        &mut self,
        component_name: &str,
        template_dir: &Path,
        output_dir: &Path,
    ) -> Result<()> {
        if !template_dir.exists() {
            return Err(Error::InvalidInput(format!(
                "Template directory does not exist: {}",
                template_dir.display()
            )));
        }

        // Create output directory if it doesn't exist
        fs::create_dir_all(output_dir)
            .map_err(|e| Error::Filesystem(format!("Failed to create output directory: {e}")))?;

        let entries = fs::read_dir(template_dir)
            .map_err(|e| Error::Filesystem(format!("Failed to read directory: {e}")))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| Error::Filesystem(format!("Failed to read directory entry: {e}")))?;
            let path = entry.path();

            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext == "yaml" || ext == "yml")
            {
                let filename = path.file_name().unwrap().to_str().unwrap();
                let output_path = output_dir.join(filename);

                match Manifest::new(&path, &output_path) {
                    Ok(manifest) => {
                        info!(
                            "Loaded manifest {} ({}) for component {}",
                            manifest.name, manifest.kind, component_name
                        );
                        self.add_manifest(component_name, manifest);
                    }
                    Err(e) => {
                        warn!("Failed to load manifest {}: {}", path.display(), e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Renders all manifests for a component
    ///
    /// # Arguments
    ///
    /// * `component_name` - Name of the component
    /// * `context` - The build context for template rendering
    pub fn render_component(&self, component_name: &str, context: &BuildContext) -> Result<()> {
        if let Some(manifests) = self.manifests.get(component_name) {
            for manifest in manifests {
                manifest.render_to_file(context)?;
            }
            info!(
                "Rendered {} manifests for component {}",
                manifests.len(),
                component_name
            );
            Ok(())
        } else {
            Err(Error::InvalidInput(format!(
                "No manifests found for component {component_name}"
            )))
        }
    }

    /// Renders all manifests in the collection
    ///
    /// # Arguments
    ///
    /// * `context` - The build context for template rendering
    pub fn render_all(&self, context: &BuildContext) -> Result<()> {
        for (component_name, manifests) in &self.manifests {
            for manifest in manifests {
                manifest.render_to_file(context)?;
            }
            info!(
                "Rendered {} manifests for component {}",
                manifests.len(),
                component_name
            );
        }
        Ok(())
    }

    /// Gets all manifests for a component
    ///
    /// # Arguments
    ///
    /// * `component_name` - Name of the component
    pub fn get_manifests(&self, component_name: &str) -> Option<&Vec<Manifest>> {
        self.manifests.get(component_name)
    }

    /// Gets all components in the collection
    pub fn get_components(&self) -> Vec<String> {
        self.manifests.keys().cloned().collect()
    }

    /// Gets the total number of manifests in the collection
    pub fn count(&self) -> usize {
        self.manifests.values().map(|v| v.len()).sum()
    }

    /// Sets the Kubernetes context
    ///
    /// # Arguments
    ///
    /// * `context` - The Kubernetes context to use
    pub fn set_context(&mut self, context: Arc<KubernetesContext>) {
        self.context = Some(context);
    }

    /// Gets the current Kubernetes context
    pub fn get_context(&self) -> Option<&Arc<KubernetesContext>> {
        self.context.as_ref()
    }
}

/// Resource type for Kubernetes manifests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// API version
    pub api_version: String,
    /// Resource kind
    pub kind: String,
    /// Resource metadata
    pub metadata: ResourceMetadata,
    /// Resource specification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec: Option<serde_yaml::Value>,
    /// Resource data (for ConfigMaps and Secrets)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, String>>,
}

/// Metadata for Kubernetes resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetadata {
    /// Resource name
    pub name: String,
    /// Resource namespace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Resource labels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
    /// Resource annotations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;

    fn create_test_manifest(dir: &TempDir, filename: &str, content: &str) -> PathBuf {
        let path = dir.path().join(filename);
        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_manifest_loading() {
        let temp_dir = TempDir::new().unwrap();
        let template_content = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test-config
  namespace: test-ns
data:
  key: value
"#;
        let template_path = create_test_manifest(&temp_dir, "config.yaml", template_content);
        let output_path = temp_dir.path().join("output").join("config.yaml");

        let manifest = Manifest::new(&template_path, &output_path).unwrap();

        assert_eq!(manifest.kind, "ConfigMap");
        assert_eq!(manifest.name, "test-config");
        assert_eq!(manifest.namespace, Some("test-ns".to_string()));
    }

    #[test]
    fn test_manifest_collection() {
        let temp_dir = TempDir::new().unwrap();

        // Create a temporary directory for templates
        let template_dir = temp_dir.path().join("templates");
        fs::create_dir_all(&template_dir).unwrap();

        // Create a temporary directory for output
        let output_dir = temp_dir.path().join("output");
        fs::create_dir_all(&output_dir).unwrap();

        // Create test manifest files
        let deployment_content = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: test-deploy
  namespace: default
spec:
  replicas: 1
"#;

        let service_content = r#"
apiVersion: v1
kind: Service
metadata:
  name: test-svc
spec:
  ports:
  - port: 80
"#;

        let deployment_path =
            create_test_manifest(&temp_dir, "deployment.yaml", deployment_content);
        let service_path = create_test_manifest(&temp_dir, "service.yaml", service_content);

        // Create test manifest objects
        let deployment =
            Manifest::new(&deployment_path, &output_dir.join("deployment.yaml")).unwrap();
        let service = Manifest::new(&service_path, &output_dir.join("service.yaml")).unwrap();

        // Create a manifest collection and add the manifests
        let mut collection = ManifestCollection::new(None);
        collection.add_manifest("test-component", deployment);
        collection.add_manifest("test-component", service);

        // Test getting manifests for a component
        let manifests = collection.get_manifests("test-component").unwrap();
        assert_eq!(manifests.len(), 2);
        assert_eq!(manifests[0].kind, "Deployment");
        assert_eq!(manifests[1].kind, "Service");

        // Test getting components
        let components = collection.get_components();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0], "test-component");

        // Test count
        assert_eq!(collection.count(), 2);
    }
}
