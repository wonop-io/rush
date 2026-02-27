//! Kubernetes manifest generation from component specifications
//!
//! This module converts Rush component specifications into Kubernetes manifests
//! including Deployments, Services, Ingresses, ConfigMaps, and Secrets.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use log::{debug, info};
use rush_build::{BuildType, ComponentBuildSpec};
use rush_core::error::Result;
use serde_yaml;

/// Kubernetes manifest generator
pub struct ManifestGenerator {
    /// Output directory for generated manifests
    output_dir: PathBuf,
    /// Namespace for resources
    namespace: String,
    /// Registry configuration for image references
    registry_url: Option<String>,
    registry_namespace: Option<String>,
    /// Environment name
    environment: String,
}

impl ManifestGenerator {
    /// Create a new manifest generator
    pub fn new(output_dir: PathBuf, namespace: String, environment: String) -> Self {
        Self {
            output_dir,
            namespace,
            environment,
            registry_url: None,
            registry_namespace: None,
        }
    }

    /// Set registry configuration for image references
    pub fn with_registry(mut self, url: Option<String>, namespace: Option<String>) -> Self {
        self.registry_url = url;
        self.registry_namespace = namespace;
        self
    }

    /// Generate manifests for all components
    pub async fn generate_manifests(
        &self,
        components: &[ComponentBuildSpec],
        secrets: Option<BTreeMap<String, String>>,
    ) -> Result<Vec<GeneratedManifest>> {
        info!(
            "Generating Kubernetes manifests for {} components",
            components.len()
        );

        // Create output directory
        fs::create_dir_all(&self.output_dir)?;

        let mut manifests = Vec::new();

        // Generate deployment and service for each component
        for component in components {
            // Skip components that don't need K8s resources
            match &component.build_type {
                BuildType::LocalService { .. } => continue,
                BuildType::PureDockerImage { .. } => {
                    // PureDockerImage still needs deployment
                }
                _ => {}
            }

            // Generate deployment (pass whether secrets exist)
            if let Some(deployment) = self.generate_deployment(component, secrets.is_some())? {
                let path = self
                    .output_dir
                    .join(format!("{}-deployment.yaml", component.component_name));
                fs::write(&path, &deployment.content)?;
                debug!("Generated deployment manifest: {path:?}");
                manifests.push(deployment);
            }

            // Generate service if component has ports
            if let Some(service) = self.generate_service(component)? {
                let path = self
                    .output_dir
                    .join(format!("{}-service.yaml", component.component_name));
                fs::write(&path, &service.content)?;
                debug!("Generated service manifest: {path:?}");
                manifests.push(service);
            }
        }

        // Generate ingress if any component needs external access
        if let Some(ingress) = self.generate_ingress(components)? {
            let path = self.output_dir.join("ingress.yaml");
            fs::write(&path, &ingress.content)?;
            debug!("Generated ingress manifest: {path:?}");
            manifests.push(ingress);
        }

        // Generate secrets if provided
        if let Some(secrets_data) = secrets {
            if let Some(secret) = self.generate_secret(secrets_data)? {
                let path = self.output_dir.join("secrets.yaml");
                fs::write(&path, &secret.content)?;
                debug!("Generated secrets manifest: {path:?}");
                manifests.push(secret);
            }
        }

        info!("Generated {} Kubernetes manifests", manifests.len());
        Ok(manifests)
    }

    /// Helper to convert BTreeMap to serde_yaml::Mapping
    fn btree_to_mapping(map: BTreeMap<String, serde_yaml::Value>) -> serde_yaml::Mapping {
        let mut mapping = serde_yaml::Mapping::new();
        for (k, v) in map {
            mapping.insert(serde_yaml::Value::String(k), v);
        }
        mapping
    }

    /// Generate a Deployment manifest for a component
    fn generate_deployment(
        &self,
        component: &ComponentBuildSpec,
        has_secrets: bool,
    ) -> Result<Option<GeneratedManifest>> {
        let mut deployment = BTreeMap::new();
        deployment.insert(
            "apiVersion".to_string(),
            serde_yaml::Value::String("apps/v1".to_string()),
        );
        deployment.insert(
            "kind".to_string(),
            serde_yaml::Value::String("Deployment".to_string()),
        );

        // Metadata
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "name".to_string(),
            serde_yaml::Value::String(component.component_name.clone()),
        );
        metadata.insert(
            "namespace".to_string(),
            serde_yaml::Value::String(self.namespace.clone()),
        );

