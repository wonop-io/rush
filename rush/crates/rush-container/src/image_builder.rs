use crate::docker::{DockerClient, DockerService, DockerServiceConfig};
use crate::tagging::ImageTagGenerator;
use log::warn;
use rush_build::ComponentBuildSpec;
use rush_build::{BuildContext, BuildType};
use rush_core::constants::*;
use rush_core::error::{Error, Result};
use rush_security::Vault;
use rush_toolchain::{Platform, ToolchainContext};
use rush_utils::PathMatcher;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Configuration for the build process
pub struct BuildConfig {
    /// Build type (e.g., TrunkWasm, RustBinary, etc.)
    pub build_type: BuildType,
    /// Path to Dockerfile
    pub dockerfile_path: Option<String>,
    /// Build context directory
    pub context_dir: Option<String>,
    /// Docker registry to use
    pub docker_registry: String,
    /// Environment (dev, staging, prod)
    pub environment: String,
    /// Domain for the service
    pub domain: String,
    /// Optional mount point
    pub mount_point: Option<String>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            build_type: BuildType::PureKubernetes,
            dockerfile_path: None,
            context_dir: None,
            docker_registry: DEFAULT_DOCKER_REGISTRY.to_string(),
            environment: ENV_DEV.to_string(),
            domain: "localhost".to_string(),
            mount_point: None,
        }
    }
}

/// Builds and manages Docker images, providing compatibility with the old DockerImage functionality
pub struct ImageBuilder {
    /// The DockerService this builder is associated with
    service: DockerService,
    /// Docker client for building images
    docker_client: Arc<dyn DockerClient>,
    /// Component name (from the old DockerImage)
    component_name: String,
    /// Product name
    product_name: String,
    /// Build configuration
    build_config: BuildConfig,
    /// Toolchain for building
    toolchain: Option<Arc<ToolchainContext>>,
    /// Tag generator for consistent tag computation
    tag_generator: Option<Arc<ImageTagGenerator>>,
    /// Vault for secrets
    vault: Option<Arc<Mutex<dyn Vault + Send>>>,
    /// Whether the image should be rebuilt
    should_rebuild: bool,
    /// Whether the image was recently rebuilt
    was_recently_rebuilt: bool,
    /// File watcher for context paths
    context_watcher: Option<PathMatcher>,
    /// Original component build spec
    spec: Option<Arc<Mutex<ComponentBuildSpec>>>,
    /// Git hash tag for the image (e.g., "abc12345" or "abc12345-wip-def67890")
    git_tag: Option<String>,
    /// Whether the image exists in the local Docker registry
    image_exists_in_cache: bool,
}

impl ImageBuilder {
    /// Creates a new ImageBuilder from a DockerService
    pub fn new(
        service: DockerService,
        docker_client: Arc<dyn DockerClient>,
        component_name: String,
        product_name: String,
    ) -> Self {
        Self {
            service,
            docker_client,
            component_name,
            product_name,
            build_config: BuildConfig::default(),
            toolchain: None,
            tag_generator: None,
            vault: None,
            should_rebuild: true,
            was_recently_rebuilt: false,
            context_watcher: None,
            spec: None,
            git_tag: None,
            image_exists_in_cache: false,
        }
    }

    /// Sets the toolchain for this image builder
    pub fn with_toolchain(mut self, toolchain: Arc<ToolchainContext>) -> Self {
        // Create tag generator when toolchain is set
        // We'll need to set the base directory later when the spec is available
        self.toolchain = Some(toolchain.clone());
        self
    }

    /// Gets the current toolchain
    pub fn toolchain(&self) -> Option<Arc<ToolchainContext>> {
        self.toolchain.clone()
    }

    /// Sets the vault for this image builder
    pub fn with_vault(mut self, vault: Arc<Mutex<dyn Vault + Send>>) -> Self {
        self.vault = Some(vault);
        self
    }

