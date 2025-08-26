use crate::args::CommonCliArgs;
use rush_config::Config;
use rush_core::error::{Error, Result};
use rush_k8s::ContextManager;
use std::sync::Arc;
use std::sync::Mutex;

/// Execute the unapply command
///
/// The unapply command removes the Kubernetes resources from the cluster
pub async fn execute(_args: &CommonCliArgs, config: Arc<Config>) -> Result<()> {
    // Reactor is not needed for unapply operation
    // The operation is handled directly via kubectl

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

    let config = rush_utils::CommandConfig::new(&kubectl_path)
        .args(vec!["delete", "-R", "-f", &output_dir])
        .capture(true);
    
    match rush_utils::CommandRunner::run(config).await {
        Ok(output) if output.success() => {
            println!("Successfully unapplied Kubernetes resources");
            Ok(())
        }
        Ok(output) => Err(Error::Kubernetes(format!(
            "Failed to unapply Kubernetes resources: {}", output.stderr
        ))),
        Err(e) => Err(Error::Kubernetes(format!(
            "Failed to unapply Kubernetes resources: {e}"
        ))),
    }
}
