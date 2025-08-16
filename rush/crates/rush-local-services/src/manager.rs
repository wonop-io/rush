use crate::docker::DockerClient;
use crate::{
    config::LocalServiceConfig,
    error::{Error, Result},
    health::HealthStatus,
    service::{LocalServiceHandle, ServiceStatus},
};
use log::{debug, error, info};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages local services lifecycle
pub struct LocalServiceManager {
    /// Docker client for service management
    docker_client: Arc<dyn DockerClient>,

    /// Running local services
    services: Arc<RwLock<HashMap<String, LocalServiceHandle>>>,

    /// Service configurations
    configs: HashMap<String, LocalServiceConfig>,

    /// Data persistence directory
    data_dir: PathBuf,

    /// Network name for services
    network_name: String,
}

impl LocalServiceManager {
    /// Create a new LocalServiceManager
    pub fn new(
        docker_client: Arc<dyn DockerClient>,
        data_dir: PathBuf,
        network_name: String,
    ) -> Self {
        Self {
            docker_client,
            services: Arc::new(RwLock::new(HashMap::new())),
            configs: HashMap::new(),
            data_dir,
            network_name,
        }
    }

    /// Register a service configuration
    pub fn register(&mut self, config: LocalServiceConfig) {
        self.configs.insert(config.name.clone(), config);
    }

    /// Start all registered services in dependency order
    pub async fn start_all(&self) -> Result<()> {
        info!("Starting all local services");

        // Get dependency order
        let start_order = self.get_start_order()?;

        for service_name in start_order {
            self.start(&service_name).await?;
        }

        info!("All local services started successfully");
        Ok(())
    }