        let mut labels = BTreeMap::new();
        labels.insert(
            "app".to_string(),
            serde_yaml::Value::String(component.component_name.clone()),
        );
        labels.insert(
            "env".to_string(),
            serde_yaml::Value::String(self.environment.clone()),
        );
        metadata.insert(
            "labels".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(labels)),
        );

        deployment.insert(
            "metadata".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(metadata)),
        );

        // Spec
        let mut spec = BTreeMap::new();
        spec.insert(
            "replicas".to_string(),
            serde_yaml::Value::Number(serde_yaml::Number::from(1)),
        );

        // Selector
        let mut selector = BTreeMap::new();
        let mut match_labels = BTreeMap::new();
        match_labels.insert(
            "app".to_string(),
            serde_yaml::Value::String(component.component_name.clone()),
        );
        selector.insert(
            "matchLabels".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(match_labels)),
        );
        spec.insert(
            "selector".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(selector)),
        );

        // Template
        let mut template = BTreeMap::new();

        // Template metadata
        let mut template_metadata = BTreeMap::new();
        let mut template_labels = BTreeMap::new();
        template_labels.insert(
            "app".to_string(),
            serde_yaml::Value::String(component.component_name.clone()),
        );
        template_metadata.insert(
            "labels".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(template_labels)),
        );
        template.insert(
            "metadata".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(template_metadata)),
        );

        // Template spec
        let mut template_spec = BTreeMap::new();

        // Container
        let mut container = BTreeMap::new();
        container.insert(
            "name".to_string(),
            serde_yaml::Value::String(component.component_name.clone()),
        );

        // Determine image
        let image = self.get_image_name(component)?;
        container.insert("image".to_string(), serde_yaml::Value::String(image));
        container.insert(
            "imagePullPolicy".to_string(),
            serde_yaml::Value::String("Always".to_string()),
        );

        // Ports if specified
        if let Some(port) = component.port {
            let mut port_spec = BTreeMap::new();
            port_spec.insert(
                "containerPort".to_string(),
                serde_yaml::Value::Number(serde_yaml::Number::from(port)),
            );

            let ports = vec![serde_yaml::Value::Mapping(Self::btree_to_mapping(
                port_spec,
            ))];
            container.insert("ports".to_string(), serde_yaml::Value::Sequence(ports));
        }

        // Environment variables
        let mut env_vars = Vec::new();

        // Add ENVIRONMENT variable
        let mut env_var = BTreeMap::new();
        env_var.insert(
            "name".to_string(),
            serde_yaml::Value::String("ENVIRONMENT".to_string()),
        );
        env_var.insert(
            "value".to_string(),
            serde_yaml::Value::String(self.environment.clone()),
        );
        env_vars.push(serde_yaml::Value::Mapping(Self::btree_to_mapping(env_var)));

        // Add custom env vars from component
        if let Some(env) = &component.env {
            for (key, value) in env {
                let mut env_var = BTreeMap::new();
                env_var.insert("name".to_string(), serde_yaml::Value::String(key.clone()));
                env_var.insert(
                    "value".to_string(),
                    serde_yaml::Value::String(value.clone()),
                );
                env_vars.push(serde_yaml::Value::Mapping(Self::btree_to_mapping(env_var)));
            }
        }

        // Add envFrom to reference all secrets if they exist
        if has_secrets {
            let mut env_from = Vec::new();
            let mut secret_ref = BTreeMap::new();
            let mut secret_ref_inner = BTreeMap::new();
            secret_ref_inner.insert(
                "name".to_string(),
                serde_yaml::Value::String(format!("{}-secrets", self.environment)),
            );
            secret_ref.insert(
                "secretRef".to_string(),
                serde_yaml::Value::Mapping(Self::btree_to_mapping(secret_ref_inner)),
            );
            env_from.push(serde_yaml::Value::Mapping(Self::btree_to_mapping(
                secret_ref,
            )));
            container.insert("envFrom".to_string(), serde_yaml::Value::Sequence(env_from));
        }

        if !env_vars.is_empty() {
            container.insert("env".to_string(), serde_yaml::Value::Sequence(env_vars));
        }

        // Add volume mount for secrets if needed
        if has_secrets
            && std::env::var("K8S_MOUNT_SECRETS").unwrap_or_else(|_| "false".to_string()) == "true"
        {
            // Add volumeMounts to container
            let mut volume_mounts = Vec::new();
            let mut volume_mount = BTreeMap::new();
            volume_mount.insert(
                "name".to_string(),
                serde_yaml::Value::String("secrets".to_string()),
            );
            volume_mount.insert(
                "mountPath".to_string(),
                serde_yaml::Value::String("/etc/secrets".to_string()),
            );
            volume_mount.insert("readOnly".to_string(), serde_yaml::Value::Bool(true));
            volume_mounts.push(serde_yaml::Value::Mapping(Self::btree_to_mapping(
                volume_mount,
            )));
            container.insert(
                "volumeMounts".to_string(),
                serde_yaml::Value::Sequence(volume_mounts),
            );
        }

        let containers = vec![serde_yaml::Value::Mapping(Self::btree_to_mapping(
            container,
        ))];
        template_spec.insert(
            "containers".to_string(),
            serde_yaml::Value::Sequence(containers),
        );

        // Add volumes to pod spec if secrets are mounted
        if has_secrets
            && std::env::var("K8S_MOUNT_SECRETS").unwrap_or_else(|_| "false".to_string()) == "true"
        {
            let mut volumes = Vec::new();
            let mut volume = BTreeMap::new();
            volume.insert(
                "name".to_string(),
                serde_yaml::Value::String("secrets".to_string()),
            );

            let mut secret = BTreeMap::new();
            secret.insert(
                "secretName".to_string(),
                serde_yaml::Value::String(format!("{}-secrets", self.environment)),
            );
            volume.insert(
                "secret".to_string(),
                serde_yaml::Value::Mapping(Self::btree_to_mapping(secret)),
            );

            volumes.push(serde_yaml::Value::Mapping(Self::btree_to_mapping(volume)));
            template_spec.insert("volumes".to_string(), serde_yaml::Value::Sequence(volumes));
        }

        template.insert(
            "spec".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(template_spec)),
        );

        spec.insert(
            "template".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(template)),
        );

        deployment.insert(
            "spec".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(spec)),
        );

        let manifest = GeneratedManifest {
            kind: ManifestKind::Deployment,
            name: component.component_name.clone(),
            namespace: self.namespace.clone(),
            content: serde_yaml::to_string(&deployment)
                .map_err(|e| anyhow::anyhow!("Failed to serialize deployment: {e}"))?,
        };

        Ok(Some(manifest))
    }

    /// Generate a Service manifest for a component
    fn generate_service(
        &self,
        component: &ComponentBuildSpec,
    ) -> Result<Option<GeneratedManifest>> {
        // Only generate service if component has a port
        let port = match component.port {
            Some(p) => p,
            None => return Ok(None),
        };

        let mut service = BTreeMap::new();
        service.insert(
            "apiVersion".to_string(),
            serde_yaml::Value::String("v1".to_string()),
        );
        service.insert(
            "kind".to_string(),
            serde_yaml::Value::String("Service".to_string()),
        );

        // Metadata
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "name".to_string(),
            serde_yaml::Value::String(component.component_name.clone()),
        );
        metadata.insert(
            "namespace".to_string(),
            serde_yaml::Value::String(self.namespace.clone()),
        );

        let mut labels = BTreeMap::new();
        labels.insert(
            "app".to_string(),
            serde_yaml::Value::String(component.component_name.clone()),
        );
        metadata.insert(
            "labels".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(labels)),
        );

        service.insert(
            "metadata".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(metadata)),
        );

        // Spec
        let mut spec = BTreeMap::new();
        spec.insert(
            "type".to_string(),
            serde_yaml::Value::String("ClusterIP".to_string()),
        );

        // Selector
        let mut selector = BTreeMap::new();
        selector.insert(
            "app".to_string(),
            serde_yaml::Value::String(component.component_name.clone()),
        );
        spec.insert(
            "selector".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(selector)),
        );

        // Ports
        let mut port_spec = BTreeMap::new();
        port_spec.insert(
            "port".to_string(),
            serde_yaml::Value::Number(serde_yaml::Number::from(port)),
        );
        port_spec.insert(
            "targetPort".to_string(),
            serde_yaml::Value::Number(serde_yaml::Number::from(port)),
        );
        port_spec.insert(
            "protocol".to_string(),
            serde_yaml::Value::String("TCP".to_string()),
        );

        spec.insert(
            "ports".to_string(),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(Self::btree_to_mapping(
                port_spec,
            ))]),
        );

        service.insert(
            "spec".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(spec)),
        );

        let manifest = GeneratedManifest {
            kind: ManifestKind::Service,
            name: component.component_name.clone(),
            namespace: self.namespace.clone(),
            content: serde_yaml::to_string(&service)
                .map_err(|e| anyhow::anyhow!("Failed to serialize service: {e}"))?,
        };

        Ok(Some(manifest))
    }

    /// Generate an Ingress manifest for exposed components
    fn generate_ingress(
        &self,
        components: &[ComponentBuildSpec],
    ) -> Result<Option<GeneratedManifest>> {
        // Filter components that have mount points (exposed via ingress)
        let exposed_components: Vec<_> = components
            .iter()
            .filter(|c| c.mount_point.is_some())
            .collect();

        if exposed_components.is_empty() {
            return Ok(None);
        }

        let mut ingress = BTreeMap::new();
        ingress.insert(
            "apiVersion".to_string(),
            serde_yaml::Value::String("networking.k8s.io/v1".to_string()),
        );
        ingress.insert(
            "kind".to_string(),
            serde_yaml::Value::String("Ingress".to_string()),
        );

        // Metadata
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "name".to_string(),
            serde_yaml::Value::String(format!("{}-ingress", self.environment)),
        );
        metadata.insert(
            "namespace".to_string(),
            serde_yaml::Value::String(self.namespace.clone()),
        );

        let mut annotations = BTreeMap::new();
        annotations.insert(
            "kubernetes.io/ingress.class".to_string(),
            serde_yaml::Value::String("nginx".to_string()),
        );
        metadata.insert(
            "annotations".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(annotations)),
        );

        ingress.insert(
            "metadata".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(metadata)),
        );

        // Spec
        let mut spec = BTreeMap::new();

        // Rules
        let mut rule = BTreeMap::new();
        let mut http = BTreeMap::new();
        let mut paths = Vec::new();

        for component in exposed_components {
            if let Some(mount_point) = &component.mount_point {
                let mut path_item = BTreeMap::new();
                path_item.insert(
                    "path".to_string(),
                    serde_yaml::Value::String(mount_point.clone()),
                );
                path_item.insert(
                    "pathType".to_string(),
                    serde_yaml::Value::String("Prefix".to_string()),
                );

                let mut backend = BTreeMap::new();
                let mut service = BTreeMap::new();
                service.insert(
                    "name".to_string(),
                    serde_yaml::Value::String(component.component_name.clone()),
                );

                let mut port = BTreeMap::new();
                port.insert(
                    "number".to_string(),
                    serde_yaml::Value::Number(serde_yaml::Number::from(
                        component.port.unwrap_or(8080),
                    )),
                );
                service.insert(
                    "port".to_string(),
                    serde_yaml::Value::Mapping(Self::btree_to_mapping(port)),
                );

                backend.insert(
                    "service".to_string(),
                    serde_yaml::Value::Mapping(Self::btree_to_mapping(service)),
                );

                path_item.insert(
                    "backend".to_string(),
                    serde_yaml::Value::Mapping(Self::btree_to_mapping(backend)),
                );

                paths.push(serde_yaml::Value::Mapping(Self::btree_to_mapping(
                    path_item,
                )));
            }
        }

        http.insert("paths".to_string(), serde_yaml::Value::Sequence(paths));
        rule.insert(
            "http".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(http)),
        );

        spec.insert(
            "rules".to_string(),
            serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(Self::btree_to_mapping(
                rule,
            ))]),
        );

        ingress.insert(
            "spec".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(spec)),
        );

        let manifest = GeneratedManifest {
            kind: ManifestKind::Ingress,
            name: format!("{}-ingress", self.environment),
            namespace: self.namespace.clone(),
            content: serde_yaml::to_string(&ingress)
                .map_err(|e| anyhow::anyhow!("Failed to serialize ingress: {e}"))?,
        };

        Ok(Some(manifest))
    }

    /// Generate a ConfigMap manifest
    pub fn generate_configmap(
        &self,
        name: String,
        data: BTreeMap<String, String>,
    ) -> Result<GeneratedManifest> {
        let mut configmap = BTreeMap::new();
        configmap.insert(
            "apiVersion".to_string(),
            serde_yaml::Value::String("v1".to_string()),
        );
        configmap.insert(
            "kind".to_string(),
            serde_yaml::Value::String("ConfigMap".to_string()),
        );

        // Metadata
        let mut metadata = BTreeMap::new();
        metadata.insert("name".to_string(), serde_yaml::Value::String(name.clone()));
        metadata.insert(
            "namespace".to_string(),
            serde_yaml::Value::String(self.namespace.clone()),
        );

        configmap.insert(
            "metadata".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(metadata)),
        );

        // Data
        let mut data_map = BTreeMap::new();
        for (key, value) in data {
            data_map.insert(key, serde_yaml::Value::String(value));
        }
        configmap.insert(
            "data".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(data_map)),
        );

        Ok(GeneratedManifest {
            kind: ManifestKind::ConfigMap,
            name,
            namespace: self.namespace.clone(),
            content: serde_yaml::to_string(&configmap)
                .map_err(|e| anyhow::anyhow!("Failed to serialize configmap: {e}"))?,
        })
    }

    /// Generate a Secret manifest
    fn generate_secret(
        &self,
        secrets: BTreeMap<String, String>,
    ) -> Result<Option<GeneratedManifest>> {
        if secrets.is_empty() {
            return Ok(None);
        }

        let mut secret = BTreeMap::new();
        secret.insert(
            "apiVersion".to_string(),
            serde_yaml::Value::String("v1".to_string()),
        );
        secret.insert(
            "kind".to_string(),
            serde_yaml::Value::String("Secret".to_string()),
        );

        // Metadata
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "name".to_string(),
            serde_yaml::Value::String(format!("{}-secrets", self.environment)),
        );
        metadata.insert(
            "namespace".to_string(),
            serde_yaml::Value::String(self.namespace.clone()),
        );

        secret.insert(
            "metadata".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(metadata)),
        );

        secret.insert(
            "type".to_string(),
            serde_yaml::Value::String("Opaque".to_string()),
        );

        // Data (base64 encoded)
        let mut data = BTreeMap::new();
        for (key, value) in secrets {
            let encoded = STANDARD.encode(value.as_bytes());
            data.insert(key, serde_yaml::Value::String(encoded));
        }
        secret.insert(
            "data".to_string(),
            serde_yaml::Value::Mapping(Self::btree_to_mapping(data)),
        );

        let manifest = GeneratedManifest {
            kind: ManifestKind::Secret,
            name: format!("{}-secrets", self.environment),
            namespace: self.namespace.clone(),
            content: serde_yaml::to_string(&secret)
                .map_err(|e| anyhow::anyhow!("Failed to serialize secret: {e}"))?,
        };

        Ok(Some(manifest))
    }

    /// Get the fully qualified image name for a component
    fn get_image_name(&self, component: &ComponentBuildSpec) -> Result<String> {
        // Use component name as base image name
        let base_image = format!("{}:latest", component.component_name);

        // Build full image name with registry
        let image = match (&self.registry_url, &self.registry_namespace) {
            (Some(reg), Some(ns)) => format!("{reg}/{ns}/{base_image}"),
            (Some(reg), None) => format!("{reg}/{base_image}"),
            (None, Some(ns)) => format!("{ns}/{base_image}"),
            (None, None) => base_image,
        };

        Ok(image)
    }
}

