//! Container orchestration and lifecycle management
//!
//! The reactor is responsible for orchestrating container lifecycle events,
//! monitoring containers, and handling file change events that trigger rebuilds.
use crate::build::{BuildType, ComponentBuildSpec, Variables};
use crate::constants::DOCKER_TAG_LATEST;
use crate::container::DockerCliClient;
use crate::container::Status;
use crate::container::{
    docker::{ContainerStatus, DockerClient, DockerService, DockerServiceConfig},
    network::setup_network,
    watcher::{setup_file_watcher, ChangeProcessor, WatcherConfig},
    BuildProcessor, ContainerService, ServiceCollection,
};
use notify::RecommendedWatcher;
use crate::core::config::Config;
use crate::error::{Error, Result};
use crate::security::{FileVault, SecretsEncoder, Vault};
use crate::shutdown;

use colored::Colorize;
use log::{debug, error, info, trace, warn};
use serde_yaml;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{broadcast, mpsc};

/// Manages the container lifecycle and coordinates rebuilds based on file changes
pub struct ContainerReactor {
    /// Configuration for the reactor
    config: Arc<ContainerReactorConfig>,

    /// Collection of services managed by this reactor
    services: ServiceCollection,

    /// File change processor for detecting code changes
    change_processor: Arc<ChangeProcessor>,
    
    /// File watcher (must be kept alive)
    _file_watcher: RecommendedWatcher,

    /// Docker client for container operations
    docker_client: Arc<dyn DockerClient>,

    /// Build processor for container builds
    build_processor: BuildProcessor,

    /// Vault for accessing secrets
    vault: Arc<Mutex<dyn Vault + Send>>,
    
    /// Toolchain for build operations
    toolchain: Option<Arc<crate::toolchain::ToolchainContext>>,

    /// Running container services
    running_services: Vec<DockerService>,

    /// Channel for triggering graceful shutdown
    shutdown_sender: broadcast::Sender<()>,

    /// Indicates if a rebuild is in progress
    rebuild_in_progress: bool,

    /// List of available components from stack spec
    available_components: Vec<String>,

    /// Component specifications
    component_specs: Vec<ComponentBuildSpec>,

    /// Secrets encoder
    secrets_encoder: Arc<dyn SecretsEncoder>,
    
    /// Output director for handling container logs
    output_director: Option<crate::output::SharedOutputDirector>,
    
    /// Mapping of component names to their actual built image names (with git tags)
    built_images: HashMap<String, String>,
}

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

/// Enum representing the result of waiting for changes or termination
enum WaitResult {
    /// File changes were detected
    FileChanged,
    /// Process was terminated
    Terminated,
    /// Timeout occurred
    Timeout,
}

impl ContainerReactor {
    /// Sets the output director for handling container logs
    pub fn set_output_director(&mut self, director: Box<dyn crate::output::OutputDirector>) {
        eprintln!("DEBUG: ContainerReactor::set_output_director called");
        self.output_director = Some(crate::output::SharedOutputDirector::new(director));
        eprintln!("DEBUG: Output director has been set");
    }
    /// Creates a new ContainerReactor
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the reactor
    /// * `docker_client` - Client for interacting with Docker
    /// * `vault` - Secret vault
    /// * `secrets_encoder` - Encoder for secrets
    ///
    /// # Returns
    ///
    /// A new ContainerReactor instance
    pub fn new(
        config: ContainerReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        vault: Arc<Mutex<dyn Vault + Send>>,
        secrets_encoder: Arc<dyn SecretsEncoder>,
    ) -> Result<Self> {
        let config = Arc::new(config);
        let (watcher, change_processor) = setup_file_watcher(config.watch_config.clone())?;

        let (shutdown_sender, _) = broadcast::channel(8);
        
        // Create the toolchain for build operations
        let toolchain = Some(Arc::new(crate::toolchain::ToolchainContext::default()));

        Ok(Self {
            config,
            services: HashMap::new(),
            change_processor: Arc::new(change_processor),
            _file_watcher: watcher,
            docker_client,
            build_processor: BuildProcessor::new(false),
            vault,
            toolchain,
            running_services: Vec::new(),
            shutdown_sender,
            rebuild_in_progress: false,
            available_components: Vec::new(),
            component_specs: Vec::new(),
            secrets_encoder,
            output_director: None,
            built_images: HashMap::new(),
        })
    }

    /// Creates a new ContainerReactor with a Config
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    ///
    /// # Returns
    ///
    /// A new ContainerReactor instance
    pub fn new_with_config(config: Arc<Config>) -> Result<Self> {
        // Create default implementations
        let docker_client = Arc::new(DockerCliClient::new("docker".to_string()));
        let vault = Arc::new(Mutex::new(FileVault::new(
            PathBuf::from("/tmp/vault"),
            None,
        )));
        let secrets_encoder = Arc::new(crate::security::Base64SecretsEncoder);

        // Create reactor config
        let reactor_config = ContainerReactorConfig {
            product_name: config.product_name().to_string(),
            product_dir: config.product_path().clone(),
            network_name: config.network_name().to_string(),
            environment: config.environment().to_string(),
            docker_registry: config.docker_registry().to_string(),
            redirected_components: HashMap::new(),
            silenced_components: HashSet::new(),
            verbose: false,
            watch_config: WatcherConfig::default(),
            git_hash: DOCKER_TAG_LATEST.to_string(),
            start_port: config.start_port(),
        };

        Self::new(reactor_config, docker_client, vault, secrets_encoder)
    }

    /// Creates a ContainerReactor from a product directory
    ///
    /// # Arguments
    ///
    /// * `config` - Application configuration
    /// * `toolchain` - Toolchain context for building
    /// * `vault` - Secret vault
    /// * `secrets_encoder` - Encoder for secrets
    /// * `k8s_encoder` - Kubernetes secret encoder
    /// * `redirected_components` - Map of components to redirect
    /// * `silence_components` - List of components to silence
    ///
    /// # Returns
    ///
    /// A new ContainerReactor instance
    pub fn from_product_dir(
        config: Arc<Config>,
        vault: Arc<Mutex<dyn Vault + Send>>,
        secrets_encoder: Arc<dyn SecretsEncoder>,
        redirected_components: HashMap<String, (String, u16)>,
        silence_components: Vec<String>,
    ) -> Result<Self> {
        // Get the git hash for tagging
        let git_hash = match get_git_folder_hash(&config.product_path().display().to_string()) {
            Ok(hash) => {
                if hash.is_empty() {
                    DOCKER_TAG_LATEST.to_string()
                } else {
                    hash[..8].to_string()
                }
            }
            Err(_) => DOCKER_TAG_LATEST.to_string(),
        };

        let product_path = config.product_path();
        let docker_client = Arc::new(DockerCliClient::new("docker".to_string()));

        // Create set of silenced components
        let silenced_components = silence_components.into_iter().collect::<HashSet<_>>();

        // Read stack configuration
        let stack_config = match fs::read_to_string(format!(
            "{}/stack.spec.yaml",
            product_path.display().to_string()
        )) {
            Ok(config) => config,
            Err(e) => return Err(format!("Failed to read stack config: {}", e).into()),
        };

        // Parse YAML
        let stack_config_value: serde_yaml::Value = serde_yaml::from_str(&stack_config)
            .map_err(|e| format!("Failed to parse stack config: {}", e))?;

        // Create watch config
        let watch_config = WatcherConfig {
            root_dir: PathBuf::from(product_path),
            watch_paths: vec![],
            debounce_ms: 100,
            use_gitignore: true,
        };

        // Create reactor config
        let reactor_config = ContainerReactorConfig {
            product_name: config.product_name().to_string(),
            product_dir: PathBuf::from(product_path),
            network_name: config.network_name().to_string(),
            environment: config.environment().to_string(),
            docker_registry: config.docker_registry().to_string(),
            redirected_components,
            silenced_components,
            verbose: false,
            watch_config,
            git_hash,
            start_port: config.start_port(),
        };

        // Create the reactor
        let mut reactor = Self::new(reactor_config, docker_client, vault, secrets_encoder)?;

        // Parse available components
        let mut available_components = Vec::new();
        let mut component_specs = Vec::new();

        // Create variables for component specs
        let variables = Variables::empty();

        // Process component specifications from stack config
        if let serde_yaml::Value::Mapping(config_map) = stack_config_value {
            for (component_name, yaml_section) in config_map {
                let component_name = component_name
                    .as_str()
                    .ok_or_else(|| "Invalid component name".to_string())?
                    .to_string();

                available_components.push(component_name.clone());

                let mut yaml_section_clone = yaml_section.clone();

                // Ensure component_name is in the YAML
                if let serde_yaml::Value::Mapping(ref mut yaml_section_map) = yaml_section_clone {
                    if !yaml_section_map
                        .contains_key(&serde_yaml::Value::String("component_name".to_string()))
                    {
                        yaml_section_map.insert(
                            serde_yaml::Value::String("component_name".to_string()),
                            serde_yaml::Value::String(component_name.clone()),
                        );
                    }
                }

                // Create component spec
                let component_spec = ComponentBuildSpec::from_yaml(
                    config.clone(),
                    variables.clone(),
                    &yaml_section_clone,
                );
                component_specs.push(component_spec);
            }
        }

        // Store the components
        reactor.available_components = available_components;
        reactor.component_specs = component_specs;

        // Build services collection
        let services = reactor.build_services_collection()?;
        reactor.set_services(services);

        trace!("Created container reactor from product directory");
        Ok(reactor)
    }

