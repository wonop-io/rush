//! Kubernetes manifest validation
//!
//! This module provides functionality for validating Kubernetes manifests
//! against schemas and checking for deprecated APIs.

use crate::error::{Error, Result};
use log::{debug, info, trace, warn};
use std::process::Command;

/// Trait defining operations for validating Kubernetes resources
pub trait K8sValidator: Send + Sync {
    /// Validates Kubernetes manifests against schema definitions
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the manifest file or directory
    /// * `kubernetes_version` - Target Kubernetes version for validation
    fn validate(&self, path: &str, kubernetes_version: &str) -> Result<()>;

    /// Checks for deprecated APIs in Kubernetes manifests
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the manifest file or directory
    /// * `kubernetes_version` - Target Kubernetes version for checking deprecations
    fn check_deprecations(&self, path: &str, kubernetes_version: &str) -> Result<()>;
}

/// Implementation that uses kubeconform for validation
pub struct KubeconformValidator;

impl K8sValidator for KubeconformValidator {
    fn validate(&self, path: &str, kubernetes_version: &str) -> Result<()> {
        trace!(
            "Validating manifests at {} with kubeconform (K8s version: {})",
            path,
            kubernetes_version
        );

        let output = Command::new("kubeconform")
            .arg("-kubernetes-version")
            .arg(kubernetes_version)
            .arg("-strict")
            .arg(path)
            .output()
            .map_err(|e| Error::External(format!("Failed to execute kubeconform: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            return Err(Error::Validation(format!(
                "Kubernetes manifest validation failed:\n{}\n{}",
                stderr, stdout
            )));
        }

        info!("Successfully validated manifests at {}", path);
        Ok(())
    }

    fn check_deprecations(&self, path: &str, kubernetes_version: &str) -> Result<()> {
        // Kubeconform currently doesn't have a separate deprecation mode,
        // but you could add additional logic here if needed in the future
        trace!("Checking for deprecated APIs using kubeconform");
        self.validate(path, kubernetes_version)
    }
}

/// Implementation that uses kubeval for validation
pub struct KubevalValidator;

impl K8sValidator for KubevalValidator {
    fn validate(&self, path: &str, kubernetes_version: &str) -> Result<()> {
        trace!(
            "Validating manifests at {} with kubeval (K8s version: {})",
            path,
            kubernetes_version
        );

        let output = Command::new("kubeval")
            .arg("--strict")
            .arg("--kubernetes-version")
            .arg(kubernetes_version)
            .arg(path)
            .output()
            .map_err(|e| Error::External(format!("Failed to execute kubeval: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            return Err(Error::Validation(format!(
                "Kubernetes manifest validation failed:\n{}\n{}",
                stderr, stdout
            )));
        }

        info!("Successfully validated manifests at {}", path);
        Ok(())
    }

    fn check_deprecations(&self, path: &str, kubernetes_version: &str) -> Result<()> {
        trace!("Checking for deprecated APIs using kubeval (additional flag)");

        let output = Command::new("kubeval")
            .arg("--strict")
            .arg("--kubernetes-version")
            .arg(kubernetes_version)
            .arg("--ignore-missing-schemas")
            .arg("--check-deprecated-apis")
            .arg(path)
            .output()
            .map_err(|e| Error::External(format!("Failed to execute kubeval: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            return Err(Error::Validation(format!(
                "Kubernetes API deprecation check failed:\n{}\n{}",
                stderr, stdout
            )));
        }

        debug!("No deprecated APIs found in manifests at {}", path);
        Ok(())
    }
}

/// Creates a K8sValidator based on the specified type
///
/// # Arguments
///
/// * `validator_type` - The type of validator to create
///
/// # Returns
///
/// A boxed K8sValidator implementation
pub fn create_validator(validator_type: &str) -> Box<dyn K8sValidator> {
    match validator_type {
        "kubeconform" => {
            info!("Using Kubeconform for Kubernetes manifest validation");
            Box::new(KubeconformValidator)
        }
        "kubeval" => {
            info!("Using Kubeval for Kubernetes manifest validation");
            Box::new(KubevalValidator)
        }
        _ => {
            warn!(
                "Unknown validator type '{}', defaulting to Kubeconform",
                validator_type
            );
            Box::new(KubeconformValidator)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_manifest(dir: &TempDir, filename: &str, content: &str) -> String {
        let file_path = dir.path().join(filename);
        let mut file = File::create(&file_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file_path.to_str().unwrap().to_string()
    }

    #[test]
    #[ignore] // Requires actual kubeval/kubeconform binaries
    fn test_kubeval_validator() {
        let temp_dir = TempDir::new().unwrap();
        let valid_manifest = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test-config
data:
  key: value
"#;

        let file_path = create_test_manifest(&temp_dir, "valid.yaml", valid_manifest);

        let validator = KubevalValidator;
        let result = validator.validate(&file_path, "1.22.0");

        assert!(result.is_ok());
    }

    #[test]
    #[ignore] // Requires actual kubeval/kubeconform binaries
    fn test_kubeconform_validator() {
        let temp_dir = TempDir::new().unwrap();
        let valid_manifest = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test-config
data:
  key: value
"#;

        let file_path = create_test_manifest(&temp_dir, "valid.yaml", valid_manifest);

        let validator = KubeconformValidator;
        let result = validator.validate(&file_path, "1.22.0");

        assert!(result.is_ok());
    }
}