/// A generated Kubernetes manifest
pub struct GeneratedManifest {
    /// The kind of Kubernetes resource
    pub kind: ManifestKind,
    /// The name of the resource
    pub name: String,
    /// The namespace for the resource
    pub namespace: String,
    /// The YAML content of the manifest
    pub content: String,
}

/// Types of Kubernetes manifests we generate
#[derive(Debug, Clone)]
pub enum ManifestKind {
    Deployment,
    Service,
    Ingress,
    ConfigMap,
    Secret,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rush_build::Variables;
    use rush_config::Config;
    use tempfile::TempDir;

    use super::*;

    fn create_test_component(name: &str, port: Option<u16>) -> ComponentBuildSpec {
        let config = Config::test_default();
        let variables = Variables::new(std::path::Path::new("."), "test");

        ComponentBuildSpec {
            build_type: BuildType::RustBinary {
                location: "src".to_string(),
                dockerfile_path: "Dockerfile".to_string(),
                context_dir: None,
                features: None,
                precompile_commands: None,
            },
            product_name: "test-product".to_string(),
            component_name: name.to_string(),
            color: "blue".to_string(),
            depends_on: vec![],
            build: None,
            mount_point: None,
            subdomain: None,
            artefacts: None,
            artefact_output_dir: "/tmp".to_string(),
            docker_extra_run_args: vec![],
            env: None,
            volumes: None,
            port,
            target_port: None,
            k8s: None,
            priority: 0,
            watch: None,
            config,
            variables,
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: HashMap::new(),
            dotenv_secrets: HashMap::new(),
            domain: "test.local".to_string(),
            cross_compile: "native".to_string(),
            health_check: None,
            startup_probe: None,
        }
    }

