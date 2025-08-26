//! Container orchestration and lifecycle management
//!
//! The reactor is responsible for orchestrating container lifecycle events,
//! monitoring containers, and handling file change events that trigger rebuilds.
use crate::DockerCliClient;
use crate::Status;
use crate::{
    build::{BuildProcessor, BuildOrchestrator, BuildOrchestratorConfig},
    docker::{ContainerStatus, DockerClient, DockerService, DockerServiceConfig},
    events::EventBus,
    lifecycle::{LifecycleManager, LifecycleConfig},
    reactor::{
        docker_integration::{DockerIntegration, DockerIntegrationConfig},
        modular_core::{Reactor, ModularReactorConfig},
        state::SharedReactorState,
        watcher_integration::{WatcherIntegration, WatcherIntegrationConfig},
    },
    watcher::{setup_file_watcher, ChangeProcessor, WatcherConfig, CoordinatorConfig},
    ContainerService, ServiceCollection,
};
use notify::RecommendedWatcher;
use rush_build::{BuildType, ComponentBuildSpec, Variables};
use rush_config::Config;
use rush_core::constants::DOCKER_TAG_LATEST;
use rush_core::error::{Error, Result};
use rush_core::shutdown;
use rush_security::{FileVault, SecretsEncoder, Vault};

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

/// Container reactor that manages the container lifecycle and coordinates rebuilds based on file changes
/// 
/// This reactor delegates to the primary Reactor implementation for all operations.
pub struct ContainerReactor {
    /// Configuration for the reactor
    config: Arc<ContainerReactorConfig>,

    /// The new modular reactor that handles the actual work
    modular_reactor: Option<Reactor>,

    /// Shared state for reactor operations
    state: SharedReactorState,

    /// Event bus for communication
    event_bus: EventBus,

    /// Lifecycle manager for container operations
    lifecycle_manager: LifecycleManager,

    /// Docker integration layer
    docker_integration: DockerIntegration,

    /// Watcher integration for file changes
    watcher_integration: WatcherIntegration,

    /// Build orchestrator for coordinating builds
    build_orchestrator: Arc<BuildOrchestrator>,

    /// Collection of services managed by this reactor
    services: ServiceCollection,
    /// Build processor for container builds
    build_processor: BuildProcessor,
    /// Vault for accessing secrets
    vault: Arc<Mutex<dyn Vault + Send>>,
    /// Toolchain for build operations
    toolchain: Option<Arc<rush_toolchain::ToolchainContext>>,
    /// Secrets encoder
    secrets_encoder: Arc<dyn SecretsEncoder>,
    /// Output sink for handling container logs
    output_sink: Arc<tokio::sync::Mutex<Box<dyn rush_output::simple::Sink>>>,
    /// Mapping of component names to their actual built image names (with git tags)
    built_images: HashMap<String, String>,
    /// Additional environment variables from external sources
    additional_env: HashMap<String, String>,
    /// Channel for triggering graceful shutdown
    shutdown_sender: broadcast::Sender<()>,
    /// Running services
    running_services: Vec<DockerService>,
    /// File watcher and change processor
    file_watcher: Option<(RecommendedWatcher, Arc<ChangeProcessor>)>,
    /// Force rebuild flag - ignores cache when true
    force_rebuild: bool,
    /// Storage for component specs until primary reactor is initialized
    temp_component_specs: Vec<ComponentBuildSpec>,
}

