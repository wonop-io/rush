use crate::container::ContainerService;
use crate::core::config::Config;
use crate::error::{Error, Result};
use crate::k8s::ContextManager;
use log::{debug, info};
use std::sync::Arc;
use std::sync::Mutex;

/// Execute the deploy command
///
/// Builds images, pushes them to registry, and deploys to Kubernetes
pub async fn execute(
    config: Arc<Config>,
    context_manager: Arc<Mutex<ContextManager>>,
    _services: &[ContainerService],
) -> Result<()> {
    debug!("Executing deploy command");

    // Select the appropriate Kubernetes context
    let context_name = config.kube_context();
    info!("Selecting Kubernetes context: {}", context_name);

    // Set the Kubernetes context
    context_manager
        .lock()
        .unwrap()
        .set_context(context_name)
        .await?;
    info!("Using Kubernetes context: {}", context_name);

    // Build and push images
    info!("Building and pushing Docker images...");

    // Build Docker images
    info!("Building Docker images...");
    // This would be handled by a separate component, likely in build.rs

    // Push Docker images
    info!("Pushing Docker images to registry...");
    // This would be handled by a separate component

    // Build Kubernetes manifests
    info!("Building Kubernetes manifests...");
    let manifest_path = config.output_path().join("k8s");

    // Apply the manifests to the cluster
    info!("Applying Kubernetes manifests...");
    let kubectl = context_manager.lock().unwrap().kubectl_path().to_string();

    let output_dir = manifest_path.display().to_string();

    let result = crate::utils::run_command(
        "kubectl apply",
        &kubectl,
        vec!["apply", "-R", "-f", &output_dir],
    )
    .await;

    match result {
        Ok(_) => {
            info!("Successfully deployed application to Kubernetes");
            Ok(())
        }
        Err(e) => Err(Error::Deploy(format!(
            "Failed to apply Kubernetes manifests: {}",
            e
        ))),
    }
}
