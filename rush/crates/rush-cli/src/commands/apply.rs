use std::sync::{Arc, Mutex};

use rush_config::Config;
use rush_container::ContainerService;
use rush_core::error::Result;
use rush_k8s::ContextManager;

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

    let config = rush_utils::CommandConfig::new(&kubectl)
        .args(vec!["apply", "-R", "-f", &output_dir])
        .capture(true);

    match rush_utils::CommandRunner::run(config).await {
        Ok(output) if output.success() => {
            println!("Successfully applied Kubernetes manifests");
            Ok(())
        }
        Ok(output) => Err(rush_core::error::Error::Kubernetes(format!(
            "Failed to apply manifests: {}",
            output.stderr
        ))),
        Err(e) => Err(rush_core::error::Error::Kubernetes(format!(
            "Failed to apply manifests: {e}"
        ))),
    }
}