    /// Gets the current vault
    pub fn vault(&self) -> Option<Arc<Mutex<dyn Vault + Send>>> {
        self.vault.clone()
    }

    /// Sets the build configuration
    pub fn with_build_config(mut self, build_config: BuildConfig) -> Self {
        self.build_config = build_config;
        self
    }

    /// Gets the current build configuration
    pub fn build_config(&self) -> &BuildConfig {
        &self.build_config
    }

    /// Sets the file watcher for detecting changes
    pub fn with_watcher(mut self, watcher: PathMatcher) -> Self {
        self.context_watcher = Some(watcher);
        self
    }

    /// Gets the current file watcher
    pub fn context_watcher(&self) -> Option<&PathMatcher> {
        self.context_watcher.as_ref()
    }

    /// Gets the component name
    pub fn component_name(&self) -> &str {
        &self.component_name
    }

    /// Gets the product name
    pub fn product_name(&self) -> &str {
        &self.product_name
    }

    /// Gets the original ComponentBuildSpec
    pub fn spec(&self) -> Arc<Mutex<ComponentBuildSpec>> {
        self.spec.as_ref().expect("No spec available").clone()
    }

    /// Checks if this image should be rebuilt
    pub fn should_rebuild(&self) -> bool {
        self.should_rebuild
    }

    /// Sets whether this image should be rebuilt
    pub fn set_should_rebuild(&mut self, should_rebuild: bool) {
        self.should_rebuild = should_rebuild;
    }

    /// Checks if this image was recently rebuilt
    pub fn was_recently_rebuilt(&self) -> bool {
        self.was_recently_rebuilt
    }

    /// Sets whether this image was recently rebuilt
    pub fn set_was_recently_rebuilt(&mut self, was_recently_rebuilt: bool) {
        self.was_recently_rebuilt = was_recently_rebuilt;
    }

    /// Sets the git tag directly (useful when already computed elsewhere)
    pub fn set_git_tag(&mut self, tag: String) {
        self.git_tag = Some(tag);
    }

    /// Gets the untagged image name (without a tag)
    pub fn untagged_image_name(&self) -> String {
        rush_core::naming::NamingConvention::image_name(&self.product_name, &self.component_name)
    }

    /// Gets the tagged image name (with a tag if available)
    pub fn tagged_image_name(&self) -> String {
        // Use git-based tag if available
        if let Some(git_tag) = &self.git_tag {
            let base_name = self.untagged_image_name();
            // Include registry if specified
            if !self.build_config.docker_registry.is_empty() {
                return format!(
                    "{}/{}:{}",
                    self.build_config.docker_registry, base_name, git_tag
                );
            } else {
                return format!("{base_name}:{git_tag}");
            }
        } else if let Some(spec) = &self.spec {
            if let Ok(spec_guard) = spec.lock() {
                if let Some(tagged_name) = &spec_guard.tagged_image_name {
                    return tagged_name.clone();
                }
            }
        }

        // Fallback to service image name if tagged name not available
        self.service.config.image.clone()
    }

    /// Computes the git-based tag for this image
    /// Returns a tag like "abc12345" for clean commits or "abc12345-wip-def67890" for dirty state
    pub fn compute_git_tag(&mut self) -> Result<String> {
        log::debug!("Computing git tag for component: {}", self.component_name);

        // Check if we have a spec to use with the tag generator
        if let Some(ref spec_arc) = self.spec {
            if let Ok(spec) = spec_arc.lock() {
                // Create tag generator if not already created
                if self.tag_generator.is_none() {
                    let toolchain = self.toolchain.as_ref()
                        .ok_or_else(|| Error::Setup("No toolchain available for computing git tag".into()))?;

                    // Determine base directory from the spec's config if available
                    let base_dir = spec.config.product_path().to_path_buf();

                    self.tag_generator = Some(Arc::new(ImageTagGenerator::new(
                        toolchain.clone(),
                        base_dir,
                    )));
                }

                // Use the centralized tag generator
                if let Some(ref tag_gen) = self.tag_generator {
                    let tag = tag_gen.compute_tag(&spec)?;
                    self.git_tag = Some(tag.clone());
                    return Ok(tag);
                }
            }
        }

        // Fallback: If no spec available, use the old implementation for compatibility
        // This should rarely happen in practice
        warn!("No spec available for tag generation, using fallback timestamp");
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
        self.git_tag = Some(timestamp.clone());
        Ok(timestamp)
    }

