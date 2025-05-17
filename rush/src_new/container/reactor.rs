//! Container orchestration and lifecycle management
//!
//! The reactor is responsible for orchestrating container lifecycle events,
//! monitoring containers, and handling file change events that trigger rebuilds.
use crate::build::{BuildType, ComponentBuildSpec, Variables};
use crate::container::DockerCliClient;
use crate::container::Status;
use crate::container::{
    docker::{ContainerStatus, DockerClient, DockerService, DockerServiceConfig},
    network::setup_network,
    watcher::{setup_file_watcher, ChangeProcessor, WatcherConfig},
    BuildProcessor, ContainerService, ServiceCollection,
};
use crate::core::config::Config;
use crate::error::Result;
use crate::security::{FileVault, SecretsEncoder, Vault};

use log::{error, info, trace, warn};
use serde_yaml;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

/// Manages the container lifecycle and coordinates rebuilds based on file changes
pub struct ContainerReactor {
    /// Configuration for the reactor
    config: Arc<ContainerReactorConfig>,

    /// Collection of services managed by this reactor
    services: ServiceCollection,

    /// File change processor for detecting code changes
    change_processor: Arc<ChangeProcessor>,

    /// Docker client for container operations
    docker_client: Arc<dyn DockerClient>,

    /// Build processor for container builds
    build_processor: BuildProcessor,

    /// Vault for accessing secrets
    vault: Arc<Mutex<dyn Vault + Send>>,

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
        let (_, change_processor) = setup_file_watcher(config.watch_config.clone())?;

        let (shutdown_sender, _) = broadcast::channel(8);

        Ok(Self {
            config,
            services: HashMap::new(),
            change_processor: Arc::new(change_processor),
            docker_client,
            build_processor: BuildProcessor::new(false),
            vault,
            running_services: Vec::new(),
            shutdown_sender,
            rebuild_in_progress: false,
            available_components: Vec::new(),
            component_specs: Vec::new(),
            secrets_encoder,
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
            git_hash: "latest".to_string(),
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
                    "latest".to_string()
                } else {
                    hash[..8].to_string()
                }
            }
            Err(_) => "latest".to_string(),
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