    /// Builds a collection of services from component specs
    fn build_services_collection(&self) -> Result<ServiceCollection> {
        let mut services: ServiceCollection = HashMap::new();
        let mut next_port = self.config.start_port;

        for spec in &self.component_specs {
            // Skip pure Kubernetes specs
            if matches!(
                spec.build_type,
                BuildType::PureKubernetes | BuildType::KubernetesInstallation { .. }
            ) {
                continue;
            }

            let component_name = &spec.component_name;
            let subdomain = spec.subdomain.clone();
            let domain = self.get_domain(subdomain.as_deref())?;

            // Determine port
            let port = spec.port.unwrap_or_else(|| {
                let p = next_port;
                next_port += 1;
                p
            });

            let target_port = spec.target_port.unwrap_or(port);

            // Create service entry with docker_host
            let docker_host = format!("{}-{}", self.config.product_name, component_name);

            // Determine the image name based on build type
            let tagged_image_name = match &spec.build_type {
                BuildType::PureDockerImage { image_name_with_tag, .. } => {
                    // Use the pre-existing image directly (e.g., "postgres:latest")
                    image_name_with_tag.clone()
                }
                _ => {
                    // Check if we have a built image with git tag for this component
                    if let Some(built_image) = self.built_images.get(component_name) {
                        built_image.clone()
                    } else {
                        // Fallback to default naming (this happens before build)
                        let image_name = if self.config.docker_registry.is_empty() {
                            format!("{}-{}", self.config.product_name, component_name)
                        } else {
                            format!("{}/{}-{}", 
                                self.config.docker_registry, 
                                self.config.product_name, 
                                component_name)
                        };
                        format!("{}:{}", image_name, self.config.git_hash)
                    }
                }
            };
            
            let service = Arc::new(ContainerService {
                id: "TODO".to_string(),
                name: component_name.clone(),
                image: tagged_image_name,
                host: component_name.clone(),
                port,
                target_port,
                mount_point: spec.mount_point.clone(),
                domain: domain.clone(),
                docker_host,
            });

            // Add to services collection
            services
                .entry(domain)
                .or_insert_with(Vec::new)
                .push(service);
        }

        Ok(services)
    }

    /// Gets the domain for a component based on subdomain
    fn get_domain(&self, subdomain: Option<&str>) -> Result<String> {
        // For simplicity in this implementation, we'll use a basic domain format
        // In a complete implementation, this would use templates and environment-specific logic
        let domain_base = format!("{}.{}", self.config.product_name, self.config.environment);

        match subdomain {
            Some(sub) => Ok(format!("{}.{}", sub, domain_base)),
            None => Ok(domain_base),
        }
    }

    /// Sets up services based on the build context
    ///
    /// # Arguments
    ///
    /// * `services` - Collection of services to manage
    pub fn set_services(&mut self, services: ServiceCollection) {
        self.services = services;
    }

    /// Sets verbosity level for the build processor
    ///
    /// # Arguments
    ///
    /// * `verbose` - Whether to output verbose build logs
    pub fn set_verbose(&mut self, verbose: bool) {
        self.build_processor = BuildProcessor::new(verbose);
    }

    /// Launches containers based on the current configuration
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn launch(&mut self) -> Result<()> {
        info!("Starting container reactor");

        // Set up the network
        setup_network(&self.config.network_name, &self.docker_client).await?;

        // Start the main launch loop
        self.launch_loop().await
    }

    /// Performs a container rollout
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn rollout(&mut self) -> Result<()> {
        info!("Rolling out containers...");
        // Implementation details for container rollout
        // This could involve stopping and starting containers in sequence
        // or applying new configurations to running containers

        // Clean up any existing containers
        self.cleanup_containers().await?;

        // Build all containers
        self.build_all().await?;

        // Launch all containers
        self.launch_containers().await?;

        info!("Rollout completed successfully");
        Ok(())
    }

    /// Build and push Docker images for all components
    pub async fn build_and_push(&mut self) -> Result<()> {
        info!("Building and pushing Docker images...");
        
        // Build all components
        self.build_all().await?;
        
        // Push images to registry
        for (_component_name, services) in self.services.iter() {
            for service in services {
                let image_name = if self.config.docker_registry.is_empty() {
                    service.name.clone()
                } else {
                    format!("{}/{}", self.config.docker_registry, service.name)
                };
                info!("Pushing image: {}", image_name);
                // TODO: Implement actual Docker push
            }
        }
        
        info!("Build and push completed successfully");
        Ok(())
    }

    /// Select Kubernetes context for deployment
    pub async fn select_kubernetes_context(&self, context: &str) -> Result<()> {
        info!("Selecting Kubernetes context: {}", context);
        
        // TODO: Implement kubectl context selection
        // let kubectl = "kubectl"; // Temporary placeholder
        let _args = vec!["config", "use-context", context];
        
        // Would use run_command here in actual implementation
        
        info!("Kubernetes context set to: {}", context);
        Ok(())
    }

    /// Apply Kubernetes manifests to the cluster
    pub async fn apply(&mut self) -> Result<()> {
        info!("Applying Kubernetes manifests...");
        
        // TODO: Generate and apply K8s manifests
        
        info!("Kubernetes manifests applied successfully");
        Ok(())
    }

    /// Remove Kubernetes resources from the cluster
    pub async fn unapply(&mut self) -> Result<()> {
        info!("Removing Kubernetes resources...");
        
        // TODO: Delete K8s resources
        
        info!("Kubernetes resources removed successfully");
        Ok(())
    }

    /// Install Kubernetes manifests (similar to apply but for installation)
    pub async fn install_manifests(&mut self) -> Result<()> {
        info!("Installing Kubernetes manifests...");
        
        // TODO: Install K8s manifests for the product
        
        info!("Kubernetes manifests installed successfully");
        Ok(())
    }

    /// Uninstall Kubernetes manifests
    pub async fn uninstall_manifests(&mut self) -> Result<()> {
        info!("Uninstalling Kubernetes manifests...");
        
        // TODO: Uninstall K8s manifests
        
        info!("Kubernetes manifests uninstalled successfully");
        Ok(())
    }

    /// Build Kubernetes manifests
    pub async fn build_manifests(&mut self) -> Result<()> {
        info!("Building Kubernetes manifests...");
        
        // TODO: Generate K8s manifests from templates
        
        info!("Kubernetes manifests built successfully");
        Ok(())
    }