    /// Checks if the image exists in the local Docker cache with the correct platform
    pub async fn check_image_exists(&mut self) -> Result<bool> {
        use tokio::process::Command;

        // Ensure we have a git tag
        self.compute_git_tag()?;

        let tagged_name = self.tagged_image_name();

        log::debug!("Checking if image exists: {}", tagged_name);

        // First check if the image exists at all
        let inspect_output = Command::new("docker")
            .args([
                "image",
                "inspect",
                &tagged_name,
                "--format",
                "{{.Architecture}}",
            ])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Failed to check image existence: {e}")))?;

        if !inspect_output.status.success() {
            // Image doesn't exist
            self.image_exists_in_cache = false;
            log::debug!("Image {} not found in cache", tagged_name);
            return Ok(false);
        }

        // Check if the architecture matches what we need
        let arch = String::from_utf8_lossy(&inspect_output.stdout)
            .trim()
            .to_string();

        // We always target linux/amd64, which should show as "amd64" architecture
        let expected_arch = "amd64";

        if arch != expected_arch {
            log::warn!(
                "Image {} exists but has wrong architecture: {} (expected {})",
                tagged_name,
                arch,
                expected_arch
            );
            self.image_exists_in_cache = false;
            return Ok(false);
        }

        self.image_exists_in_cache = true;
        log::info!(
            "Image {} already exists in cache with correct architecture",
            tagged_name
        );

        Ok(self.image_exists_in_cache)
    }

    /// Determines if the image should be rebuilt based on cache and file changes
    pub async fn evaluate_rebuild_needed(&mut self) -> Result<bool> {
        // First check if image exists
        self.should_rebuild = !self.check_image_exists().await?;
        Ok(self.should_rebuild)
    }

    /// Generates a build context with secrets from the vault
    #[allow(clippy::await_holding_lock)]
    pub async fn generate_build_context(&self) -> Result<BuildContext> {
        if let Some(vault) = &self.vault {
            let vault_guard = vault
                .lock()
                .map_err(|_| Error::Vault("Failed to lock vault".into()))?;
            let secrets = match vault_guard
                .get(
                    &self.product_name,
                    &self.component_name,
                    &self.build_config.environment,
                )
                .await
            {
                Ok(secrets) => secrets,
                Err(e) => {
                    warn!("Failed to get secrets: {}", e);
                    HashMap::new()
                }
            };

            // Create build context with the proper fields - modify this as needed
            // to match your BuildContext structure
            let context = BuildContext {
                build_type: self.build_config.build_type.clone(),
                // Add fields that match your BuildContext struct
                location: None,
                target: Platform::new("linux", "x86_64"), // Use explicit Platform creation instead of Default
                host: Platform::new("linux", "x86_64"), // Use explicit Platform creation instead of Default
                rust_target: String::new(),
                toolchain: (**self.toolchain.as_ref().unwrap()).clone(), // Dereference the Arc<ToolchainContext>
                services: Default::default(),
                environment: self.build_config.environment.clone(),
                domain: self.build_config.domain.clone(),
                product_name: self.product_name.clone(),
                product_uri: String::new(), // Fill in appropriately
                component: self.component_name.clone(),
                docker_registry: self.build_config.docker_registry.clone(),
                image_name: String::new(), // Fill in appropriately
                domains: HashMap::new(),
                env: HashMap::new(),
                secrets,
                cross_compile: if let Some(spec_arc) = &self.spec {
                    spec_arc.lock().unwrap().cross_compile.clone()
                } else {
                    "native".to_string()
                },
            };

            Ok(context)
        } else {
            Err(Error::Setup("Vault not configured".into()))
        }
    }

