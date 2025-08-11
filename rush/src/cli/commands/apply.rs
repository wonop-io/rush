use crate::container::ContainerService;
use crate::core::config::Config;
use crate::error::Result;
use crate::k8s::ContextManager;
use std::sync::Arc;
use std::sync::Mutex;

/// Applies Kubernetes manifests to the cluster
pub async fn execute(
    config: Arc<Config>,
    context_manager: Arc<Mutex<ContextManager>>,
    _services: &[ContainerService],
) -> Result<()> {
    // Select the Kubernetes context
    context_manager
        .lock()
        .unwrap()
        .set_context(config.kube_context())
        .await?;

    // Build the manifests
    println!("Building Kubernetes manifests...");
    // Access the Config inside the Arc with a reference to resolve method not found error
    let manifest_path = config.output_path().join("k8s");

    // Apply the manifests
    println!(
        "Applying Kubernetes manifests to cluster '{}'...",
        config.kube_context()
    );
    let kubectl = {
        let x = context_manager.lock().unwrap();
        x.kubectl_path().to_string()
    };

    // Use recursive apply on the directory
    let output_dir = manifest_path.display().to_string();

    let result = crate::utils::run_command(
        "kubectl apply",
        &kubectl,
        vec!["apply", "-R", "-f", &output_dir],
    )
    .await;

    match result {
        Ok(_) => {
            println!("Successfully applied Kubernetes manifests");
            Ok(())
        }
        Err(e) => Err(crate::error::Error::Kubernetes(format!(
            "Failed to apply manifests: {}",
            e
        ))),
    }
}