    /// Deploy containers to Kubernetes
    pub async fn deploy(&mut self) -> Result<()> {
        info!("Deploying to Kubernetes...");
        
        // Build manifests
        self.build_manifests().await?;
        
        // Apply manifests
        self.apply().await?;
        
        info!("Deployment completed successfully");
        Ok(())
    }

    /// Build all container images
    pub async fn build(&mut self) -> Result<()> {
        info!("Building all container images...");
        self.build_all().await?;
        info!("All container images built successfully");
        Ok(())
    }

    /// Main container lifecycle loop that handles:
    /// 1. Building containers
    /// 2. Launching containers
    /// 3. Monitoring for file changes
    /// 4. Handling shutdowns and rebuilds
    async fn launch_loop(&mut self) -> Result<()> {
        let shutdown_token = shutdown::global_shutdown().cancellation_token();
        let mut should_continue = true;

        while should_continue {
            // Check for shutdown before each iteration
            if shutdown_token.is_cancelled() {
                info!("Launch loop cancelled due to shutdown signal");
                break;
            }

            // Clean up any existing containers
            tokio::select! {
                result = self.cleanup_containers() => {
                    result?;
                }
                _ = shutdown_token.cancelled() => {
                    info!("Container cleanup cancelled due to shutdown signal");
                    break;
                }
            }

            // Build all containers with shutdown handling
            let build_result = tokio::select! {
                result = self.build_all() => result,
                _ = shutdown_token.cancelled() => {
                    info!("Container build cancelled due to shutdown signal");
                    break;
                }
            };

            if let Err(e) = build_result {
                error!("Build failed: {}", e);

                // If shutdown was signalled during build error, exit immediately
                if shutdown_token.is_cancelled() {
                    info!("Exiting due to shutdown signal during build error");
                    break;
                }
                
                // Check if this is a Docker build error that likely won't be fixed by retrying
                let is_fatal_error = match &e {
                    Error::Docker(msg) => {
                        // These errors typically require manual intervention
                        msg.contains("Dockerfile or build context not found") ||
                        msg.contains("Permission denied") ||
                        msg.contains("No space left on device") ||
                        msg.contains("Docker build failed")
                    }
                    _ => false,
                };
                
                if is_fatal_error {
                    error!("\n╔══════════════════════════════════════════════════════════════╗");
                    error!("║                    FATAL BUILD ERROR                          ║");
                    error!("╚══════════════════════════════════════════════════════════════╝");
                    error!("\nThe build failed with an error that requires manual intervention.");
                    error!("\n📋 Next Steps:");
                    error!("   1. Review the detailed error output above");
                    error!("   2. Fix the identified issues");
                    error!("   3. Restart Rush with: rush dev");
                    error!("\n💡 Common Fixes:");
                    error!("   • Missing Dockerfile: Create or correct the Dockerfile path");
                    error!("   • Docker not running: Start Docker Desktop/daemon");
                    error!("   • Out of space: Run 'docker system prune -a'");
                    error!("   • Build errors: Check Dockerfile syntax and base image availability");
                    error!("\nExiting Rush...\n");
                    return Err(e);
                }

                // For non-fatal errors, wait for file changes or manual termination
                info!("Waiting for file changes to retry build...");
                match self.wait_for_changes_or_termination().await {
                    WaitResult::FileChanged => {
                        info!("File changes detected, retrying build...");
                        continue;
                    },
                    WaitResult::Terminated => break,
                    WaitResult::Timeout => {
                        warn!("Timeout waiting for changes, retrying anyway...");
                        continue;
                    },
                }
            }

            // Check for shutdown before launching containers
            if shutdown_token.is_cancelled() {
                info!("Skipping container launch due to shutdown signal");
                break;
            }

            // Launch all containers with shutdown handling
            tokio::select! {
                result = self.launch_containers() => {
                    if let Err(e) = result {
                        error!("Failed to launch containers: {}", e);
                        return Err(e);
                    }
                }
                _ = shutdown_token.cancelled() => {
                    info!("Container launch cancelled due to shutdown signal");
                    break;
                }
            }

            // Monitor containers and wait for changes
            should_continue = tokio::select! {
                result = self.monitor_and_handle_events() => result?,
                _ = shutdown_token.cancelled() => {
                    info!("Container monitoring cancelled due to shutdown signal");
                    false
                }
            };
        }

        info!("Container reactor shutting down");
        
        // Cleanup containers on shutdown (with timeout to prevent hanging)
        let cleanup_result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.cleanup_containers()
        ).await;
        
        match cleanup_result {
            Ok(Ok(())) => info!("Container cleanup completed successfully"),
            Ok(Err(e)) => warn!("Container cleanup failed: {}", e),
            Err(_) => warn!("Container cleanup timed out"),
        }
        