    /// Checks if any files in the build context have changed
    pub fn is_any_file_in_context(&self, file_paths: &[PathBuf]) -> bool {
        if let Some(watcher) = &self.context_watcher {
            for file in file_paths {
                if watcher.matches(file) {
                    return true;
                }
            }
        }
        false
    }

    /// Builds the Docker image
    pub async fn build(&mut self) -> Result<()> {
        use log::info;
        use rush_utils::DockerCrossCompileGuard;

        // Ensure we have a git tag computed
        self.compute_git_tag()?;

        // Validate that we're not using 'latest' tag (unless intentional)
        if let Some(tag) = &self.git_tag {
            if tag == DOCKER_TAG_LATEST {
                warn!(
                    "Building image {} with '{}' tag - caching will not work properly! \
                     Component: {}, Context: {:?}",
                    self.untagged_image_name(),
                    DOCKER_TAG_LATEST,
                    self.component_name,
                    self.build_config.context_dir
                );
            }
        }

        // Use the tagged image name with git hash
        let image_tag = self.tagged_image_name();
        info!("Building Docker image: {}", image_tag);

        // Set up cross-compilation environment if needed
        let (cross_compile, target) = if let Some(spec_arc) = &self.spec {
            let spec = spec_arc.lock().unwrap();
            let cross_compile = spec.cross_compile.clone();
            let target = if let Some(toolchain) = &self.toolchain {
                toolchain.target().to_docker_target()
            } else {
                "linux/amd64".to_string() // Default target
            };
            (cross_compile, target)
        } else {
            // Default values if spec is not available
            ("native".to_string(), "linux/amd64".to_string())
        };

        // Create cross-compilation guard for native compilation only
        // cross-rs handles its own Docker environment
        let _cross_guard = if cross_compile == "native" {
            Some(DockerCrossCompileGuard::new(&target))
        } else {
            None
        };

        // Get the dockerfile and context paths
        let dockerfile_path = self
            .build_config
            .dockerfile_path
            .as_ref()
            .ok_or_else(|| Error::Build("No Dockerfile path specified".into()))?;

        let default_context = ".".to_string();
        let context_path = self
            .build_config
            .context_dir
            .as_ref()
            .unwrap_or(&default_context);

        info!("Build config context dir: {}", context_path);
        // Use the docker client to build the image
        self.docker_client
            .build_image(&image_tag, dockerfile_path, context_path)
            .await?;

        info!("Successfully built Docker image: {}", image_tag);
        Ok(())
    }

    /// Pushes the Docker image to the registry
    pub async fn push(&self) -> Result<()> {
        // Implementation of push logic using service and toolchain
        Ok(())
    }

    /// Gets the underlying DockerService
    pub fn service(&self) -> &DockerService {
        &self.service
    }

