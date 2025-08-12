use rush_core::error::{Error, Result};
use rush_k8s::validation::{K8sValidator, KubeconformValidator, KubevalValidator};
use log::{error, info};
use std::path::Path;

/// Command implementation for validating Kubernetes manifests
pub async fn execute(
    manifest_path: &Path,
    kubernetes_version: &str,
    check_deprecations: bool,
    validator_type: &str,
) -> Result<()> {
    info!(
        "Validating Kubernetes manifests at {}",
        manifest_path.display()
    );

    // Create the appropriate validator based on the type
    let validator: Box<dyn K8sValidator> = match validator_type.to_lowercase().as_str() {
        "kubeconform" => Box::new(KubeconformValidator),
        "kubeval" => Box::new(KubevalValidator),
        _ => {
            return Err(Error::InvalidInput(format!(
                "Unsupported validator type: {validator_type}"
            )))
        }
    };

    // Run the validation
    if check_deprecations {
        info!("Checking for deprecated APIs in Kubernetes manifests...");
        match validator.check_deprecations(
            manifest_path.to_str().unwrap_or_default(),
            kubernetes_version,
        ) {
            Ok(_) => {
                info!("No deprecated APIs found in the manifests");
                Ok(())
            }
            Err(e) => {
                error!("Deprecated APIs found in manifests: {}", e);
                Err(Error::Validation(format!("Deprecated APIs found: {e}")))
            }
        }
    } else {
        info!(
            "Validating Kubernetes manifests against schema for version {}",
            kubernetes_version
        );
        match validator.validate(
            manifest_path.to_str().unwrap_or_default(),
            kubernetes_version,
        ) {
            Ok(_) => {
                info!("All manifests validated successfully");
                Ok(())
            }
            Err(e) => {
                error!("Validation failed: {}", e);
                Err(Error::Validation(format!(
                    "Schema validation failed: {e}"
                )))
            }
        }
    }
}
