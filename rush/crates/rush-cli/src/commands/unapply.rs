use crate::args::CommonCliArgs;
use rush_config::Config;
use rush_container::ContainerReactor;
use rush_core::error::{Error, Result};
use rush_k8s::ContextManager;
use std::sync::Arc;
use std::sync::Mutex;

/// Execute the unapply command
///
/// The unapply command removes the Kubernetes resources from the cluster
pub async fn execute(_args: &CommonCliArgs, config: Arc<Config>) -> Result<()> {
    // Create a container reactor to handle the unapply operation
    let _reactor = ContainerReactor::new_with_config(config.clone())
        .map_err(|e| Error::Setup(format!("Failed to create container reactor: {e}")))?;

    // Create and set up the Kubernetes context manager
    let context_manager = Arc::new(Mutex::new(ContextManager::new("kubectl".to_string(), None)));

    // Set the Kubernetes context based on the environment
    if let Err(e) = context_manager
        .lock()
        .unwrap()
        .set_context(config.kube_context())
        .await
    {
        return Err(Error::Kubernetes(format!(
            "Failed to set Kubernetes context: {e}"
        )));
    }

    // Get the kubectl path from the context manager
    let kubectl_path = {
        let cm = context_manager.lock().unwrap();
        cm.kubectl_path().to_string()
    };

    // Perform the unapply operation using kubectl directly
    let manifest_path = config.output_path().join("k8s");
    let output_dir = manifest_path.display().to_string();

    match rush_utils::run_command(
        "kubectl delete",
        &kubectl_path,
        vec!["delete", "-R", "-f", &output_dir],
    )
    .await
    {
        Ok(_) => {
            println!("Successfully unapplied Kubernetes resources");
            Ok(())
        }
        Err(e) => Err(Error::Kubernetes(format!(
            "Failed to unapply Kubernetes resources: {e}"
        ))),
    }
}