    /// Creates an ImageBuilder from a ComponentBuildSpec
    pub fn from_build_spec(
        spec: Arc<Mutex<ComponentBuildSpec>>,
        docker_client: Arc<dyn DockerClient>,
    ) -> Result<Self> {
        let spec_guard = spec
            .lock()
            .map_err(|_| Error::Internal("Failed to lock spec".into()))?;

        // Extract configuration from spec
        log::info!(
            "DEBUG -- Getting context dir: {}",
            Self::context_dir_from_spec(&spec_guard).unwrap()
        );
        let build_config = BuildConfig {
            build_type: spec_guard.build_type.clone(),
            dockerfile_path: Self::docker_path_from_spec(&spec_guard),
            context_dir: Self::context_dir_from_spec(&spec_guard),
            docker_registry: spec_guard.config.docker_registry().to_string(),
            environment: spec_guard.config.environment().to_string(),
            domain: spec_guard.domain.clone(),
            mount_point: spec_guard.mount_point.clone(),
        };

        // Create DockerServiceConfig from spec

        let service_config = DockerServiceConfig {
            name: spec_guard.docker_local_name(),
            image: rush_core::naming::NamingConvention::image_name(&spec_guard.product_name, &spec_guard.component_name),
            network: spec_guard.config.network_name().to_string(),
            env_vars: {
                // Merge dotenv and dotenv_secrets for image building
                let mut env = spec_guard.dotenv.clone();
                env.extend(spec_guard.dotenv_secrets.clone());
                env
            },
            ports: if let Some(port) = spec_guard.port {
                if let Some(target_port) = spec_guard.target_port {
                    vec![format!("{}:{}", port, target_port)]
                } else {
                    vec![]
                }
            } else {
                vec![]
            },
            volumes: if let Some(volumes) = spec_guard.volumes.clone() {
                volumes
                    .into_iter()
                    .map(|(host, container)| format!("{host}:{container}"))
                    .collect()
            } else {
                vec![]
            },
        };

        // Create the DockerService
        let service = DockerService::new(
            "".to_string(), // ID will be set when launching
            service_config,
            docker_client.clone(),
        );

        // Create ImageBuilder
        let mut builder = Self::new(
            service,
            docker_client,
            spec_guard.component_name.clone(),
            spec_guard.product_name.clone(),
        )
        .with_build_config(build_config);

        // Store the original spec
        builder.spec = Some(spec.clone());

        // Add path matcher if available
        if let Some(watch) = &spec_guard.watch {
            builder.context_watcher = Some((**watch).clone());
        }

        Ok(builder)
    }

    /// Extracts Dockerfile path from ComponentBuildSpec
    fn docker_path_from_spec(spec: &ComponentBuildSpec) -> Option<String> {
        match &spec.build_type {
            BuildType::TrunkWasm {
                dockerfile_path, ..
            }
            | BuildType::DixiousWasm {
                dockerfile_path, ..
            }
            | BuildType::RustBinary {
                dockerfile_path, ..
            }
            | BuildType::Book {
                dockerfile_path, ..
            }
            | BuildType::Zola {
                dockerfile_path, ..
            }
            | BuildType::Script {
                dockerfile_path, ..
            }
            | BuildType::Ingress {
                dockerfile_path, ..
            } => Some(dockerfile_path.clone()),
            _ => None,
        }
    }

    /// Extracts context directory from ComponentBuildSpec
    fn context_dir_from_spec(spec: &ComponentBuildSpec) -> Option<String> {
        match &spec.build_type {
            BuildType::TrunkWasm { context_dir, .. }
            | BuildType::DixiousWasm { context_dir, .. }
            | BuildType::RustBinary { context_dir, .. }
            | BuildType::Book { context_dir, .. }
            | BuildType::Zola { context_dir, .. }
            | BuildType::Script { context_dir, .. }
            | BuildType::Ingress { context_dir, .. } => context_dir.clone(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::DockerService;
    use crate::image_builder::ImageBuilder;
    use std::sync::Arc;

    #[test]
    fn test_image_builder_creation() {
        let mock_client = Arc::new(crate::docker::DockerCliClient::new("docker".to_string()));
        let config = DockerServiceConfig {
            name: "test".to_string(),
            image: "test:latest".to_string(),
            network: "test-net".to_string(),
            env_vars: HashMap::new(),
            ports: vec![],
            volumes: vec![],
        };

        let service = DockerService::new("test-id".to_string(), config, mock_client.clone());
        let builder = ImageBuilder::new(
            service,
            mock_client,
            "test-component".to_string(),
            "test-product".to_string(),
        );

        assert_eq!(builder.component_name(), "test-component");
        assert!(builder.should_rebuild());
        assert!(!builder.was_recently_rebuilt());
    }
}