    #[tokio::test]
    async fn test_generate_deployment() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "test".to_string(),
        );

        let component = create_test_component("test-app", Some(8080));
        let manifests = generator
            .generate_manifests(&[component], None)
            .await
            .unwrap();

        // Should generate deployment and service
        assert_eq!(manifests.len(), 2);

        // Check deployment
        let deployment = manifests
            .iter()
            .find(|m| matches!(m.kind, ManifestKind::Deployment))
            .expect("Should have a deployment");
        assert_eq!(deployment.name, "test-app");
        assert_eq!(deployment.namespace, "test-namespace");
        assert!(deployment.content.contains("kind: Deployment"));
        assert!(deployment.content.contains("replicas: 1"));

        // Check service
        let service = manifests
            .iter()
            .find(|m| matches!(m.kind, ManifestKind::Service))
            .expect("Should have a service");
        assert_eq!(service.name, "test-app");
        assert!(service.content.contains("kind: Service"));
        assert!(service.content.contains("port: 8080"));
    }

    #[tokio::test]
    async fn test_generate_with_registry() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "test".to_string(),
        )
        .with_registry(Some("gcr.io".to_string()), Some("my-project".to_string()));

        let component = create_test_component("test-app", Some(8080));
        let manifests = generator
            .generate_manifests(&[component], None)
            .await
            .unwrap();

        let deployment = manifests
            .iter()
            .find(|m| matches!(m.kind, ManifestKind::Deployment))
            .unwrap();

        // Check that image has registry prefix
        assert!(deployment
            .content
            .contains("image: gcr.io/my-project/test-app:latest"));
    }

    #[tokio::test]
    async fn test_generate_secrets() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "test".to_string(),
        );

        let component = create_test_component("test-app", None);

        let mut secrets = BTreeMap::new();
        secrets.insert("API_KEY".to_string(), "secret123".to_string());
        secrets.insert("DB_PASSWORD".to_string(), "pass456".to_string());

        let manifests = generator
            .generate_manifests(&[component], Some(secrets))
            .await
            .unwrap();

        // Should have deployment and secret (no service since no port)
        assert_eq!(manifests.len(), 2);

        let secret = manifests
            .iter()
            .find(|m| matches!(m.kind, ManifestKind::Secret))
            .expect("Should have a secret");
        assert_eq!(secret.name, "test-secrets");
        assert!(secret.content.contains("kind: Secret"));
        assert!(secret.content.contains("type: Opaque"));
    }
}