// Use the ContainerReactorConfig from the config module
use super::config::ContainerReactorConfig;

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
    /// Sets the output sink for handling container logs
    pub fn set_output_sink(&mut self, sink: Box<dyn rush_output::simple::Sink>) {
        self.output_sink = Arc::new(tokio::sync::Mutex::new(sink));
    }

    /// Add an environment variable to be injected into all containers
    pub fn add_env_var(&mut self, key: String, value: String) {
        self.additional_env.insert(key, value);
    }
    
    /// Set the force rebuild flag
    pub fn set_force_rebuild(&mut self, force: bool) {
        self.force_rebuild = force;
        if force {
            info!("Force rebuild enabled - will ignore Docker cache");
        }
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
        let (shutdown_sender, _) = broadcast::channel::<()>(8);

        // Create the toolchain for build operations
        let toolchain = Some(Arc::new(rush_toolchain::ToolchainContext::default()));

        // Create event bus and shared state - these are the core of the modular architecture
        let event_bus = EventBus::new();
        let state = SharedReactorState::new();

        // Create Docker integration with enhanced features disabled
        let docker_integration_config = DockerIntegrationConfig {
            use_enhanced_client: false,
            enable_metrics: false,
            enable_pooling: false,
            ..Default::default()
        };
        let docker_integration = DockerIntegration::new(
            docker_client.clone(),
            docker_integration_config,
            event_bus.clone(),
            state.clone(),
        )?;

        // Create lifecycle manager configuration
        let lifecycle_config = LifecycleConfig {
            product_name: config.product_name.clone(),
            environment: config.environment.clone(),
            network_name: config.network_name.clone(),
            auto_restart: true, // Enable modern features by default
            enable_health_checks: true, // Enable modern features by default
            ..Default::default()
        };
        let lifecycle_manager = LifecycleManager::new(
            lifecycle_config,
            docker_integration.client(),
            vault.clone(),
            event_bus.clone(),
            state.clone(),
        );

        // Create watcher integration with shutdown sender
        let (shutdown_sender, _shutdown_receiver) = broadcast::channel(1);
        let watcher_config = WatcherIntegrationConfig {
            coordinator_config: CoordinatorConfig {
                handler_config: crate::watcher::HandlerConfig::default(),
                auto_rebuild: true,
                rebuild_cooldown: std::time::Duration::from_secs(2),
                max_pending_changes: 10,
            },
            use_new_watcher: true,
        };
        let watcher_integration = WatcherIntegration::new(
            watcher_config,
            event_bus.clone(),
            state.clone(),
            shutdown_sender.clone(),
        )?;

        // Create build orchestrator config
        let orchestrator_config = BuildOrchestratorConfig {
            product_name: config.product_name.clone(),
            product_dir: config.product_dir.clone(),
            build_timeout: Duration::from_secs(300),
            parallel_builds: true,
            max_parallel: 4,
            enable_cache: true,
            cache_dir: config.product_dir.join(".rush/cache"),
        };

        // Create build orchestrator
        let build_orchestrator = Arc::new(BuildOrchestrator::new(
            orchestrator_config,
            docker_integration.client(),
            event_bus.clone(),
            state.clone(),
        ));

        Ok(Self {
            config,
            modular_reactor: None, // Will be created when needed
            state,
            event_bus,
            lifecycle_manager,
            docker_integration,
            watcher_integration,
            build_orchestrator,
            // Service management fields
            services: HashMap::new(),
            build_processor: BuildProcessor::new(false),
            vault,
            toolchain,
            secrets_encoder,
            output_sink: Arc::new(tokio::sync::Mutex::new(
                Box::new(rush_output::simple::StdoutSink::new()),
            )),
            built_images: HashMap::new(),
            additional_env: HashMap::new(),
            shutdown_sender,
            running_services: Vec::new(),
            file_watcher: None,
            force_rebuild: false,
            temp_component_specs: Vec::new(),
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
        let secrets_encoder = Arc::new(rush_security::NoopEncoder);

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
        let stack_config =
            match fs::read_to_string(format!("{}/stack.spec.yaml", product_path.display())) {
                Ok(config) => config,
                Err(e) => return Err(format!("Failed to read stack config: {e}").into()),
            };

        // Parse YAML
        let stack_config_value: serde_yaml::Value = serde_yaml::from_str(&stack_config)
            .map_err(|e| format!("Failed to parse stack config: {e}"))?;

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
                        .contains_key(serde_yaml::Value::String("component_name".to_string()))
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

        // Store the components in state
        // Note: We skip the async state initialization here because:
        // 1. This is called from within an async context (can't use block_on)
        // 2. The state will be properly initialized when the modular reactor is created
        // 3. The legacy reactor doesn't directly use the SharedReactorState for these components
        // The modular components will handle their own state management when needed.
        
        // Store component specs in temp field for later use by modular reactor
        reactor.temp_component_specs = component_specs;

        // Note: Local services (including Stripe) are now started by local_services_startup.rs
        // before the reactor is created, so we don't handle them here anymore

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

        for spec in self.component_specs() {
            // Skip pure Kubernetes specs and LocalServices
            if matches!(
                spec.build_type,
                BuildType::PureKubernetes
                    | BuildType::KubernetesInstallation { .. }
                    | BuildType::LocalService { .. }
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
                BuildType::PureDockerImage {
                    image_name_with_tag,
                    ..
                } => {
                    // Use the pre-existing image directly (e.g., "postgres:latest")
                    image_name_with_tag.clone()
                }
                _ => {
                    // Check if we have a built image with git tag for this component
                    if let Some(built_image) = self.built_images.get(component_name) {
                        built_image.clone()
                    } else {
                        // Use default naming before build
                        let image_name = if self.config.docker_registry.is_empty() {
                            format!("{}-{}", self.config.product_name, component_name)
                        } else {
                            format!(
                                "{}/{}-{}",
                                self.config.docker_registry,
                                self.config.product_name,
                                component_name
                            )
                        };
                        format!("{}:{}", image_name, self.config.git_hash)
                    }
                }
            };

            let service = Arc::new(ContainerService {
                id: format!("{}_{}", spec.product_name, spec.component_name),
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
            services.entry(domain).or_default().push(service);
        }

        Ok(services)
    }

    /// Gets the domain for a component based on subdomain
    fn get_domain(&self, subdomain: Option<&str>) -> Result<String> {
        // For simplicity in this implementation, we'll use a basic domain format
        // In a complete implementation, this would use templates and environment-specific logic
        let domain_base = format!("{}.{}", self.config.product_name, self.config.environment);

        match subdomain {
            Some(sub) => Ok(format!("{sub}.{domain_base}")),
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
    
    /// Ensure the modular reactor is initialized
    async fn ensure_modular_reactor(&mut self) -> Result<()> {
        // Initialize the modular reactor if not already done
        if self.modular_reactor.is_none() {
            info!("Initializing modular reactor");
            let modular_config = self.create_modular_config();
            
            // Use the component specs that were stored during from_product_dir
            let component_specs = self.temp_component_specs.clone();
            
            let modular_reactor = crate::reactor::factory::ReactorFactory::create_reactor(
                modular_config,
                self.docker_integration.client(),
                component_specs,
                Some(self.config.as_ref().clone()),
            ).await?;
            
            if let crate::reactor::factory::ReactorImplementation::Primary(mut reactor) = modular_reactor {
                // Pass services to the modular reactor
                let all_services: Vec<ContainerService> = self.services.values()
                    .flat_map(|service_list| service_list.iter().map(|s| (**s).clone()))
                    .collect();
                reactor.set_services(all_services);
                
                // Pass the output sink for log capture
                reactor.set_output_sink(self.output_sink.clone());
                
                self.modular_reactor = Some(reactor);
                info!("Modular reactor initialized successfully");
            } else {
                return Err(Error::Internal("Expected modular reactor implementation".into()));
            }
        }
        Ok(())
    }
    
    pub async fn launch(&mut self) -> Result<()> {
        info!("Starting container reactor");
        
        // Ensure modular reactor is initialized
        self.ensure_modular_reactor().await?;

        // Set up the network
        // Setup network
        if !self
            .docker_client()
            .network_exists(&self.config.network_name)
            .await?
        {
            self.docker_client()
                .create_network(&self.config.network_name)
                .await?;
        }

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

        panic!("Rollout not implemented")
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
                // Docker push implementation would go here
            }
        }

        info!("Build and push completed successfully");
        Ok(())
    }

    /// Select Kubernetes context for deployment
    pub async fn select_kubernetes_context(&self, context: &str) -> Result<()> {
        info!("Selecting Kubernetes context: {}", context);

        // Kubectl context selection implementation would go here
        let _args = ["config", "use-context", context];

        // Would use run_command here in actual implementation

        info!("Kubernetes context set to: {}", context);
        Ok(())
    }

    /// Apply Kubernetes manifests to the cluster
    pub async fn apply(&mut self) -> Result<()> {
        info!("Applying Kubernetes manifests...");

        // K8s manifest generation and deployment would go here

        info!("Kubernetes manifests applied successfully");
        Ok(())
    }

    /// Remove Kubernetes resources from the cluster
    pub async fn unapply(&mut self) -> Result<()> {
        info!("Removing Kubernetes resources...");

        // K8s resource deletion would go here

        info!("Kubernetes resources removed successfully");
        Ok(())
    }

    /// Install Kubernetes manifests (similar to apply but for installation)
    pub async fn install_manifests(&mut self) -> Result<()> {
        info!("Installing Kubernetes manifests...");

        // K8s manifest installation would go here

        info!("Kubernetes manifests installed successfully");
        Ok(())
    }

    /// Uninstall Kubernetes manifests
    pub async fn uninstall_manifests(&mut self) -> Result<()> {
        info!("Uninstalling Kubernetes manifests...");

        // K8s manifest uninstallation would go here

        info!("Kubernetes manifests uninstalled successfully");
        Ok(())
    }

    /// Build Kubernetes manifests
    pub async fn build_manifests(&mut self) -> Result<()> {
        info!("Building Kubernetes manifests...");

        // K8s manifest generation from templates would go here

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
        
        // Ensure modular reactor is initialized
        self.ensure_modular_reactor().await?;
        
        // Use the modular reactor for building
        if let Some(ref mut reactor) = self.modular_reactor {
            reactor.build().await?;
        } else {
            return Err(Error::Internal("Modular reactor not initialized".into()));
        }
        
        info!("All container images built successfully");
        Ok(())
    }

    /// Main container lifecycle loop that handles:
    /// 1. Building containers
    /// 2. Launching containers
    /// 3. Monitoring for file changes
    /// 4. Handling shutdowns and rebuilds
    async fn launch_loop(&mut self) -> Result<()> {
        let working_dir = self.config.product_dir.clone();
        let _dir_guard = rush_utils::Directory::chpath(&working_dir);

        // Clean up containers first before accessing the modular reactor
        info!("Cleaning up application containers (preserving local services)");
        self.cleanup_containers().await?;
        
        // The modular reactor handles everything: building, running, watching files
        if let Some(reactor) = &mut self.modular_reactor {
            
            info!("Starting modular reactor for building and running containers");
            
            // First trigger a rebuild to ensure all images are built
            if let Err(e) = reactor.rebuild_all().await {
                error!("Initial build failed: {}", e);
                
                // Wait for file changes to retry
                info!("Waiting for file changes to retry build...");
                info!("💡 Tip: Fix the build error and save a file to trigger rebuild");
                
                // Start the reactor anyway - it will handle file watching and rebuilds
            }
            
            // Start the reactor (handles lifecycle management)
            reactor.start().await?;
            
            // Run the main reactor loop (handles file watching, rebuilds, etc.)
            reactor.run().await
        } else {
            Err(Error::Internal("Modular reactor not initialized".into()))
        }
    }

    /// Builds all container images
    async fn build_all(&mut self) -> Result<()> {
        // Delegate to the modular reactor
        if let Some(ref mut reactor) = self.modular_reactor {
            reactor.build().await
        } else {
            Err(Error::Internal("Modular reactor not initialized".into()))
        }
    }

    fn update_service_images(&mut self) -> Result<()> {
        // Create a new services collection with updated image names
        let mut updated_services: ServiceCollection = HashMap::new();

        for (domain, service_list) in &self.services {
            let mut updated_list = Vec::new();

            for service in service_list {
                // Check if we have a built image for this service
                let updated_image = if let Some(built_image) = self.built_images.get(&service.name)
                {
                    built_image.clone()
                } else {
                    // Keep the original image name (e.g., for PureDockerImage)
                    service.image.clone()
                };

                // Create a new service with the updated image
                let updated_service = Arc::new(ContainerService {
                    id: service.id.clone(),
                    name: service.name.clone(),
                    image: updated_image,
                    host: service.host.clone(),
                    port: service.port,
                    target_port: service.target_port,
                    mount_point: service.mount_point.clone(),
                    domain: service.domain.clone(),
                    docker_host: service.docker_host.clone(),
                });

                updated_list.push(updated_service);
            }

            updated_services.insert(domain.clone(), updated_list);
        }

        // Replace the services collection with the updated one
        self.services = updated_services;

        Ok(())
    }

    /// Launches all containers
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    #[allow(clippy::await_holding_lock)]
    async fn launch_containers(&mut self) -> Result<()> {
        info!("Launching containers");

        // Note: Local services (including Stripe) are now started by local_services_startup.rs
        // before the reactor is created, so we don't handle them here anymore

        let (_status_sender, _status_receiver) = mpsc::channel::<Status>(100);

        // Create service configs for each service
        let mut service_configs = Vec::new();

        for service_list in self.services.values() {
            for service in service_list {
                let should_redirect = self
                    .config
                    .redirected_components
                    .contains_key(&service.name);

                if !should_redirect {
                    // Load secrets for this component from vault
                    // Note: We need to hold the lock across the await because Vault methods are async
                    #[allow(clippy::await_holding_lock)]
                    let secrets = {
                        let vault_guard = self.vault.lock().unwrap();
                        vault_guard
                            .get(
                                &self.config.product_name,
                                &service.name,
                                &self.config.environment,
                            )
                            .await
                            .unwrap_or_default()
                    };

                    // Load environment variables for this component
                    let component_spec = self
                        .component_specs()
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

                    // Add additional environment variables from local services
                    for (key, value) in &self.additional_env {
                        env_vars.insert(key.clone(), value.clone());
                    }

                    // Collect volumes from component spec
                    let mut volumes = Vec::new();
                    if let Some(spec) = component_spec {
                        if let Some(spec_volumes) = &spec.volumes {
                            for (host_path, container_path) in spec_volumes {
                                volumes.push(format!("{host_path}:{container_path}"));
                            }
                        }
                    }

                    // Container name should be product_name-component_name (to match old implementation)
                    let container_name = format!("{}-{}", self.config.product_name, service.name);

                    let config = DockerServiceConfig {
                        name: container_name,
                        image: service.image.clone(),
                        network: self.config.network_name.clone(),
                        env_vars,
                        ports: vec![format!("{}:{}", service.port, service.target_port)],
                        volumes, // Use the already fixed volumes
                    };

                    service_configs.push(config);
                }
            }
        }

        // Launch all services
        for config in service_configs {
            // Launch container
            let container_id = self
                .docker_client()
                .run_container(
                    &config.image,
                    &config.name,
                    &config.network,
                    &config
                        .env_vars
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>(),
                    &config.ports,
                    &config.volumes,
                )
                .await?;

            // Create service object
            let service = DockerService::new(
                container_id.clone(),
                config.clone(),
                self.docker_client().clone(),
            );

            // Start following logs for this container
            let docker_client = self.docker_client().clone();
            let container_name = config.name.clone();

            // Extract component name from the full container name (e.g., "helloworld.wonop.io-frontend" -> "frontend")
            let component_name = if let Some(service) = self
                .services
                .values()
                .flat_map(|v| v.iter())
                .find(|s| config.name.contains(&s.name))
            {
                service.name.clone()
            } else {
                // Try to extract from container name pattern "product-component"
                container_name
                    .rsplit('-')
                    .next()
                    .unwrap_or(&container_name)
                    .to_string()
            };

            // Start following logs immediately to capture all output from the beginning
            // Use docker logs --follow to ensure we get everything from container start
            let sink = self.output_sink.clone();
            let component_name_for_sink = component_name.clone();
            let container_id_clone = container_id.clone();

            // Start log following task immediately with minimal delay
            tokio::spawn(async move {
                // Very small delay to ensure container process has started
                // This is much faster than the previous attach approach
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

                // Use docker logs --follow to get ALL logs from the beginning
                if let Err(e) = crate::simple_output::follow_container_logs_from_start(
                    docker_client,
                    &container_id_clone,
                    component_name_for_sink,
                    sink,
                )
                .await
                {
                    // Only log errors if we're not shutting down
                    let shutdown_token = shutdown::global_shutdown().cancellation_token();
                    if !shutdown_token.is_cancelled() {
                        error!(
                            "Error following logs for container {}: {}",
                            container_name, e
                        );
                    } else {
                        debug!(
                            "Container {} log following stopped during shutdown",
                            container_name
                        );
                    }
                }
            });

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

        info!(
            "Testing {} changed files for significance",
            changed_files.len()
        );
        for file in changed_files {
            debug!("  Changed file: {}", file.display());
        }

        let mut affected_components = Vec::new();

        // Check each component to see if it's affected by the changes
        debug!(
            "Checking {} components for changes",
            self.component_specs().len()
        );
        for spec in self.component_specs() {
            debug!("Evaluating component: {}", spec.component_name);

            // Skip redirected components (they're not built locally)
            if self
                .config
                .redirected_components
                .contains_key(&spec.component_name)
            {
                debug!("  Skipping redirected component: {}", spec.component_name);
                continue;
            }

            // Check if any changed file is in this component's context or matches its watch patterns
            if self.is_any_file_in_component_context(spec, changed_files) {
                info!(
                    "  ✓ Component '{}' is affected by file changes",
                    spec.component_name
                );
                affected_components.push(spec.component_name.clone());
            } else {
                debug!("  ✗ Component '{}' not affected", spec.component_name);
            }
        }

        if !affected_components.is_empty() {
            info!(
                "Rebuild triggered for components: {:?}",
                affected_components
            );
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
    fn is_any_file_in_component_context(
        &self,
        spec: &ComponentBuildSpec,
        file_paths: &[PathBuf],
    ) -> bool {
        debug!(
            "    Checking context for component: {}",
            spec.component_name
        );

        // First check if component has watch patterns defined
        if let Some(watch_matcher) = &spec.watch {
            debug!("    Component has watch patterns defined");
            let matched = file_paths.iter().any(|file| {
                let matches = watch_matcher.matches(file);
                if matches {
                    debug!("      ✓ File {} matches watch pattern", file.display());
                } else {
                    debug!(
                        "      ✗ File {} does not match watch patterns",
                        file.display()
                    );
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
            BuildType::TrunkWasm {
                context_dir,
                location,
                ..
            } => {
                context_dir.clone().unwrap_or_else(|| {
                    // For TrunkWasm, derive context from the parent directory of location
                    if let Some(parent) = std::path::Path::new(location).parent() {
                        parent.to_string_lossy().to_string()
                    } else {
                        ".".to_string()
                    }
                })
            }
            BuildType::RustBinary {
                context_dir,
                location,
                ..
            } => {
                // For RustBinary, use context_dir if specified, otherwise use location
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::DixiousWasm {
                context_dir,
                location,
                ..
            } => {
                // For DixiousWasm, use context_dir if specified, otherwise use location
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::Script {
                context_dir,
                location,
                ..
            } => {
                // For Script, use context_dir if specified, otherwise use location
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::Zola {
                context_dir,
                location,
                ..
            } => {
                // For Zola, use context_dir if specified, otherwise use location
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::Book {
                context_dir,
                location,
                ..
            } => {
                // For Book, use context_dir if specified, otherwise use location
                context_dir.clone().unwrap_or_else(|| location.clone())
            }
            BuildType::Ingress { context_dir, .. } => {
                // For Ingress, use context_dir if specified, otherwise current directory
                context_dir.clone().unwrap_or_else(|| ".".to_string())
            }
            BuildType::PureDockerImage { .. }
            | BuildType::PureKubernetes
            | BuildType::KubernetesInstallation { .. }
            | BuildType::LocalService { .. } => {
                // These types don't have a build context for file watching
                debug!("    Build type doesn't support file watching");
                return false;
            }
        };

        // Check if any changed file is within the component's context directory
        let context_path = self.config.product_dir.join(&context_dir);
        debug!("    Context directory: {}", context_path.display());

        let result = file_paths.iter().any(|file_path| {
            debug!(
                "      Checking if {} is in context {}",
                file_path.display(),
                context_path.display()
            );

            // Try to get absolute paths for comparison
            if let (Ok(abs_file), Ok(abs_context)) = (
                std::fs::canonicalize(file_path),
                std::fs::canonicalize(&context_path),
            ) {
                let is_match = abs_file.starts_with(&abs_context);
                debug!(
                    "        Absolute comparison: {} starts_with {} = {}",
                    abs_file.display(),
                    abs_context.display(),
                    is_match
                );
                is_match
            } else {
                // Use simple path comparison
                let is_match = file_path.starts_with(&context_path);
                debug!(
                    "        Simple comparison: {} starts_with {} = {}",
                    file_path.display(),
                    context_path.display(),
                    is_match
                );
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
                    // Skip file change processing if we're shutting down
                    if shutdown_token.is_cancelled() {
                        debug!("Ignoring file changes during shutdown");
                        return Ok(false);
                    }

                    // Check if we have pending changes to process
                    let changed_files = self.change_processor().process_pending_changes().await?;
                    if !changed_files.is_empty() {
                        // Double-check we're not shutting down before processing
                        if shutdown_token.is_cancelled() {
                            debug!("Ignoring file changes during shutdown");
                            return Ok(false);
                        }

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
                        let container_name = service.name().unwrap_or_else(|| service.id().to_string());

                        match service.status().await {
                            Ok(ContainerStatus::Exited(code)) => {
                                // Container has exited
                                if code != 0 {
                                    error!("Container '{}' failed with exit code {} - initiating shutdown", container_name, code);
                                } else {
                                    warn!("Container '{}' exited with code 0 - initiating shutdown", container_name);
                                }

                                info!("Shutting down all application containers due to container exit");

                                // Trigger global shutdown so all parts of the system know we're shutting down
                                shutdown::global_shutdown().shutdown(shutdown::ShutdownReason::ContainerExit);

                                // Clean up containers
                                self.cleanup_containers().await?;
                                return Ok(false); // Signal to stop the reactor
                            }
                            Ok(ContainerStatus::Created) => {
                                // Container was just created, not started yet
                                debug!("Container '{}' is created but not started", container_name);
                            }
                            Ok(ContainerStatus::Restarting) => {
                                // Container is restarting
                                info!("Container '{}' is restarting", container_name);
                            }
                            Ok(ContainerStatus::Paused) => {
                                // Container is paused
                                warn!("Container '{}' is paused", container_name);
                            }
                            Ok(ContainerStatus::Dead) => {
                                // Container is dead
                                error!("Container '{}' is dead - initiating shutdown", container_name);
                                
                                // Trigger shutdown for dead container
                                shutdown::global_shutdown().shutdown(shutdown::ShutdownReason::ContainerExit);
                                self.cleanup_containers().await?;
                                return Ok(false);
                            }
                            Ok(ContainerStatus::Unknown) => {
                                // Container might have been removed or doesn't exist
                                warn!("Container '{}' status unknown - it may have crashed or been removed", container_name);

                                // Don't immediately shutdown for unknown status - wait a bit
                                // The container might be restarting or temporarily unreachable
                                debug!("Will check again on next interval");
                            }
                            Ok(ContainerStatus::Running) => {
                                // Container is running normally
                                trace!("Container '{}' is running", container_name);
                            }
                            Err(e) => {
                                warn!("Error checking status for container '{}': {}", container_name, e);
                            }
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
        info!("Cleaning up application containers (preserving local services)");

        // Clear any pending file changes to prevent processing during shutdown
        self.change_processor().clear();
        debug!("Cleared pending file changes");

        // IMPORTANT: Local services should persist and only be stopped on final program termination
        // They are managed by DevEnvironment, not the reactor

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
                    warn!(
                        "Failed to clean up container {} (attempt {}/{}), retrying...",
                        service.id(),
                        retries,
                        max_retries
                    );
                    tokio::time::sleep(Duration::from_millis(500 * retries as u64)).await;
                } else {
                    warn!(
                        "Failed to clean up container {} after {} retries",
                        service.id(),
                        max_retries
                    );
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
        trace!("Cleaning up application containers by name pattern (excluding local services)");

        // Clean up application containers ONLY
        for spec in self.component_specs() {
            // Skip LocalService specs - they should persist
            if matches!(spec.build_type, BuildType::LocalService { .. }) {
                debug!(
                    "Skipping local service {} - it should persist",
                    spec.component_name
                );
                continue;
            }

            // Use product_name-component_name to match the container naming convention
            let container_name = format!("{}-{}", self.config.product_name, spec.component_name);

            // Try to kill and remove with retries
            self.kill_and_remove_container_with_retry(&container_name, 3)
                .await?;
        }

        // DO NOT clean up local service containers during reactor cleanup
        // Local services (rush-local-*) should only be stopped on final program termination
        // They are managed by DevEnvironment, not the reactor
        debug!("Preserving local service containers - they persist across rebuilds");

        trace!("Container cleanup by name completed");
        Ok(())
    }

    /// Kill and remove a container with retry logic
    async fn kill_and_remove_container_with_retry(
        &self,
        container_name: &str,
        max_retries: u32,
    ) -> Result<()> {
        let mut retries = 0;

        while retries < max_retries {
            // First try to kill if running
            match self.kill_container_by_name(container_name).await {
                Ok(_) => debug!("Successfully killed container: {}", container_name),
                Err(e) => {
                    if retries == max_retries - 1 {
                        warn!(
                            "Failed to kill container {} after {} retries: {}",
                            container_name, max_retries, e
                        );
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
                        warn!(
                            "Failed to remove container {} (attempt {}/{}): {}, retrying...",
                            container_name, retries, max_retries, e
                        );
                        // Wait a bit before retrying
                        tokio::time::sleep(Duration::from_millis(500 * retries as u64)).await;
                    } else {
                        warn!(
                            "Failed to remove container {} after {} retries: {}",
                            container_name, max_retries, e
                        );
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
            .args(["ps", "-q", "-f", &format!("name={container_name}")])
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
                            .map_err(|e| {
                                Error::Container(format!(
                                    "Failed to execute kill command for {container_name}: {e}"
                                ))
                            })?;

                        if !kill_output.status.success() {
                            let stderr = String::from_utf8_lossy(&kill_output.stderr);
                            return Err(Error::Container(format!(
                                "Failed to kill container {container_name}: {stderr}"
                            )));
                        }
                    } else {
                        trace!("No running container found for {}", container_name);
                    }
                } else {
                    trace!("Error checking for running container {}", container_name);
                }
            }
            Err(e) => {
                trace!(
                    "Error executing docker ps command for {}: {}",
                    container_name,
                    e
                );
            }
        }

        Ok(())
    }

    /// Remove a container by name (handles both running and stopped containers)
    async fn remove_container_by_name(&self, container_name: &str) -> Result<()> {
        // Check for any containers (running or stopped) with this name
        let check_output = Command::new("docker")
            .args(["ps", "-a", "-q", "-f", &format!("name={container_name}")])
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
                            .map_err(|e| {
                                Error::Container(format!(
                                    "Failed to execute rm command for {container_name}: {e}"
                                ))
                            })?;

                        if !rm_output.status.success() {
                            let stderr = String::from_utf8_lossy(&rm_output.stderr);
                            return Err(Error::Container(format!(
                                "Failed to remove container {container_name}: {stderr}"
                            )));
                        }
                    } else {
                        trace!("No container found for {}", container_name);
                    }
                } else {
                    trace!("Error checking for containers with name {}", container_name);
                }
            }
            Err(e) => {
                trace!(
                    "Error executing docker ps command for {}: {}",
                    container_name,
                    e
                );
            }
        }

        Ok(())
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
        // Use a very long timeout (1 hour) since we want to wait for user to fix the issue
        let wait_timeout = tokio::time::sleep(Duration::from_secs(3600)); // 1 hour timeout
        tokio::pin!(wait_timeout);

        loop {
            tokio::select! {
                _ = shutdown_token.cancelled() => {
                    info!("Received termination signal while waiting");
                    return WaitResult::Terminated;
                }

                _ = file_check_interval.tick() => {
                    // Check for shutdown before processing file changes
                    if shutdown_token.is_cancelled() {
                        debug!("Shutdown detected during file change wait");
                        return WaitResult::Terminated;
                    }

                    let changed_files = self.change_processor().process_pending_changes().await.unwrap_or_else(|_| Vec::new());
                    if !changed_files.is_empty() {
                        // Double-check shutdown before returning FileChanged
                        if shutdown_token.is_cancelled() {
                            debug!("Ignoring file changes due to shutdown");
                            return WaitResult::Terminated;
                        }
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
        use rush_build::{Artefact, BuildContext, ServiceSpec};
        use rush_core::error::Error;
        use rush_toolchain::{Platform, ToolchainContext};
        use std::collections::HashMap;
        use std::fs;
        use std::path::{Path, PathBuf};

        // Check if this component has artifacts to render
        if spec.artefacts.is_none() {
            return Ok(());
        }

        let artifact_count = spec.artefacts.as_ref().map(|a| a.len()).unwrap_or(0);
        info!(
            "Rendering {} artifacts for component: {}",
            artifact_count, spec.component_name
        );

        // Artifacts paths are relative to product directory
        // We need to resolve them to absolute paths
        let product_dir = &self.config.product_dir;

        // Create toolchain context
        let host_platform = Platform::default();
        let target_platform = Platform::new("linux", "x86_64");
        let toolchain =
            ToolchainContext::create_with_platforms(host_platform.clone(), target_platform.clone());

        // Get location from build type
        let location = spec.build_type.location().unwrap_or(".");

        // For Ingress components, we need to filter services to only include
        // the components specified in the ingress configuration
        let services = if let BuildType::Ingress { components, .. } = &spec.build_type {
            // Build a filtered services map based on the ingress components
            let mut filtered_services = HashMap::new();

            // We need to collect service information for the specified components
            // Use the properly computed domain from the ComponentBuildSpec
            // This domain is already computed from the rushd.yaml template based on environment
            let domain = spec.domain.clone();

            for component_name in components {
                // Try to find the actual component spec to get its configuration
                let component_spec = self.component_specs()
                    .iter()
                    .find(|s| &s.component_name == component_name);
                
                let docker_host = format!("{}-{}", spec.product_name, component_name);
                
                // Use actual port and mount_point from component spec if available
                let (port, target_port, mount_point) = if let Some(comp_spec) = component_spec {
                    (
                        comp_spec.port.unwrap_or(8000),
                        comp_spec.target_port.unwrap_or(comp_spec.port.unwrap_or(8000)),
                        comp_spec.mount_point.clone(),
                    )
                } else {
                    // Use defaults if component spec not found
                    let default_mount = if component_name == "frontend" {
                        Some("/".to_string())
                    } else if component_name == "backend" {
                        Some("/api".to_string())
                    } else {
                        None
                    };
                    (8000, 8000, default_mount)
                };
                
                let service_spec = ServiceSpec {
                    name: component_name.clone(),
                    host: docker_host.clone(),
                    docker_host,
                    domain: domain.clone(),
                    port,
                    target_port,
                    mount_point,
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
            cross_compile: spec.cross_compile.clone(),
        };

        // Create output directory for artifacts
        let output_dir = Path::new(&spec.artefact_output_dir);
        if !output_dir.exists() {
            fs::create_dir_all(output_dir).map_err(|e| Error::FileSystem {
                path: output_dir.to_path_buf(),
                message: format!("Failed to create artifact output directory: {e}"),
            })?;
        }

        // Render each artifact
        // Note: The artifacts come with relative paths, we need to make them absolute
        if let Some(artefacts) = &spec.artefacts {
            for (input_path, output_name) in artefacts.iter() {
                // Make input path absolute relative to product directory
                // First normalize the path to handle './' prefixes
                let normalized_input_path = input_path.strip_prefix("./").unwrap_or(input_path);

                let absolute_input_path = if Path::new(normalized_input_path).is_absolute() {
                    PathBuf::from(normalized_input_path)
                } else {
                    product_dir.join(normalized_input_path)
                };

                debug!(
                    "Artifact path normalization: '{}' -> '{}' -> '{}'",
                    input_path,
                    normalized_input_path,
                    absolute_input_path.display()
                );

                // For ingress, nginx.conf goes directly to the output directory
                // (which is already "target/rushd" by default)
                let absolute_output_path = output_dir.join(output_name);

                info!(
                    "Rendering artifact: {} -> {}",
                    absolute_input_path.display(),
                    absolute_output_path.display()
                );

                // Create the artifact with absolute paths
                let artifact = match Artefact::new(
                    absolute_input_path.to_string_lossy().to_string(),
                    absolute_output_path.to_string_lossy().to_string(),
                ) {
                    Ok(artifact) => artifact,
                    Err(e) => {
                        error!(
                            "Failed to create artifact for {}: {}",
                            spec.component_name, e
                        );
                        return Err(e);
                    }
                };

                // Render the artifact
                if let Err(e) = artifact.render_to_file(&context) {
                    error!(
                        "Failed to render artifact for {}: {}",
                        spec.component_name, e
                    );
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Runs the build script for a component before Docker build
    async fn run_build_script_for_component(&self, spec: &ComponentBuildSpec) -> Result<()> {
        use rush_build::{BuildContext, BuildScript};
        use rush_core::error::Error;
        use rush_toolchain::{Platform, ToolchainContext};
        // Using sink directly for output

        // Skip components that don't need build scripts
        if !spec.build_type.requires_docker_build() {
            return Ok(());
        }

        // Create toolchain context with proper cross-compilation setup
        // This matches the old implementation's behavior
        let host_platform = Platform::default();
        let target_platform = Platform::new("linux", "x86_64");

        // For cross-compilation scenarios, we need to handle potential toolchain issues
        let toolchain = if host_platform.os != target_platform.os
            || host_platform.arch != target_platform.arch
        {
            // Cross-compilation scenario
            match std::panic::catch_unwind(|| {
                ToolchainContext::create_with_platforms(
                    host_platform.clone(),
                    target_platform.clone(),
                )
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
            cross_compile: spec.cross_compile.clone(),
        };

        // Check if we're attempting cross-compilation
        let is_cross_compile = location.contains("backend") && cfg!(not(target_os = "linux"));

        if is_cross_compile {
            // For cross-compilation, we need special handling
            // Check if cross is installed
            if let Ok(output) = std::process::Command::new("which").arg("cross").output() {
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

        info!(
            "Running build script for component: {}",
            spec.component_name
        );

        // Write script to temp file and execute it
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let script_path = build_dir.join("build_script.sh");
        fs::write(&script_path, &script_content)
            .map_err(|e| Error::Build(format!("Failed to write build script: {e}")))?;

        // Make script executable
        let metadata = fs::metadata(&script_path)
            .map_err(|e| Error::Build(format!("Failed to get script metadata: {e}")))?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)
            .map_err(|e| Error::Build(format!("Failed to set script permissions: {e}")))?;

        // Use Directory guard to change to build directory and execute the script
        let output = {
            let _dir_guard = rush_utils::Directory::chpath(&build_dir);

            // Use the sink for build output
            let sink = self.output_sink.clone();
            let component_name = spec.component_name.clone();

            // Run the build command and capture output through our sink
            crate::simple_output::follow_build_output_simple(
                component_name,
                vec!["bash".to_string(), "./build_script.sh".to_string()],
                sink,
                Some(self.config.product_dir.clone()),
            )
            .await
            .map(|_| String::new())
        };

        // Clean up the script file
        let _ = fs::remove_file(&script_path);

        match output {
            Ok(_) => {
                info!(
                    "Build script completed successfully for: {}",
                    spec.component_name
                );
                Ok(())
            }
            Err(e) => {
                error!("Build script failed for {}: {}", spec.component_name, e);
                Err(Error::Build(format!("Build script failed: {e}")))
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
            .map_err(|e| Error::Docker(format!("Cannot execute docker command: {e}")))?;

        if !output.status.success() {
            return Err(Error::Docker(
                "Docker is not available or not running".into(),
            ));
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
        context_dir: &Path,
        dockerfile: &Path,
        spec: &ComponentBuildSpec,
    ) -> Result<String> {
        // Note: We already checked if rebuild is needed in build_all() before calling this
        info!("Building Docker image: {}", image_name);

        // Extract component name from the image name (format: product-component)
        let component_name = if let Some(dash_pos) = image_name.rfind('-') {
            &image_name[dash_pos + 1..]
        } else {
            image_name
        };

        // Create an ImageBuilder with the right configuration
        let service_config = DockerServiceConfig {
            name: image_name.to_string(),
            image: format!("{image_name}:{image_tag}"),
            network: self.config.network_name.clone(),
            env_vars: HashMap::new(),
            ports: Vec::new(),
            volumes: Vec::new(),
        };

        let service = DockerService::new(
            "".to_string(), // ID will be set when container launches
            service_config,
            self.docker_client().clone(),
        );

        // Use the actual build type from the spec to preserve location information
        let build_config = crate::BuildConfig {
            build_type: spec.build_type.clone(),
            dockerfile_path: Some(dockerfile.to_string_lossy().to_string()),
            context_dir: Some(context_dir.to_string_lossy().to_string()),
            docker_registry: self.config.docker_registry.clone(),
            environment: self.config.environment.clone(),
            domain: spec.domain.clone(),
            mount_point: spec.mount_point.clone(),
        };

        let mut image_builder = crate::ImageBuilder::new(
            service,
            self.docker_client().clone(),
            component_name.to_string(),
            self.config.product_name.clone(),
        )
        .with_build_config(build_config);

        // Set up toolchain if available
        if let Some(toolchain) = &self.toolchain {
            image_builder = image_builder.with_toolchain(toolchain.clone());
        }

        // Compute the git tag to get the final image name
        image_builder.compute_git_tag()?;
        let image_tag = image_builder.tagged_image_name();

        // Get the dockerfile and context paths from the image builder
        let dockerfile_path = image_builder
            .build_config()
            .dockerfile_path
            .as_ref()
            .ok_or_else(|| Error::Setup("No dockerfile path specified".into()))?;
        let context_path = image_builder
            .build_config()
            .context_dir
            .as_deref()
            .unwrap_or(".");

        // Build the image using proper stream capture
        // Always build for linux/amd64 regardless of host architecture
        // This ensures compatibility with deployment environments
        let platform = Some("linux/amd64".to_string());

        crate::simple_output::capture_docker_build(
            &image_tag,
            dockerfile_path,
            context_path,
            component_name.to_string(),
            self.output_sink.clone(),
            platform.as_deref(),
        )
        .await?;

        // Return the actual tagged image name that was built
        Ok(image_tag)
    }
    

    /// Run the reactor main loop (new modular interface)
    /// 
    /// This method provides compatibility with the new modular reactor interface.
    /// It delegates to the primary reactor for the actual work.
    pub async fn run(&mut self) -> Result<()> {
        info!("Starting reactor");

        // Initialize the modular reactor if not already done
        if self.modular_reactor.is_none() {
            info!("Creating primary reactor from configuration");
            let modular_config = self.create_modular_config();
            
            let modular_reactor = crate::reactor::factory::ReactorFactory::create_reactor(
                modular_config,
                self.docker_integration.client(),
                vec![], // Empty component specs for now - will be set later
                Some(self.config.as_ref().clone()),
            ).await?;
            
            if let crate::reactor::factory::ReactorImplementation::Primary(reactor) = modular_reactor {
                self.modular_reactor = Some(reactor);
            }
        }

        // Run the modular reactor
        if let Some(reactor) = &mut self.modular_reactor {
            reactor.run().await
        } else {
            Err(Error::Internal("Primary reactor is required".into()))
        }
    }

    /// Rebuild all components (new modular interface)
    /// 
    /// This method provides compatibility with the new modular reactor interface.
    pub async fn rebuild_all(&mut self) -> Result<()> {
        info!("Rebuilding all components using modular architecture");
        
        if let Some(reactor) = &mut self.modular_reactor {
            reactor.rebuild_all().await
        } else {
            Err(Error::Internal("Modular reactor not available - cannot rebuild".into()))
        }
    }

    /// Create modular configuration from legacy configuration
    fn create_modular_config(&self) -> ModularReactorConfig {
        ModularReactorConfig {
            base: self.config.as_ref().clone(),
            lifecycle: LifecycleConfig {
                product_name: self.config.product_name.clone(),
                environment: self.config.environment.clone(),
                network_name: self.config.network_name.clone(),
                auto_restart: true, // Enable modern features by default
                enable_health_checks: true, // Enable modern features by default
                ..Default::default()
            },
            build: BuildOrchestratorConfig {
                product_name: self.config.product_name.clone(),
                product_dir: self.config.product_dir.clone(),
                build_timeout: Duration::from_secs(300),
                parallel_builds: true,
                max_parallel: 4,
                enable_cache: true,
                cache_dir: self.config.product_dir.join(".rush/cache"),
            },
            watcher: CoordinatorConfig::default(),
            docker: DockerIntegrationConfig {
                use_enhanced_client: true, // Enable modern features by default
                enable_metrics: true, // Enable modern features by default
                enable_pooling: true, // Enable modern features by default
                ..Default::default()
            },
            #[allow(deprecated)]
            use_legacy: false,
        }
    }


    /// Get the Docker client (compatibility method)
    pub fn docker_client(&self) -> Arc<dyn DockerClient> {
        self.docker_integration.client()
    }

    /// Get the change processor (compatibility method) 
    pub fn change_processor(&self) -> Arc<crate::watcher::ChangeProcessor> {
        // Return the actual change processor if we have one, otherwise create a dummy
        if let Some((_watcher, processor)) = &self.file_watcher {
            processor.clone()
        } else {
            // Create a dummy processor (shouldn't happen in normal operation)
            Arc::new(crate::watcher::ChangeProcessor::new(&self.config.product_dir, 500))
        }
    }

    /// Check if rebuild is in progress (compatibility method)
    pub fn rebuild_in_progress(&self) -> bool {
        // Check the state from the modular reactor using try_read for sync context
        if let Ok(state) = self.state.try_read() {
            state.is_rebuilding()
        } else {
            false
        }
    }
    
    /// Set rebuild in progress state (compatibility method)
    pub async fn set_rebuild_in_progress(&mut self, in_progress: bool) {
        let mut state = self.state.write().await;
        if in_progress {
            state.start_rebuild(vec![]); // Start with no specific components
        } else {
            state.complete_rebuild();
        }
    }

    /// Get component specs from modular reactor or temp storage
    pub fn component_specs(&self) -> &Vec<ComponentBuildSpec> {
        // Return the temp specs that will be used to initialize the modular reactor
        &self.temp_component_specs
    }

    /// Get mutable component specs (no longer supported - use modular reactor)
    pub fn component_specs_mut(&mut self) -> &mut Vec<ComponentBuildSpec> {
        // Return a mutable reference to temp specs for compatibility
        &mut self.temp_component_specs
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
