//! Development environment manager
//!
//! This module coordinates local services and application containers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use log::{error, info};
use rush_build::{BuildType, ComponentBuildSpec};
use rush_core::error::{Error, Result};
use rush_local_services::{
    DockerLocalService, LocalServiceConfig, LocalServiceManager, LocalServiceType,
    ProcessLocalService,
};

use crate::ContainerReactor;

/// Manages the complete development environment
pub struct DevEnvironment {
    /// Manages local services (databases, etc.)
    local_services: LocalServiceManager,

    /// Manages application containers
    reactor: ContainerReactor,

    /// Network name
    network_name: String,

    /// Data directory for local services
    data_dir: PathBuf,
}

impl DevEnvironment {
    /// Create a new development environment
    pub fn new(reactor: ContainerReactor, network_name: String, data_dir: PathBuf) -> Self {
        Self {
            local_services: LocalServiceManager::new(),
            reactor,
            network_name,
            data_dir,
        }
    }

    /// Register local services from component specs
    pub fn register_local_services(
        &mut self,
        component_specs: &[ComponentBuildSpec],
    ) -> Result<()> {
        for spec in component_specs {
            if let BuildType::LocalService {
                service_type,
                persist_data,
                env,
                health_check,
                init_scripts,
                post_startup_tasks,
                command,
                ..
            } = &spec.build_type
            {
                info!("Registering local service: {}", spec.component_name);

                // Skip Stripe CLI - it needs special handling
                if service_type == "stripe-cli" || service_type == "stripe" {
                    self.register_stripe_service(spec, command.clone())?;
                    continue;
                }

                // Create configuration for Docker-based service
                let config = LocalServiceConfig {
                    name: spec.component_name.clone(),
                    service_type: self.parse_service_type(service_type),
                    image: None, // Will use default for service type
                    persist_data: *persist_data,
                    env: env.clone().unwrap_or_default(),
                    ports: vec![],   // TODO: Extract from spec
                    volumes: vec![], // TODO: Extract from spec
                    docker_args: vec![],
                    health_check: health_check.clone(),
                    init_scripts: init_scripts.clone().unwrap_or_default(),
                    post_startup_tasks: post_startup_tasks.clone().unwrap_or_default(),
                    depends_on: vec![], // TODO: Extract dependencies
                    command: command.clone(),
                    network_mode: Some(self.network_name.clone()),
                    container_name: None,
                    resources: None,
                };

                // Create Docker-based service with compatibility shim
                // TODO: Update DockerLocalService to use SimpleDocker
                let docker_client =
                    Arc::new(crate::docker::DockerCliClient::new("docker".to_string()));
                let service = DockerLocalService::new(
                    spec.component_name.clone(),
                    config.service_type.clone(),
                    docker_client,
                    config,
                    self.network_name.clone(),
                    self.data_dir.clone(),
                );

                self.local_services.register(Box::new(service));
            }
        }

        Ok(())
    }

    /// Register Stripe CLI as a process-based service
    fn register_stripe_service(
        &mut self,
        spec: &ComponentBuildSpec,
        command: Option<String>,
    ) -> Result<()> {
        info!("Registering Stripe CLI service: {}", spec.component_name);

        // Extract webhook URL from environment or use default
        let webhook_url = spec
            .env
            .as_ref()
            .and_then(|e| e.get("STRIPE_WEBHOOK_URL"))
            .cloned()
            .unwrap_or_else(|| "http://localhost:8080/api/stripe/webhook".to_string());

        // Parse command or use default
        let (cmd, args) = if let Some(command_str) = command {
            let parts: Vec<String> = command_str.split_whitespace().map(String::from).collect();
            if parts.is_empty() {
                (
                    "stripe".to_string(),
                    vec![
                        "listen".to_string(),
                        "--forward-to".to_string(),
                        webhook_url,
                        "--skip-verify".to_string(),
                    ],
                )
            } else {
                (parts[0].clone(), parts[1..].to_vec())
            }
        } else {
            (
                "stripe".to_string(),
                vec![
                    "listen".to_string(),
                    "--forward-to".to_string(),
                    webhook_url,
                    "--skip-verify".to_string(),
                ],
            )
        };

        // Create process-based service for Stripe
        let service = ProcessLocalService::new(
            spec.component_name.clone(),
            LocalServiceType::StripeCLI,
            cmd,
            args,
            HashMap::new(),
            true, // Use PTY for Stripe
        );

        self.local_services.register(Box::new(service));
        Ok(())
    }

    /// Parse service type from string
    fn parse_service_type(&self, service_type: &str) -> LocalServiceType {
        match service_type.to_lowercase().as_str() {
            "postgresql" | "postgres" => LocalServiceType::PostgreSQL,
            "mysql" => LocalServiceType::MySQL,
            "mongodb" | "mongo" => LocalServiceType::MongoDB,
            "redis" => LocalServiceType::Redis,
            "localstack" => LocalServiceType::LocalStack,
            "minio" => LocalServiceType::MinIO,
            "elasticmq" => LocalServiceType::ElasticMQ,
            "stripe-cli" | "stripe" => LocalServiceType::StripeCLI,
            "mailhog" => LocalServiceType::MailHog,
            custom => LocalServiceType::Custom(custom.to_string()),
        }
    }

    /// Start the development environment
    pub async fn start(&mut self) -> Result<()> {
        // Phase 1: Start local services
        info!("Phase 1: Starting local services...");
        self.local_services
            .start_all()
            .await
            .map_err(|e| Error::Docker(format!("Failed to start local services: {e}")))?;

        // Phase 2: Wait for services to be healthy
        info!("Phase 2: Waiting for local services to be healthy...");
        self.local_services
            .wait_for_healthy(Duration::from_secs(60))
            .await
            .map_err(|e| Error::Docker(format!("Failed waiting for services: {e}")))?;

        // Phase 3: Inject service environment variables
        info!("Phase 3: Injecting service environment variables...");
        let env_vars = self.local_services.get_env_vars();
        let env_secrets = self.local_services.get_env_secrets();

        // Add environment variables to reactor
        for (key, value) in env_vars {
            info!("Adding environment variable: {key}=...");
            self.reactor.add_env_var(key.clone(), value.clone());
        }

        for (key, value) in env_secrets {
            info!("Adding secret: {key}=...");
            self.reactor.add_env_var(key.clone(), value.clone());
        }

        // Phase 4: Start application containers
        info!("Phase 4: Starting application containers...");

        // Run the reactor - it will handle its own lifecycle
        self.reactor.launch().await
    }

    /// Stop the development environment
    pub async fn stop(&mut self) -> Result<()> {
        // The reactor handles its own cleanup through the shutdown signal
        info!("DevEnvironment::stop() called - stopping all local services");

        // Check if we're in a shutdown scenario
        let shutdown_token = rush_core::shutdown::global_shutdown().cancellation_token();
        if shutdown_token.is_cancelled() {
            info!("Shutdown detected in stop() - will forcefully stop local services");
        }

        // Stop local services
        info!("Calling local_services.stop_all()...");
        match self.local_services.stop_all().await {
            Ok(()) => info!("Local services stopped successfully"),
            Err(e) => error!("Failed to stop local services: {e}"),
        }

        info!("DevEnvironment::stop() completed");
        Ok(())
    }

    /// Get status of all services
    pub async fn get_status(&self) -> Vec<(String, bool)> {
        self.local_services.get_status().await
    }
}
