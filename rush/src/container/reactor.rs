//! Container orchestration and lifecycle management
//!
//! The reactor is responsible for orchestrating container lifecycle events,
//! monitoring containers, and handling file change events that trigger rebuilds.
use crate::container::DockerCliClient;
use crate::container::Status;
use crate::container::{
    docker::{ContainerStatus, DockerClient, DockerService, DockerServiceConfig},
    network::setup_network,
    watcher::{setup_file_watcher, ChangeProcessor, WatcherConfig},
    BuildProcessor, ServiceCollection,
};
use crate::core::config::Config;
use crate::error::Result;
use crate::security::FileVault;
use crate::security::Vault;

use log::{error, info, warn};
use std::collections::{HashMap, HashSet};
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
    ///
    /// # Returns
    ///
    /// A new ContainerReactor instance
    pub fn new(
        config: ContainerReactorConfig,
        docker_client: Arc<dyn DockerClient>,
        vault: Arc<Mutex<dyn Vault + Send>>,
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
        };

        Self::new(reactor_config, docker_client, vault)
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

        // TODO: Implement actual build logic for all services
        // For each service in self.services:
        // 1. Create Docker service config
        // 2. Build the image

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

        let (status_sender, _status_receiver) = mpsc::channel::<Status>(100);

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
        let dockerfile_path = dockerfile.to_string_lossy();
        let context_path = context_dir.to_string_lossy();

        self.docker_client
            .build_image(&tag, &dockerfile_path, &context_path)
            .await
    }
}
