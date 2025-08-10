use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use log::{debug, error, info, trace, warn};
use tokio::sync::mpsc;

use crate::{
    build::BuildContext,
    container::{setup_network, ContainerService, DockerClient, ServiceConfig},
    error::{Error, Result},
    utils::run_command,
};

/// Launches containers based on the provided build context and configuration.
///
/// This function handles the startup flow for containers including:
/// 1. Setting up networks
/// 2. Building images if needed
/// 3. Starting containers with proper configuration
/// 4. Monitoring startup progress
///
/// # Arguments
///
/// * `context` - The build context containing service configurations
/// * `docker_client` - Client for Docker operations
/// * `status_sender` - Channel to report status updates
///
/// # Returns
///
/// * `Result<Vec<ContainerService>>` - The started container services or an error
pub async fn launch_containers(
    context: &BuildContext,
    docker_client: Arc<dyn DockerClient>,
    status_sender: mpsc::Sender<ContainerStatus>,
) -> Result<Vec<ContainerService>> {
    // Set up the network for container communication
    let network_name = format!("net-{}", context.product_uri);
    setup_network(&network_name, &docker_client).await?;

    // Collect services to launch from context
    let services = collect_services_from_context(context)?;

    // Launch services in dependency order
    let mut launched_services = Vec::new();
    for service_config in services {
        info!("Launching service: {}", service_config.name);

        // Send status update
        let _ = status_sender
            .send(ContainerStatus::Starting(service_config.name.clone()))
            .await;

        // Create and start the service
        match launch_service(&service_config, &network_name, &docker_client).await {
            Ok(service) => {
                launched_services.push(service);
                let _ = status_sender
                    .send(ContainerStatus::Running(service_config.name))
                    .await;
            }
            Err(e) => {
                error!("Failed to launch service {}: {}", service_config.name, e);
                let _ = status_sender
                    .send(ContainerStatus::Failed(service_config.name, e.to_string()))
                    .await;
                return Err(Error::LaunchFailed(format!(
                    "Service startup failed: {}",
                    e
                )));
            }
        }
    }

    info!("Successfully launched {} services", launched_services.len());
    Ok(launched_services)
}

/// Collects service configurations from the build context
fn collect_services_from_context(context: &BuildContext) -> Result<Vec<ServiceConfig>> {
    let mut services = Vec::new();

    for (domain, service_specs) in &context.services {
        for spec in service_specs {
            let config = ServiceConfig {
                name: spec.name.clone(),
                image: context.image_name.clone(),
                host: spec.host.clone(),
                port: spec.port,
                target_port: spec.target_port,
                environment: context.env.clone(),
                secrets: context.secrets.clone(),
                volumes: HashMap::new(), // TODO: Add volumes from context
                mount_point: spec.mount_point.clone(),
                domain: domain.clone(),
            };
            services.push(config);
        }
    }

    // Sort by dependency order (when implemented)
    Ok(services)
}

/// Launches a single service container
async fn launch_service(
    config: &ServiceConfig,
    network_name: &str,
    docker_client: &Arc<dyn DockerClient>,
) -> Result<ContainerService> {
    trace!(
        "Launching service {} with image {}",
        config.name,
        config.image
    );

    // Prepare container configuration
    let mut env_vars = Vec::new();
    for (key, value) in &config.environment {
        env_vars.push(format!("{}={}", key, value));
    }

    for (key, value) in &config.secrets {
        env_vars.push(format!("{}={}", key, value));
    }

    // Create port mapping string
    let port_mapping = format!("{}:{}", config.port, config.target_port);

    // Start the container with proper working directory
    // Set working directory to product directory (reverse domain notation as path)
    let working_dir = if let Some(product_name) = config.name.split('-').next() {
        format!("/app/{}", product_name.replace('.', "/"))
    } else {
        "/app".to_string()
    };
    
    let container_id = docker_client
        .run_container(
            &config.image,
            &config.name,
            &network_name,
            &env_vars,
            &[port_mapping],
            &[], // volumes
            Some(&working_dir),
        )
        .await?;

    debug!("Started container {} with ID {}", config.name, container_id);

    // Create service object
    let service = ContainerService {
        id: container_id,
        name: config.name.clone(),
        image: config.image.clone(),
        host: config.host.clone(),
        port: config.port,
        target_port: config.target_port,
        domain: config.domain.clone(),
        mount_point: config.mount_point.clone(),
        docker_host: "TODO".to_string(), // config.docker_host.clone(),
    };

    Ok(service)
}

/// Status enum for container lifecycle events
#[derive(Debug, Clone)]
pub enum ContainerStatus {
    Starting(String),
    Running(String),
    Restarting(String),
    Stopping(String),
    Stopped(String),
    Failed(String, String),
}