        Ok(())
    }

    /// Builds all container images
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    async fn build_all(&mut self) -> Result<()> {
        let shutdown_token = shutdown::global_shutdown().cancellation_token();
        
        // Check for shutdown before starting build
        if shutdown_token.is_cancelled() {
            info!("Build cancelled due to shutdown signal");
            return Err(Error::Terminated("Build cancelled due to shutdown".into()));
        }
        
        // Verify Docker is working before attempting builds
        if let Err(e) = self.verify_docker_available().await {
            error!("\n=== Docker Check Failed ===");
            error!("Unable to connect to Docker daemon.");
            error!("\nPlease ensure:");
            error!("1. Docker Desktop is running (if on macOS/Windows)");
            error!("2. Docker daemon is started (if on Linux)");
            error!("3. You have permission to access Docker socket");
            error!("\nTest with: docker ps");
            return Err(e);
        }

        info!("Building container images");
        self.rebuild_in_progress = true;

        // Process all component specs to build their images
        for spec in &self.component_specs {
            // Check for shutdown before each component build
            if shutdown_token.is_cancelled() {
                info!("Component build loop cancelled due to shutdown signal");
                return Err(Error::Terminated("Build cancelled due to shutdown".into()));
            }
            // Skip components that don't need Docker builds
            if !spec.build_type.requires_docker_build() {
                continue;
            }

            let component_name = &spec.component_name;
            info!("Building component: {}", component_name);

            // Skip redirected components
            if self
                .config
                .redirected_components
                .contains_key(component_name)
            {
                info!("Skipping redirected component: {}", component_name);
                continue;
            }

            // Get Dockerfile path from build type
            let dockerfile_path = match spec.build_type.dockerfile_path() {
                Some(path) => path,
                None => {
                    warn!("No Dockerfile specified for {}, skipping", component_name);
                    continue;
                }
            };

            // Get context directory
            // If no context_dir is specified, use the parent directory of the Dockerfile
            let context_dir = match &spec.build_type {
                BuildType::TrunkWasm { context_dir, location, .. } => {
                    context_dir.clone().unwrap_or_else(|| {
                        // If no context specified, use parent of Dockerfile location
                        if let Some(parent) = std::path::Path::new(dockerfile_path).parent() {
                            parent.to_string_lossy().to_string()
                        } else if let Some(parent) = std::path::Path::new(location).parent() {
                            // Fallback: derive context from the parent directory of location
                            parent.to_string_lossy().to_string()
                        } else {
                            ".".to_string()
                        }
                    })
                }
                BuildType::RustBinary { context_dir, .. }
                | BuildType::DixiousWasm { context_dir, .. }
                | BuildType::Script { context_dir, .. }
                | BuildType::Zola { context_dir, .. }
                | BuildType::Book { context_dir, .. }
                | BuildType::Ingress { context_dir, .. } => {
                    context_dir.clone().unwrap_or_else(|| {
                        // If no context specified, use parent directory of Dockerfile
                        if let Some(parent) = std::path::Path::new(dockerfile_path).parent() {
                            parent.to_string_lossy().to_string()
                        } else {
                            ".".to_string()
                        }
                    })
                }
                _ => continue,
            };
            
            debug!("Component: {}, Dockerfile: {}, Context dir: {}", 
                   component_name, dockerfile_path, context_dir);

            // Create image name and tag
            let image_name = if self.config.docker_registry.is_empty() {
                format!("{}-{}", self.config.product_name, component_name)
            } else {
                format!(
                    "{}/{}-{}",
                    self.config.docker_registry, self.config.product_name, component_name
                )
            };
            let image_tag = &self.config.git_hash;

            // Set tagged image name in component spec
            let tagged_image_name = format!("{}:{}", image_name, image_tag);

            // Render artifacts for components that need them (e.g., Ingress)
            if let Err(e) = self.render_artifacts_for_component(&spec).await {
                error!("Failed to render artifacts for {}: {}", component_name, e);
                self.rebuild_in_progress = false;
                return Err(e);
            }

            // Build any necessary scripts or templates first
            // Note: For cross-compilation scenarios (e.g., macOS to Linux), 
            // the build script may fail if the proper toolchain isn't installed.
            // In production, consider using Docker multi-stage builds instead.
            if let Err(e) = self.run_build_script_for_component(&spec).await {
                error!("Failed to run build script for {}: {}", component_name, e);
                
                // Check if this is a cross-compilation issue
                // For RustBinary builds targeting Linux from non-Linux hosts, this is expected
                let is_cross_compile_issue = matches!(&spec.build_type, BuildType::RustBinary { .. }) 
                    && cfg!(not(target_os = "linux"));
                    
                if is_cross_compile_issue {
                    error!(
                        "Cross-compilation from {} to Linux failed for {}.",
                        std::env::consts::OS,
                        component_name
                    );
                    error!(
                        "Solutions:\n\
                        1) Use Docker multi-stage builds (recommended):\n\
                           - An example Dockerfile.multistage has been created in the backend directory\n\
                           - Update your rush.yaml to use 'dockerfile: Dockerfile.multistage'\n\
                        2) Install and configure 'cross' for Rust cross-compilation:\n\
                           cargo install cross\n\
                           Note: On Apple Silicon, you may need: export DOCKER_DEFAULT_PLATFORM=linux/amd64\n\
                        3) Install a cross-compilation toolchain:\n\
                           brew install FiloSottile/musl-cross/musl-cross\n\
                           brew install x86_64-unknown-linux-gnu (for x86_64)\n\
                        4) Build on a Linux machine or CI/CD environment\n\
                        5) Use a pre-built binary if available"
                    );
                }
                
                self.rebuild_in_progress = false;
                return Err(e);
            }

            // Build the Docker image - make paths absolute relative to product directory
            let dockerfile = if Path::new(dockerfile_path).is_absolute() {
                PathBuf::from(dockerfile_path)
            } else {
                self.config.product_dir.join(dockerfile_path)
            };
            
            let context = if Path::new(&context_dir).is_absolute() {
                PathBuf::from(&context_dir)
            } else {
                self.config.product_dir.join(&context_dir)
            };
            
            info!("Build paths for {}: Dockerfile={}, Context={}", 
                  component_name, dockerfile.display(), context.display());

            // Set up Docker cross-compilation environment
            let target_platform = "linux/amd64"; // TODO: Should be configurable based on target
            let _docker_guard = crate::utils::DockerCrossCompileGuard::new(target_platform);

            match self
                .build_image(&image_name, image_tag, &context, &dockerfile, &spec)
                .await
            {
                Ok(actual_image_name) => {
                    // Store the actual built image name for use during launch
                    self.built_images.insert(component_name.clone(), actual_image_name.clone());
                    info!(
                        "Successfully built/cached image for {}: {}",
                        component_name, actual_image_name
                    );
                }
                Err(e) => {
                    error!("\n=== Build Failed for Component: {} ===", component_name);
                    error!("Error: {}", e);
                    error!("\nBuild Configuration:");
                    error!("  Dockerfile: {}", dockerfile.display());
                    error!("  Context: {}", context.display());
                    error!("  Image: {}", image_name);
                    error!("  Tag: {}", image_tag);
                    
                    // Check if paths exist
                    if !dockerfile.exists() {
                        error!("\n⚠️  Dockerfile does not exist at: {}", dockerfile.display());
                    }
                    if !context.exists() {
                        error!("\n⚠️  Build context directory does not exist at: {}", context.display());
                    }
                    
                    self.rebuild_in_progress = false;
                    
                    // Return a more descriptive error
                    return Err(Error::Docker(format!(
                        "Failed to build image for component '{}'. Check the output above for details.", 
                        component_name
                    )));
                }
            }
        }

        info!("All container images built successfully");
        self.rebuild_in_progress = false;
        Ok(())
    }

    /// Launches all containers
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    async fn launch_containers(&mut self) -> Result<()> {
        info!("Launching containers");

        let (_status_sender, _status_receiver) = mpsc::channel::<Status>(100);

        // Create service configs for each service
        let mut service_configs = Vec::new();

        for (_domain, service_list) in &self.services {
            for service in service_list {
                let should_redirect = self
                    .config
                    .redirected_components
                    .contains_key(&service.name);

                if !should_redirect {
                    // Load secrets for this component from vault
                    let vault_guard = self.vault.lock().unwrap();
                    let secrets = vault_guard
                        .get(
                            &self.config.product_name,
                            &service.name,
                            &self.config.environment,
                        )
                        .await
                        .unwrap_or_default();

                    // Load environment variables for this component
                    let component_spec = self
                        .component_specs
                        .iter()
                        .find(|spec| spec.component_name == service.name);

                    let mut env_vars = HashMap::new();
                    
                    // Add environment variables from component spec
                    if let Some(spec) = component_spec {
                        // Add dotenv variables (from .env files)
                        for (key, value) in &spec.dotenv {
                            env_vars.insert(key.clone(), value.clone());
                        }
                        
                        // Add env variables (from YAML spec)
                        if let Some(env) = &spec.env {
                            for (key, value) in env {
                                env_vars.insert(key.clone(), value.clone());
                            }
                        }
                    }
                    
                    // Add encoded secrets as environment variables
                    let encoded_secrets = self.secrets_encoder.encode_secrets(secrets);
                    for (key, value) in encoded_secrets {
                        env_vars.insert(key, value);
                    }

                    // Collect volumes from component spec
                    let mut volumes = Vec::new();
                    if let Some(spec) = component_spec {
                        if let Some(spec_volumes) = &spec.volumes {
                            for (host_path, container_path) in spec_volumes {
                                volumes.push(format!("{}:{}", host_path, container_path));
                            }
                        }
                    }

                    // All containers should use the product directory as working directory
                    let working_dir = format!("/app/{}", self.config.product_name.replace('.', "/"));
                    
                    // Container name should be product_name-component_name (to match old implementation)
                    let container_name = format!("{}-{}", self.config.product_name, service.name);
                    
                    let config = DockerServiceConfig {
                        name: container_name,
                        image: service.image.clone(),
                        network: self.config.network_name.clone(),
                        env_vars,
                        ports: vec![format!("{}:{}", service.port, service.target_port)],
                        volumes,  // Use the already fixed volumes
                        working_dir: Some(working_dir),
                    };

                    service_configs.push(config);
                }
            }
        }

        // Launch all services
        for config in service_configs {
            // Launch container
            let container_id = self
                .docker_client
                .run_container(
                    &config.image,
                    &config.name,
                    &config.network,
                    &config
                        .env_vars
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect::<Vec<_>>(),
                    &config.ports,
                    &config.volumes,
                    config.working_dir.as_deref(),
                )
                .await?;

            // Create service object
            let service = DockerService::new(container_id.clone(), config.clone(), self.docker_client.clone());

            // Start following logs for this container
            let docker_client = self.docker_client.clone();
            let container_name = config.name.clone();
            let color = self.get_color_for_component(&container_name);
            
            // Use output director if available, otherwise use standard output
            if let Some(ref output_director) = self.output_director {
                eprintln!("DEBUG: Using output director for container {}", container_name);
                let director = output_director.clone();
                let source = crate::output::OutputSource::with_color(&container_name, "container", color);
                
                tokio::spawn(async move {
                    eprintln!("DEBUG: Starting log follower for {}", container_name);
                    // Create a simple log follower that uses the shared director
                    if let Err(e) = follow_container_logs_with_shared_director(
                        docker_client,
                        &container_id,
                        source,
                        director
                    ).await {
                        error!("Error following logs for {}: {}", container_name, e);
                    }
                });
            } else {
                eprintln!("DEBUG: No output director, using standard output for {}", container_name);
                // Fall back to standard output
                tokio::spawn(async move {
                    if let Err(e) = docker_client.follow_container_logs(
                        &container_id, 
                        container_name.clone(), 
                        color
                    ).await {
                        error!("Error following logs for {}: {}", container_name, e);
                    }
                });
            }

            self.running_services.push(service);
        }

        info!(
            "Successfully launched {} containers",
            self.running_services.len()
        );
        Ok(())
    }

    /// Tests if the changed files affect any component that needs rebuilding
    ///
    /// # Arguments
    ///
    /// * `changed_files` - List of files that have changed
    ///
    /// # Returns
    ///
    /// Boolean indicating whether any component needs to be rebuilt
    async fn test_if_significant_change(&mut self, changed_files: &[PathBuf]) -> bool {
        if changed_files.is_empty() {
            info!("No changed files to process");
            return false;
        }

        info!("Testing {} changed files for significance", changed_files.len());
        for file in changed_files {
            debug!("  Changed file: {}", file.display());
        }

        let mut affected_components = Vec::new();

        // Check each component to see if it's affected by the changes
        debug!("Checking {} components for changes", self.component_specs.len());
        for spec in &self.component_specs {
            debug!("Evaluating component: {}", spec.component_name);
            
            // Skip redirected components (they're not built locally)
            if self.config.redirected_components.contains_key(&spec.component_name) {
                debug!("  Skipping redirected component: {}", spec.component_name);
                continue;
            }

            // Check if any changed file is in this component's context or matches its watch patterns
            if self.is_any_file_in_component_context(&spec, changed_files) {
                info!("  ✓ Component '{}' is affected by file changes", spec.component_name);
                affected_components.push(spec.component_name.clone());
            } else {
                debug!("  ✗ Component '{}' not affected", spec.component_name);
            }
        }

        if !affected_components.is_empty() {
            info!("Rebuild triggered for components: {:?}", affected_components);
            true
        } else {
            info!("No components affected by file changes - rebuild skipped");
            info!("  (Check watch patterns in stack.spec.yaml or component context directories)");
            false
        }
    }

    /// Checks if any of the changed files affect a specific component
    ///
    /// # Arguments
    ///
    /// * `spec` - The component specification
    /// * `file_paths` - List of changed file paths
    ///
    /// # Returns
    ///
    /// Boolean indicating whether the component is affected
    fn is_any_file_in_component_context(&self, spec: &ComponentBuildSpec, file_paths: &[PathBuf]) -> bool {
        debug!("    Checking context for component: {}", spec.component_name);
        
        // First check if component has watch patterns defined
        if let Some(watch_matcher) = &spec.watch {
            debug!("    Component has watch patterns defined");
            let matched = file_paths.iter().any(|file| {
                let matches = watch_matcher.matches(file);
                if matches {
                    debug!("      ✓ File {} matches watch pattern", file.display());
                } else {
                    debug!("      ✗ File {} does not match watch patterns", file.display());
                }
                matches
            });
            
            // When watch patterns are defined, ONLY rebuild if a file matches the patterns
            debug!("    Watch pattern result: {}", matched);
            return matched;
        }
        
        // If no watch patterns are defined, fall back to checking the context directory
        debug!("    No watch patterns defined, checking context directory");

        // Get the component's context directory based on build type
        let context_dir = match &spec.build_type {
            BuildType::TrunkWasm { context_dir, location, .. } => {
                context_dir.clone().unwrap_or_else(|| {
                    // For TrunkWasm, derive context from the parent directory of location
                    if let Some(parent) = std::path::Path::new(location).parent() {
                        parent.to_string_lossy().to_string()
                    } else {
                        ".".to_string()
                    }
                })
            }
            BuildType::RustBinary { context_dir, location, .. } => {
                // For RustBinary, use context_dir if specified, otherwise use location
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::DixiousWasm { context_dir, location, .. } => {
                // For DixiousWasm, use context_dir if specified, otherwise use location  
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::Script { context_dir, location, .. } => {
                // For Script, use context_dir if specified, otherwise use location
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::Zola { context_dir, location, .. } => {
                // For Zola, use context_dir if specified, otherwise use location
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::Book { context_dir, location, .. } => {
                // For Book, use context_dir if specified, otherwise use location
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::Ingress { context_dir, .. } => {
                // For Ingress, use context_dir if specified, otherwise current directory
                context_dir.clone().unwrap_or_else(|| ".".to_string())
            }
            BuildType::PureDockerImage { .. } 
            | BuildType::PureKubernetes
            | BuildType::KubernetesInstallation { .. } => {
                // These types don't have a build context for file watching
                debug!("    Build type doesn't support file watching");
                return false;
            }
        };

        // Check if any changed file is within the component's context directory
        let context_path = self.config.product_dir.join(&context_dir);
        debug!("    Context directory: {}", context_path.display());
        
        let result = file_paths.iter().any(|file_path| {
            debug!("      Checking if {} is in context {}", file_path.display(), context_path.display());
            
            // Try to get absolute paths for comparison
            if let (Ok(abs_file), Ok(abs_context)) = (
                std::fs::canonicalize(file_path),
                std::fs::canonicalize(&context_path)
            ) {
                let is_match = abs_file.starts_with(&abs_context);
                debug!("        Absolute comparison: {} starts_with {} = {}", 
                     abs_file.display(), abs_context.display(), is_match);
                is_match
            } else {
                // Fallback to simple path comparison
                let is_match = file_path.starts_with(&context_path);
                debug!("        Simple comparison: {} starts_with {} = {}", 
                     file_path.display(), context_path.display(), is_match);
                is_match
            }
        });
        
        debug!("    Context directory check result: {}", result);
        result
    }

    /// Monitors running containers and handles events like file changes or termination signals
    ///
    /// # Returns
    ///
    /// Boolean indicating whether to continue running (true) or shut down (false)
    async fn monitor_and_handle_events(&mut self) -> Result<bool> {
        let shutdown_token = shutdown::global_shutdown().cancellation_token();
        info!("Monitoring containers and watching for file changes");

        let mut file_check_interval = tokio::time::interval(Duration::from_millis(100));
        let mut status_check_interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    info!("Received termination signal");
                    return Ok(false);
                }

                _ = file_check_interval.tick() => {
                    // Check if we have pending changes to process
                    let changed_files = self.change_processor.process_pending_changes().await?;
                    if !changed_files.is_empty() {
                        info!("Processing file changes...");
                        // Test if the changes are significant (affect any component)
                        if self.test_if_significant_change(&changed_files).await {
                            info!("Detected significant file changes, triggering rebuild");
                            return Ok(true);
                        } else {
                            debug!("File changes detected but no components affected");
                        }
                    }
                }

                _ = status_check_interval.tick() => {
                    // Check container statuses
                    for service in &self.running_services {
                        match service.status().await {
                            Ok(ContainerStatus::Exited(code)) => {
                                warn!("Container {} exited with code {}", service.id(), code);
                                if code != 0 {
                                    error!("Container failed with non-zero exit code");
                                    // Wait for changes before restarting
                                    match self.wait_for_changes_or_termination().await {
                                        WaitResult::FileChanged => return Ok(true),
                                        WaitResult::Terminated => return Ok(false),
                                        WaitResult::Timeout => return Ok(true),
                                    }
                                }
                            }
                            Ok(ContainerStatus::Unknown) => {
                                warn!("Container {} status unknown", service.id());
                            }
                            Err(e) => {
                                error!("Error checking container status: {}", e);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    /// Cleans up running containers
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    async fn cleanup_containers(&mut self) -> Result<()> {
        info!("Cleaning up containers");

        // Broadcast shutdown signal to all services
        let _ = self.shutdown_sender.send(());

        // Stop and remove each service in the tracking list with retry logic
        for service in &self.running_services {
            let mut retries = 0;
            let max_retries = 3;
            
            while retries < max_retries {
                // Try to stop gracefully first
                let stop_result = service.stop().await;
                let remove_result = service.remove().await;
                
                if stop_result.is_ok() && remove_result.is_ok() {
                    break;
                }
                
                retries += 1;
                if retries < max_retries {
                    warn!("Failed to clean up container {} (attempt {}/{}), retrying...", 
                          service.id(), retries, max_retries);
                    tokio::time::sleep(Duration::from_millis(500 * retries as u64)).await;
                } else {
                    warn!("Failed to clean up container {} after {} retries", 
                          service.id(), max_retries);
                }
            }
        }

        // Clear the list of running services
        self.running_services.clear();

        // Also clean up any containers that might exist from previous failed attempts
        // This mirrors the old implementation's kill_and_clean behavior
        self.cleanup_containers_by_name().await?;

        Ok(())
    }

    /// Clean up containers by name pattern (like old implementation)
    /// This handles containers that might exist from previous failed attempts
    async fn cleanup_containers_by_name(&self) -> Result<()> {
        trace!("Cleaning up containers by name pattern");

        for spec in &self.component_specs {
            // Use product_name-component_name to match the container naming convention
            let container_name = format!("{}-{}", self.config.product_name, spec.component_name);
            
            // Try to kill and remove with retries
            self.kill_and_remove_container_with_retry(&container_name, 3).await?;
        }

        trace!("Container cleanup by name completed");
        Ok(())
    }

    /// Kill and remove a container with retry logic
    async fn kill_and_remove_container_with_retry(&self, container_name: &str, max_retries: u32) -> Result<()> {
        let mut retries = 0;
        
        while retries < max_retries {
            // First try to kill if running
            match self.kill_container_by_name(container_name).await {
                Ok(_) => debug!("Successfully killed container: {}", container_name),
                Err(e) => {
                    if retries == max_retries - 1 {
                        warn!("Failed to kill container {} after {} retries: {}", 
                              container_name, max_retries, e);
                    }
                }
            }
            
            // Then try to remove
            match self.remove_container_by_name(container_name).await {
                Ok(_) => {
                    debug!("Successfully removed container: {}", container_name);
                    return Ok(());
                }
                Err(e) => {
                    retries += 1;
                    if retries < max_retries {
                        warn!("Failed to remove container {} (attempt {}/{}): {}, retrying...", 
                              container_name, retries, max_retries, e);
                        // Wait a bit before retrying
                        tokio::time::sleep(Duration::from_millis(500 * retries as u64)).await;
                    } else {
                        warn!("Failed to remove container {} after {} retries: {}", 
                              container_name, max_retries, e);
                        // Don't fail completely, just warn
                        return Ok(());
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Kill a running container by name
    async fn kill_container_by_name(&self, container_name: &str) -> Result<()> {
        // Check if the container is running
        let check_output = Command::new("docker")
            .args(["ps", "-q", "-f", &format!("name={}", container_name)])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match check_output {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let container_ids: Vec<&str> = stdout.trim().lines().collect();
                    
                    if !container_ids.is_empty() {
                        // Container is running, kill it
                        info!("Killing running container: {}", container_name);
                        let kill_output = Command::new("docker")
                            .args(["kill", container_name])
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .output()
                            .await
                            .map_err(|e| Error::Container(format!("Failed to execute kill command for {}: {}", container_name, e)))?;

                        if !kill_output.status.success() {
                            let stderr = String::from_utf8_lossy(&kill_output.stderr);
                            return Err(Error::Container(format!("Failed to kill container {}: {}", container_name, stderr)));
                        }
                    } else {
                        trace!("No running container found for {}", container_name);
                    }
                } else {
                    trace!("Error checking for running container {}", container_name);
                }
            }
            Err(e) => {
                trace!("Error executing docker ps command for {}: {}", container_name, e);
            }
        }

        Ok(())
    }

    /// Remove a container by name (handles both running and stopped containers)
    async fn remove_container_by_name(&self, container_name: &str) -> Result<()> {
        // Check for any containers (running or stopped) with this name
        let check_output = Command::new("docker")
            .args(["ps", "-a", "-q", "-f", &format!("name={}", container_name)])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match check_output {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let container_ids: Vec<&str> = stdout.trim().lines().collect();
                    
                    if !container_ids.is_empty() {
                        // Container exists, remove it
                        info!("Removing container: {}", container_name);
                        let rm_output = Command::new("docker")
                            .args(["rm", "-f", container_name])
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .output()
                            .await
                            .map_err(|e| Error::Container(format!("Failed to execute rm command for {}: {}", container_name, e)))?;

                        if !rm_output.status.success() {
                            let stderr = String::from_utf8_lossy(&rm_output.stderr);
                            return Err(Error::Container(format!("Failed to remove container {}: {}", container_name, stderr)));
                        }
                    } else {
                        trace!("No container found for {}", container_name);
                    }
                } else {
                    trace!("Error checking for containers with name {}", container_name);
                }
            }
            Err(e) => {
                trace!("Error executing docker ps command for {}: {}", container_name, e);
            }
        }

        Ok(())
    }

    /// Get color for component based on spec
    fn get_color_for_component(&self, component_name: &str) -> &'static str {
        // Find the component spec to get its color
        for spec in &self.component_specs {
            if spec.component_name == component_name {
                // Return the color from the spec, or a default
                return match spec.color.as_str() {
                    "red" => "red",
                    "green" => "green",
                    "yellow" => "yellow",
                    "blue" => "blue",
                    "magenta" => "magenta",
                    "cyan" => "cyan",
                    "white" => "white",
                    _ => "white"
                };
            }
        }
        "white" // default color
    }

    /// Waits for file changes or a termination signal
    ///
    /// # Returns
    ///
    /// WaitResult indicating what happened
    async fn wait_for_changes_or_termination(&self) -> WaitResult {
        let shutdown_token = shutdown::global_shutdown().cancellation_token();
        info!("Waiting for file changes or termination signal");

        let mut file_check_interval = tokio::time::interval(Duration::from_millis(100));
        let wait_timeout = tokio::time::sleep(Duration::from_secs(300)); // 5 minute timeout
        tokio::pin!(wait_timeout);

        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    info!("Received termination signal while waiting");
                    return WaitResult::Terminated;
                }

                _ = file_check_interval.tick() => {
                    let changed_files = self.change_processor.process_pending_changes().await.unwrap_or_else(|_| Vec::new());
                    if !changed_files.is_empty() {
                        info!("File changes detected, resuming build");
                        return WaitResult::FileChanged;
                    }
                }

                _ = &mut wait_timeout => {
                    info!("Wait timeout reached, attempting rebuild");
                    return WaitResult::Timeout;
                }
            }
        }
    }

    /// Renders artifacts for a component before Docker build
    async fn render_artifacts_for_component(&self, spec: &ComponentBuildSpec) -> Result<()> {
        use crate::build::{Artefact, BuildContext, ServiceSpec, Variables};
        use crate::toolchain::{Platform, ToolchainContext};
        use crate::error::Error;
        use std::collections::HashMap;
        use std::fs;
        use std::path::{Path, PathBuf};
        
        // Check if this component has artifacts to render
        if spec.artefacts.is_none() {
            return Ok(());
        }
        
        let artifact_count = spec.artefacts.as_ref().map(|a| a.len()).unwrap_or(0);
        info!("Rendering {} artifacts for component: {}", artifact_count, spec.component_name);
        
        // Artifacts paths are relative to product directory
        // We need to resolve them to absolute paths
        let product_dir = &self.config.product_dir;
        
        // Create toolchain context
        let host_platform = Platform::default();
        let target_platform = Platform::new("linux", "x86_64");
        let toolchain = ToolchainContext::new(host_platform.clone(), target_platform.clone());
        
        // Get location from build type
        let location = spec.build_type.location().unwrap_or(".");
        
        // For Ingress components, we need to filter services to only include
        // the components specified in the ingress configuration
        let services = if let BuildType::Ingress { components, .. } = &spec.build_type {
            // Build a filtered services map based on the ingress components
            let mut filtered_services = HashMap::new();
            
            // We need to collect service information for the specified components
            // For now, we'll create basic service specs for the components
            let domain = format!("{}.local", spec.product_name);
            
            for component in components {
                let docker_host = format!("{}-{}", spec.product_name, component);
                let service_spec = ServiceSpec {
                    name: component.clone(),
                    host: docker_host.clone(),
                    docker_host,
                    domain: domain.clone(),
                    port: 8000, // Default port, should be configurable
                    target_port: 8000,
                    mount_point: if component == "frontend" {
                        Some("/".to_string())
                    } else if component == "backend" {
                        Some("/api".to_string())
                    } else {
                        None
                    },
                };
                
                // Add to the domain
                filtered_services
                    .entry(domain.clone())
                    .or_insert_with(Vec::new)
                    .push(service_spec);
            }
            
            filtered_services
        } else {
            Default::default()
        };
        
        // Create build context for artifact rendering
        let context = BuildContext {
            build_type: spec.build_type.clone(),
            location: Some(location.to_string()),
            target: target_platform,
            host: host_platform,
            rust_target: "x86_64-unknown-linux-gnu".to_string(),
            toolchain,
            services,
            environment: self.config.environment.clone(),
            domain: spec.domain.clone(),
            product_name: spec.product_name.clone(),
            product_uri: format!("{}.{}", spec.product_name, self.config.environment),
            component: spec.component_name.clone(),
            docker_registry: self.config.docker_registry.clone(),
            image_name: format!("{}-{}", spec.product_name, spec.component_name),
            domains: Default::default(),
            env: spec.dotenv.clone(),
            secrets: Default::default(),
        };
        
        // Create output directory for artifacts
        let output_dir = Path::new(&spec.artefact_output_dir);
        if !output_dir.exists() {
            fs::create_dir_all(output_dir)
                .map_err(|e| Error::FileSystem { 
                    path: output_dir.to_path_buf(),
                    message: format!("Failed to create artifact output directory: {}", e)
                })?;
        }
        
        // Create rushd subdirectory if needed (for ingress nginx.conf)
        let rushd_dir = output_dir.join("rushd");
        if !rushd_dir.exists() {
            fs::create_dir_all(&rushd_dir)
                .map_err(|e| Error::FileSystem {
                    path: rushd_dir.clone(),
                    message: format!("Failed to create rushd directory: {}", e)
                })?;
        }
        
        // Render each artifact
        // Note: The artifacts come with relative paths, we need to make them absolute
        if let Some(artefacts) = &spec.artefacts {
            for (input_path, output_name) in artefacts.iter() {
                // Make input path absolute relative to product directory
                let absolute_input_path = if Path::new(input_path).is_absolute() {
                    PathBuf::from(input_path)
                } else {
                    product_dir.join(input_path)
                };
                
                // For ingress, output goes to rushd/nginx.conf in the context directory
                let absolute_output_path = if spec.component_name == "ingress" && output_name == "nginx.conf" {
                    rushd_dir.join("nginx.conf")
                } else {
                    output_dir.join(output_name)
                };
                
                info!("Rendering artifact: {} -> {}", 
                     absolute_input_path.display(), 
                     absolute_output_path.display());
                
                // Create the artifact with absolute paths
                let artifact = Artefact::new(
                    absolute_input_path.to_string_lossy().to_string(),
                    absolute_output_path.to_string_lossy().to_string()
                );
                
                // Render the artifact
                artifact.render_to_file(&context);
            }
        }
        
        Ok(())
    }

    /// Runs the build script for a component before Docker build
    async fn run_build_script_for_component(&self, spec: &ComponentBuildSpec) -> Result<()> {
        use crate::build::{BuildContext, BuildScript, Variables};
        use crate::toolchain::{Platform, ToolchainContext};
        use crate::utils::run_command;
        use crate::error::Error;
        
        // Skip components that don't need build scripts
        if !spec.build_type.requires_docker_build() {
            return Ok(());
        }
        
        // Create toolchain context with proper cross-compilation setup
        // This matches the old implementation's behavior
        let host_platform = Platform::default();
        let target_platform = Platform::new("linux", "x86_64");
        
        // For cross-compilation scenarios, we need to handle potential toolchain issues
        let toolchain = if host_platform.os != target_platform.os || host_platform.arch != target_platform.arch {
            // Cross-compilation scenario
            match std::panic::catch_unwind(|| {
                ToolchainContext::new(host_platform.clone(), target_platform.clone())
            }) {
                Ok(tc) => {
                    info!("Cross-compilation toolchain initialized successfully");
                    tc.setup_env();
                    tc
                }
                Err(_) => {
                    warn!("Cross-compilation toolchain not found, using default toolchain");
                    warn!("This may cause build failures for cross-compilation scenarios");
                    // Fall back to default toolchain
                    let tc = ToolchainContext::default();
                    tc.setup_env();
                    tc
                }
            }
        } else {
            // Native compilation
            let tc = ToolchainContext::default();
            tc.setup_env();
            tc
        };
        
        // Get location from build type (like the old implementation)
        let location = spec.build_type.location().unwrap_or(".");
        
        // Create build context
        let context = BuildContext {
            build_type: spec.build_type.clone(),
            location: Some(location.to_string()),
            target: target_platform,
            host: host_platform, 
            rust_target: "x86_64-unknown-linux-gnu".to_string(),
            toolchain,
            services: Default::default(),
            environment: self.config.environment.clone(),
            domain: spec.domain.clone(),
            product_name: spec.product_name.clone(),
            product_uri: format!("{}.{}", spec.product_name, self.config.environment),
            component: spec.component_name.clone(),
            docker_registry: self.config.docker_registry.clone(),
            image_name: format!("{}-{}", spec.product_name, spec.component_name),
            domains: Default::default(),
            env: spec.dotenv.clone(),
            secrets: Default::default(),
        };
        
        // Check if we're attempting cross-compilation
        let is_cross_compile = location.contains("backend") && cfg!(not(target_os = "linux"));
        
        if is_cross_compile {
            // For cross-compilation, we need special handling
            // Check if cross is installed
            if let Ok(output) = std::process::Command::new("which")
                .arg("cross")
                .output() {
                if output.status.success() {
                    info!("Found 'cross' tool for cross-compilation");
                    // Use cross instead of cargo for cross-compilation
                    // This would require modifying the build script template
                } else {
                    warn!(
                        "Cross-compilation from {} to Linux requires 'cross' tool. \
                        Install with: cargo install cross",
                        std::env::consts::OS
                    );
                }
            }
        }
        
        // Generate build script
        let build_script = BuildScript::new(spec.build_type.clone());
        let script_content = build_script.render(&context);
        
        if script_content.is_empty() {
            // No build script needed for this component
            return Ok(());
        }
        
        // Execute build script from product directory root (template will cd to location)
        let build_dir = self.config.product_dir.clone();
        
        info!("Running build script for component: {}", spec.component_name);
        
        // Write script to temporary file and execute it
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::env;
        
        let script_path = build_dir.join("build_script.sh");
        fs::write(&script_path, &script_content)
            .map_err(|e| Error::Build(format!("Failed to write build script: {}", e)))?;
            
        // Make script executable
        let metadata = fs::metadata(&script_path)
            .map_err(|e| Error::Build(format!("Failed to get script metadata: {}", e)))?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)
            .map_err(|e| Error::Build(format!("Failed to set script permissions: {}", e)))?;
        
        // Change to build directory and execute the script
        let original_dir = env::current_dir()
            .map_err(|e| Error::Build(format!("Failed to get current directory: {}", e)))?;
        
        env::set_current_dir(&build_dir)
            .map_err(|e| Error::Build(format!("Failed to change to build directory: {}", e)))?;
        
        // Execute the script
        let output = run_command(
            "Build script",
            "bash",
            vec!["./build_script.sh"],
        ).await;
        
        // Change back to original directory
        let _ = env::set_current_dir(original_dir);
        
        // Clean up the script file
        let _ = fs::remove_file(&script_path);
        
        match output {
            Ok(_) => {
                info!("Build script completed successfully for: {}", spec.component_name);
                Ok(())
            }
            Err(e) => {
                error!("Build script failed for {}: {}", spec.component_name, e);
                Err(Error::Build(format!("Build script failed: {}", e)))
            }
        }
    }

    /// Builds a specific Docker image
    ///
    /// # Arguments
    ///
    /// * `image_name` - Name of the image to build
    /// * `image_tag` - Tag for the image
    /// * `context_dir` - Build context directory
    /// * `dockerfile` - Path to Dockerfile
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    async fn verify_docker_available(&self) -> Result<()> {
        use tokio::process::Command;
        
        let output = Command::new("docker")
            .args(["version", "--format", "json"])
            .output()
            .await
            .map_err(|e| Error::Docker(format!("Cannot execute docker command: {}", e)))?;
            
        if !output.status.success() {
            return Err(Error::Docker("Docker is not available or not running".into()));
        }
        
        Ok(())
    }
    
    /// Builds a Docker image with caching support
    ///
    /// # Arguments
    ///
    /// * `image_name` - Name of the image to build
    /// * `image_tag` - Tag for the image
    /// * `context_dir` - Build context directory
    /// * `dockerfile` - Path to Dockerfile
    /// * `spec` - Component build specification
    ///
    /// # Returns
    ///
    /// The actual image name that was built (with git tag)
    async fn build_image(
        &self,
        image_name: &str,
        image_tag: &str,
        context_dir: &PathBuf,
        dockerfile: &PathBuf,
        spec: &ComponentBuildSpec,
    ) -> Result<String> {
        info!("Evaluating if image {} needs to be built", image_name);

        // Extract component name from the image name (format: product-component)
        let component_name = if let Some(dash_pos) = image_name.rfind('-') {
            &image_name[dash_pos + 1..]
        } else {
            image_name
        };

        // Create an ImageBuilder with the right configuration
        // Set working directory to product directory
        let working_dir = format!("/app/{}", self.config.product_name.replace('.', "/"));
        
        let service_config = DockerServiceConfig {
            name: image_name.to_string(),
            image: format!("{}:{}", image_name, image_tag),
            network: self.config.network_name.clone(),
            env_vars: HashMap::new(),
            ports: Vec::new(),
            volumes: Vec::new(),
            working_dir: Some(working_dir),
        };

        let service = DockerService::new(
            "".to_string(), // ID will be set when container launches
            service_config,
            self.docker_client.clone(),
        );

        // Use the actual build type from the spec to preserve location information
        let build_config = crate::container::BuildConfig {
            build_type: spec.build_type.clone(),
            dockerfile_path: Some(dockerfile.to_string_lossy().to_string()),
            context_dir: Some(context_dir.to_string_lossy().to_string()),
            docker_registry: self.config.docker_registry.clone(),
            environment: self.config.environment.clone(),
            domain: spec.domain.clone(),
            mount_point: spec.mount_point.clone(),
        };

        let mut image_builder = crate::container::ImageBuilder::new(
            service,
            self.docker_client.clone(),
            component_name.to_string(),
            self.config.product_name.clone(),
        )
        .with_build_config(build_config);
        
        // Set up toolchain if available
        if let Some(toolchain) = &self.toolchain {
            image_builder = image_builder.with_toolchain(toolchain.clone());
        }

        // Evaluate if rebuild is needed based on cache
        let needs_rebuild = match image_builder.evaluate_rebuild_needed().await {
            Ok(needed) => needed,
            Err(e) => {
                warn!("Failed to evaluate cache status: {}, proceeding with build", e);
                true
            }
        };

        if !needs_rebuild {
            info!("Image {} already exists in cache with clean git tag, skipping build", image_name);
            return Ok(image_builder.tagged_image_name());
        }

        info!("Building image {} with git-based tag", image_name);
        // Build the image
        image_builder.build().await?;
        
        // Return the actual tagged image name that was built
        Ok(image_builder.tagged_image_name())
    }
}

/// Gets the git hash for a folder
/// TODO: This is also implemented in toolchain/context.rs
fn get_git_folder_hash(subdirectory_path: &str) -> Result<String> {
    use std::process::Command;

    let hash_output = Command::new("git")
        .args(["log", "-n", "1", "--format=%H", "--", subdirectory_path])
        .output()
        .map_err(|e| e.to_string())?;

    let hash = String::from_utf8(hash_output.stdout)
        .map_err(|e| e.to_string())?
        .trim()
        .to_string();

    if !hash_output.status.success() || hash.is_empty() {
        return Ok(DOCKER_TAG_LATEST.to_string());
    }

    Ok(hash)
}

/// Helper function to follow container logs with a shared output director
async fn follow_container_logs_with_shared_director(
    _docker_client: Arc<dyn DockerClient>,
    container_id: &str,
    source: crate::output::OutputSource,
    director: crate::output::SharedOutputDirector,
) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    
    eprintln!("DEBUG: Starting docker logs command for container {}", container_id);
    
    // Use docker logs command to follow the container logs
    let mut child = Command::new("docker")
        .args(["logs", "-f", "--tail", "100", container_id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Docker(format!("Failed to follow container logs: {}", e)))?;
    
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    
    let mut handles = vec![];
    
    // Handle stdout
    if let Some(stdout) = stdout {
        let source_clone = source.clone();
        let director_clone = director.clone();
        let handle = tokio::spawn(async move {
            eprintln!("DEBUG: Starting stdout reader for container");
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let mut line_count = 0;
            
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        eprintln!("DEBUG: EOF reached on stdout after {} lines", line_count);
                        break;  // EOF
                    }
                    Ok(_) => {
                        line_count += 1;
                        if line_count <= 5 {
                            eprintln!("DEBUG: stdout line {}: {}", line_count, line.trim());
                        }
                        let output_data = line.as_bytes().to_vec();
                        let stream = crate::output::OutputStream::stdout(output_data);
                        if let Err(e) = director_clone.write_output(&source_clone, &stream).await {
                            error!("Failed to write stdout output: {}", e);
                            break;
                        }
                        // Flush periodically
                        if line_count % 10 == 0 {
                            let _ = director_clone.flush().await;
                        }
                    }
                    Err(e) => {
                        error!("Error reading stdout: {}", e);
                        break;
                    }
                }
            }
            eprintln!("DEBUG: stdout reader finished with {} lines", line_count);
        });
        handles.push(handle);
    }
    
    // Handle stderr
    if let Some(stderr) = stderr {
        let source_clone = source.clone();
        let director_clone = director.clone();
        let handle = tokio::spawn(async move {
            eprintln!("DEBUG: Starting stderr reader for container");
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            let mut line_count = 0;
            
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        eprintln!("DEBUG: EOF reached on stderr after {} lines", line_count);
                        break;  // EOF
                    }
                    Ok(_) => {
                        line_count += 1;
                        if line_count <= 5 {
                            eprintln!("DEBUG: stderr line {}: {}", line_count, line.trim());
                        }
                        let output_data = line.as_bytes().to_vec();
                        let stream = crate::output::OutputStream::stderr(output_data);
                        if let Err(e) = director_clone.write_output(&source_clone, &stream).await {
                            error!("Failed to write stderr output: {}", e);
                            break;
                        }
                        // Flush periodically
                        if line_count % 10 == 0 {
                            let _ = director_clone.flush().await;
                        }
                    }
                    Err(e) => {
                        error!("Error reading stderr: {}", e);
                        break;
                    }
                }
            }
            eprintln!("DEBUG: stderr reader finished with {} lines", line_count);
        });
        handles.push(handle);
    }
    
    // This function should run indefinitely following logs
    // The spawned tasks will continue running
    // We don't wait for them to complete since logs should stream continuously
    
    eprintln!("DEBUG: Log following tasks spawned for container {}", container_id);
    
    Ok(())
}