            let service = Arc::new(ContainerService {
                id: "TODO".to_string(),
                name: component_name.clone(),
                image: component_name.clone(),
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

    /// Main container lifecycle loop that handles:
    /// 1. Building containers
    /// 2. Launching containers
    /// 3. Monitoring for file changes
    /// 4. Handling shutdowns and rebuilds
    async fn launch_loop(&mut self) -> Result<()> {
        let mut should_continue = true;

        while should_continue {
            // Clean up any existing containers
            self.cleanup_containers().await?;

            // Build all containers
            if let Err(e) = self.build_all().await {
                error!("Build failed: {}", e);

                // Wait for file changes or manual termination
                match self.wait_for_changes_or_termination().await {
                    WaitResult::FileChanged => continue,
                    WaitResult::Terminated => break,
                    WaitResult::Timeout => continue,
                }
            }

            // Launch all containers
            self.launch_containers().await?;

            // Monitor containers and wait for changes
            should_continue = self.monitor_and_handle_events().await?;
        }

        info!("Container reactor shutting down");
        self.cleanup_containers().await?;
        Ok(())
    }

    /// Builds all container images
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    async fn build_all(&mut self) -> Result<()> {
        info!("Building container images");
        self.rebuild_in_progress = true;

        // Process all component specs to build their images
        for spec in &self.component_specs {
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
            let context_dir = match &spec.build_type {
                BuildType::TrunkWasm { context_dir, .. }
                | BuildType::RustBinary { context_dir, .. }
                | BuildType::DixiousWasm { context_dir, .. }
                | BuildType::Script { context_dir, .. }
                | BuildType::Zola { context_dir, .. }
                | BuildType::Book { context_dir, .. }
                | BuildType::Ingress { context_dir, .. } => {
                    context_dir.clone().unwrap_or_else(|| ".".to_string())
                }
                _ => continue,
            };

            // Create image name and tag
            let image_name = format!(
                "{}/{}-{}",
                self.config.docker_registry, self.config.product_name, component_name
            );
            let image_tag = &self.config.git_hash;

            // Set tagged image name in component spec
            let tagged_image_name = format!("{}:{}", image_name, image_tag);

            // Build any necessary scripts or templates first
            // This would typically be done by the build processor

            // Build the Docker image
            let dockerfile = Path::new(dockerfile_path);
            let context = Path::new(&context_dir);

            if let Err(e) = self
                .build_image(&image_name, image_tag, context, dockerfile)
                .await
            {
                error!("Failed to build image for {}: {}", component_name, e);
                self.rebuild_in_progress = false;
                return Err(e);
            }

            info!(
                "Successfully built image for {}: {}",
                component_name, tagged_image_name
            );
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
                    // Create Docker service config
                    let env_vars = HashMap::new();
                    // Add environment variables and secrets here

                    let config = DockerServiceConfig {
                        name: service.name.clone(),
                        // Use docker_host and git_hash for image name
                        image: service.image.clone(),
                        network: self.config.network_name.clone(),
                        env_vars,
                        ports: vec![format!("{}:{}", service.port, service.target_port)],
                        volumes: Vec::new(), // Add volumes as needed
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
                )
                .await?;

            // Create service object
            let service = DockerService::new(container_id, config, self.docker_client.clone());

            self.running_services.push(service);
        }

        info!(
            "Successfully launched {} containers",
            self.running_services.len()
        );
        Ok(())
    }

    /// Monitors running containers and handles events like file changes or termination signals
    ///
    /// # Returns
    ///
    /// Boolean indicating whether to continue running (true) or shut down (false)
    async fn monitor_and_handle_events(&mut self) -> Result<bool> {
        info!("Monitoring containers and watching for file changes");

        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        let mut file_check_interval = tokio::time::interval(Duration::from_millis(100));
        let mut status_check_interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            tokio::select! {
                _ = &mut ctrl_c => {
                    info!("Received termination signal");
                    return Ok(false);
                }

                _ = file_check_interval.tick() => {
                    if self.change_processor.process_pending_changes().await? {
                        info!("Detected file changes, triggering rebuild");
                        return Ok(true);
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

        // Stop and remove each service
        for service in &self.running_services {
            // Try to stop gracefully first
            if let Err(e) = service.stop().await {
                warn!("Error stopping container {}: {}", service.id(), e);
            }

            // Force remove container
            if let Err(e) = service.remove().await {
                warn!("Error removing container {}: {}", service.id(), e);
            }
        }

        // Clear the list of running services
        self.running_services.clear();

        Ok(())
    }

    /// Waits for file changes or a termination signal
    ///
    /// # Returns
    ///
    /// WaitResult indicating what happened
    async fn wait_for_changes_or_termination(&self) -> WaitResult {
        info!("Waiting for file changes or termination signal");

        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        let mut file_check_interval = tokio::time::interval(Duration::from_millis(100));
        let wait_timeout = tokio::time::sleep(Duration::from_secs(300)); // 5 minute timeout
        tokio::pin!(wait_timeout);

        loop {
            tokio::select! {
                _ = &mut ctrl_c => {
                    info!("Received termination signal while waiting");
                    return WaitResult::Terminated;
                }

                _ = file_check_interval.tick() => {
                    if self.change_processor.process_pending_changes().await.unwrap_or(false) {
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
    async fn build_image(
        &self,
        image_name: &str,
        image_tag: &str,
        context_dir: &Path,
        dockerfile: &Path,
    ) -> Result<()> {
        info!("Building image {}", image_name);

        let tag = format!("{}:{}", image_name, image_tag);

        // Create an ImageBuilder with the right configuration
        let service_config = DockerServiceConfig {
            name: image_name.to_string(),
            image: tag.clone(),
            network: self.config.network_name.clone(),
            env_vars: HashMap::new(),
            ports: Vec::new(),
            volumes: Vec::new(),
        };

        let service = DockerService::new(
            "".to_string(), // ID will be set when container launches
            service_config,
            self.docker_client.clone(),
        );

        let build_config = crate::container::BuildConfig {
            build_type: BuildType::RustBinary {
                location: "".to_string(),
                dockerfile_path: dockerfile.to_string_lossy().to_string(),
                context_dir: Some(context_dir.to_string_lossy().to_string()),
                features: None,
                precompile_commands: None,
            },
            dockerfile_path: Some(dockerfile.to_string_lossy().to_string()),
            context_dir: Some(context_dir.to_string_lossy().to_string()),
            docker_registry: self.config.docker_registry.clone(),
            environment: self.config.environment.clone(),
            domain: "".to_string(),
            mount_point: None,
        };

        let image_builder = crate::container::ImageBuilder::new(
            service,
            image_name.to_string(),
            self.config.product_name.clone(),
        )
        .with_build_config(build_config);

        // Build the image
        image_builder.build().await
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
        return Ok("latest".to_string());
    }

    Ok(hash)
}