    /// Start a specific service (internal implementation)
    fn start_internal<'a>(
        &'a self,
        name: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { self.start_impl(name).await })
    }

    /// Start a specific service
    pub async fn start(&self, name: &str) -> Result<()> {
        self.start_internal(name).await
    }

    /// Internal implementation of start
    async fn start_impl(&self, name: &str) -> Result<()> {
        let config = self
            .configs
            .get(name)
            .ok_or_else(|| Error::ServiceNotFound(name.to_string()))?;

        // Check dependencies
        for dep in &config.depends_on {
            if !self.is_running(dep).await {
                self.start_internal(dep).await?;
            }
        }

        let mut services = self.services.write().await;

        // Check if already running
        if let Some(service) = services.get(name) {
            if service.is_running() {
                info!("Service {} is already running", name);
                return Ok(());
            }
        }

        // Create and start service
        let mut service = LocalServiceHandle::new(config.clone(), self.docker_client.clone());

        service.start().await?;

        // Run initialization scripts if any
        if !config.init_scripts.is_empty() {
            self.run_init_scripts(&service, config).await?;
        }

        services.insert(name.to_string(), service);

        Ok(())
    }

    /// Stop a specific service (internal implementation)
    fn stop_internal<'a>(
        &'a self,
        name: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { self.stop_impl(name).await })
    }

    /// Stop a specific service (and dependents)
    pub async fn stop(&self, name: &str) -> Result<()> {
        self.stop_internal(name).await
    }

    /// Internal implementation of stop
    async fn stop_impl(&self, name: &str) -> Result<()> {
        // Find services that depend on this one
        let dependents = self.get_dependents(name);

        // Stop dependents first
        for dep in dependents {
            self.stop_internal(&dep).await?;
        }

        // Stop the service
        let mut services = self.services.write().await;
        if let Some(mut service) = services.remove(name) {
            service.stop().await?;
        }

        Ok(())
    }

    /// Stop all services
    pub async fn stop_all(&self) -> Result<()> {
        info!("Stopping all local services");

        let mut services = self.services.write().await;
        for (name, mut service) in services.drain() {
            if let Err(e) = service.stop().await {
                error!("Failed to stop service {}: {}", name, e);
            }
        }

        Ok(())
    }

    /// Restart a specific service
    pub async fn restart(&self, name: &str) -> Result<()> {
        self.stop(name).await?;
        self.start(name).await
    }

    /// Check if a service is running
    pub async fn is_running(&self, name: &str) -> bool {
        let services = self.services.read().await;
        services.get(name).is_some_and(|s| s.is_running())
    }

    /// Get the status of all services
    pub async fn get_status(&self) -> HashMap<String, ServiceStatus> {
        let services = self.services.read().await;
        services
            .iter()
            .map(|(name, service)| (name.clone(), service.status()))
            .collect()
    }

    /// Check health of all services
    pub async fn health_check_all(&self) -> Result<HashMap<String, HealthStatus>> {
        let mut results = HashMap::new();
        let mut services = self.services.write().await;

        for (name, service) in services.iter_mut() {
            match service.check_health().await {
                Ok(status) => {
                    results.insert(name.clone(), status);
                }
                Err(e) => {
                    error!("Health check failed for {}: {}", name, e);
                    results.insert(name.clone(), HealthStatus::Unhealthy(e.to_string()));
                }
            }
        }

        Ok(results)
    }

    /// Get logs for a service
    pub async fn get_logs(&self, name: &str, lines: usize) -> Result<String> {
        let services = self.services.read().await;
        let service = services
            .get(name)
            .ok_or_else(|| Error::ServiceNotFound(name.to_string()))?;

        service.logs(lines).await
    }

    /// Get connection strings for all services
    pub async fn get_connection_strings(&self) -> HashMap<String, String> {
        let mut connections = HashMap::new();
        let services = self.services.read().await;

        for (name, service) in services.iter() {
            if !service.is_running() {
                continue;
            }

            let config = &service.config;
            let hostname = service.hostname();

            match &config.service_type {
                crate::types::LocalServiceType::PostgreSQL => {
                    let port = service.port().unwrap_or(5432);
                    let default_user = "postgres".to_string();
                    let default_pass = "postgres".to_string();
                    let default_db = "postgres".to_string();
                    let user = config.env.get("POSTGRES_USER").unwrap_or(&default_user);
                    let pass = config.env.get("POSTGRES_PASSWORD").unwrap_or(&default_pass);
                    let db = config.env.get("POSTGRES_DB").unwrap_or(&default_db);

                    connections.insert(
                        format!("{}_DATABASE_URL", name.to_uppercase()),
                        format!("postgres://{user}:{pass}@{hostname}:{port}/{db}"),
                    );
                }
                crate::types::LocalServiceType::Redis => {
                    let port = service.port().unwrap_or(6379);
                    connections.insert(
                        format!("{}_REDIS_URL", name.to_uppercase()),
                        format!("redis://{hostname}:{port}"),
                    );
                }
                crate::types::LocalServiceType::MinIO => {
                    let port = service.port().unwrap_or(9000);
                    connections.insert(
                        format!("{}_S3_ENDPOINT", name.to_uppercase()),
                        format!("http://{hostname}:{port}"),
                    );

                    if let Some(access_key) = config.env.get("MINIO_ROOT_USER") {
                        connections.insert(
                            format!("{}_S3_ACCESS_KEY", name.to_uppercase()),
                            access_key.clone(),
                        );
                    }

                    if let Some(secret_key) = config.env.get("MINIO_ROOT_PASSWORD") {
                        connections.insert(
                            format!("{}_S3_SECRET_KEY", name.to_uppercase()),
                            secret_key.clone(),
                        );
                    }
                }
                crate::types::LocalServiceType::LocalStack => {
                    let port = service.port().unwrap_or(4566);
                    connections.insert(
                        format!("{}_AWS_ENDPOINT", name.to_uppercase()),
                        format!("http://{hostname}:{port}"),
                    );

                    let default_region = "us-east-1".to_string();
                    connections.insert(
                        format!("{}_AWS_REGION", name.to_uppercase()),
                        config
                            .env
                            .get("AWS_DEFAULT_REGION")
                            .unwrap_or(&default_region)
                            .clone(),
                    );
                }
                _ => {}
            }
        }

        connections
    }

    /// Get the start order based on dependencies
    fn get_start_order(&self) -> Result<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = HashMap::new();

        for name in self.configs.keys() {
            self.visit_dependencies(name, &mut visited, &mut order)?;
        }

        Ok(order)
    }

    /// Visit dependencies recursively (topological sort)
    fn visit_dependencies(
        &self,
        name: &str,
        visited: &mut HashMap<String, bool>,
        order: &mut Vec<String>,
    ) -> Result<()> {
        if let Some(&in_progress) = visited.get(name) {
            if in_progress {
                return Err(Error::Configuration(format!(
                    "Circular dependency detected: {name}"
                )));
            }
            return Ok(());
        }

        visited.insert(name.to_string(), true);

        if let Some(config) = self.configs.get(name) {
            for dep in &config.depends_on {
                self.visit_dependencies(dep, visited, order)?;
            }
        }

        visited.insert(name.to_string(), false);
        order.push(name.to_string());

        Ok(())
    }

    /// Get services that depend on the given service
    fn get_dependents(&self, name: &str) -> Vec<String> {
        let mut dependents = Vec::new();

        for (service_name, config) in &self.configs {
            if config.depends_on.contains(&name.to_string()) {
                dependents.push(service_name.clone());
            }
        }

        dependents
    }

    /// Run initialization scripts for a service
    async fn run_init_scripts(
        &self,
        service: &LocalServiceHandle,
        config: &LocalServiceConfig,
    ) -> Result<()> {
        if let Some(container_id) = service.container_id() {
            info!("Running initialization scripts for {}", config.name);

            for script in &config.init_scripts {
                debug!("Running init script: {}", script);

                let result = self
                    .docker_client
                    .exec_in_container(container_id, &["sh", "-c", script])
                    .await;

                if let Err(e) = result {
                    error!("Init script failed for {}: {}", config.name, e);
                    return Err(Error::Docker(format!("Init script failed: {e}")));
                }
            }

            info!("Initialization scripts completed for {}", config.name);
        }

        Ok(())
    }
}
