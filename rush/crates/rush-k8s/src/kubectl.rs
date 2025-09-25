//! Kubectl command wrapper for Kubernetes operations
//!
//! This module provides a safe and convenient interface for executing
//! kubectl commands with proper error handling and output processing.

use std::path::{Path, PathBuf};
use std::process::Command;

use log::{debug, error, info, warn};
use rush_core::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// Configuration for kubectl operations
#[derive(Debug, Clone)]
pub struct KubectlConfig {
    /// Path to kubectl binary (defaults to "kubectl")
    pub kubectl_path: String,
    /// Kubernetes context to use
    pub context: Option<String>,
    /// Namespace for operations
    pub namespace: Option<String>,
    /// Enable dry-run mode
    pub dry_run: bool,
    /// Kubeconfig file path
    pub kubeconfig: Option<PathBuf>,
    /// Enable verbose output
    pub verbose: bool,
}

impl Default for KubectlConfig {
    fn default() -> Self {
        Self {
            kubectl_path: "kubectl".to_string(),
            context: None,
            namespace: None,
            dry_run: false,
            kubeconfig: None,
            verbose: false,
        }
    }
}

/// Result of a kubectl command execution
#[derive(Debug, Clone)]
pub struct KubectlResult {
    /// Command exit code
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Whether the command succeeded
    pub success: bool,
}

/// Kubectl wrapper for executing kubernetes operations
pub struct Kubectl {
    pub config: KubectlConfig,
}

impl Default for Kubectl {
    fn default() -> Self {
        Self::new(KubectlConfig::default())
    }
}

impl Kubectl {
    /// Create a new kubectl wrapper with the given configuration
    pub fn new(config: KubectlConfig) -> Self {
        Self { config }
    }

    /// Set the namespace for operations
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.config.namespace = Some(namespace);
        self
    }

    /// Set the context for operations
    pub fn with_context(mut self, context: String) -> Self {
        self.config.context = Some(context);
        self
    }

    /// Enable dry-run mode
    pub fn dry_run(mut self, enabled: bool) -> Self {
        self.config.dry_run = enabled;
        self
    }

    /// Apply a manifest file to the cluster
    pub async fn apply(&self, manifest_path: &Path) -> Result<KubectlResult> {
        let mut args = vec![
            "apply".to_string(),
            "-f".to_string(),
            manifest_path.display().to_string(),
        ];

        if self.config.dry_run {
            args.push("--dry-run=client".to_string());
            args.push("-o".to_string());
            args.push("yaml".to_string());
        }

        self.execute(args).await
    }

    /// Apply all manifests in a directory
    pub async fn apply_dir(&self, dir_path: &Path) -> Result<Vec<KubectlResult>> {
        if !dir_path.is_dir() {
            return Err(Error::Filesystem(format!(
                "{} is not a directory",
                dir_path.display()
            )));
        }

        let mut results = Vec::new();

        // Apply manifests in order: ConfigMaps, Secrets, Services, Deployments, Ingresses
        let order = ["configmap", "secret", "service", "deployment", "ingress"];

        for kind in &order {
            let _pattern = format!("*-{kind}.yaml");
            let entries = std::fs::read_dir(dir_path)?;

            for entry in entries {
                let entry = entry?;
                let path = entry.path();

                if path.is_file() {
                    if let Some(name) = path.file_name() {
                        if name.to_string_lossy().ends_with(&format!("-{kind}.yaml"))
                            || (kind == &"secret" && name.to_string_lossy() == "secrets.yaml")
                            || (kind == &"ingress" && name.to_string_lossy() == "ingress.yaml")
                        {
                            info!("Applying manifest: {}", path.display());
                            match self.apply(&path).await {
                                Ok(result) => {
                                    if result.success {
                                        info!("Successfully applied: {}", path.display());
                                    } else {
                                        warn!(
                                            "Failed to apply {}: {}",
                                            path.display(),
                                            result.stderr
                                        );
                                    }
                                    results.push(result);
                                }
                                Err(e) => {
                                    error!("Error applying {}: {}", path.display(), e);
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Delete resources from a manifest file
    pub async fn delete(&self, manifest_path: &Path) -> Result<KubectlResult> {
        let mut args = vec![
            "delete".to_string(),
            "-f".to_string(),
            manifest_path.display().to_string(),
            "--ignore-not-found=true".to_string(),
        ];

        if self.config.dry_run {
            args.push("--dry-run=client".to_string());
        }

        self.execute(args).await
    }

    /// Delete all resources from a directory of manifests
    pub async fn delete_dir(&self, dir_path: &Path) -> Result<Vec<KubectlResult>> {
        if !dir_path.is_dir() {
            return Err(Error::Filesystem(format!(
                "{} is not a directory",
                dir_path.display()
            )));
        }

        let mut results = Vec::new();

        // Delete in reverse order: Ingresses, Deployments, Services, Secrets, ConfigMaps
        let order = ["ingress", "deployment", "service", "secret", "configmap"];

        for kind in &order {
            let entries = std::fs::read_dir(dir_path)?;

            for entry in entries {
                let entry = entry?;
                let path = entry.path();

                if path.is_file() {
                    if let Some(name) = path.file_name() {
                        if name.to_string_lossy().ends_with(&format!("-{kind}.yaml"))
                            || (kind == &"secret" && name.to_string_lossy() == "secrets.yaml")
                            || (kind == &"ingress" && name.to_string_lossy() == "ingress.yaml")
                        {
                            info!("Deleting resources from: {}", path.display());
                            match self.delete(&path).await {
                                Ok(result) => {
                                    if result.success {
                                        info!("Successfully deleted: {}", path.display());
                                    } else if !result.stderr.contains("NotFound") {
                                        warn!(
                                            "Issue deleting {}: {}",
                                            path.display(),
                                            result.stderr
                                        );
                                    }
                                    results.push(result);
                                }
                                Err(e) => {
                                    error!("Error deleting {}: {}", path.display(), e);
                                    // Continue with other deletions even if one fails
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Get resources of a specific type
    pub async fn get(&self, resource_type: &str, name: Option<&str>) -> Result<KubectlResult> {
        let mut args = vec!["get".to_string(), resource_type.to_string()];

        if let Some(name) = name {
            args.push(name.to_string());
        }

        args.push("-o".to_string());
        args.push("json".to_string());

        self.execute(args).await
    }

    /// Check if a resource exists
    pub async fn exists(&self, resource_type: &str, name: &str) -> Result<bool> {
        match self.get(resource_type, Some(name)).await {
            Ok(result) => Ok(result.success),
            Err(_) => Ok(false),
        }
    }

    /// Execute a rollout restart for a deployment
    pub async fn rollout_restart(&self, deployment_name: &str) -> Result<KubectlResult> {
        let args = vec![
            "rollout".to_string(),
            "restart".to_string(),
            format!("deployment/{}", deployment_name),
        ];

        self.execute(args).await
    }

    /// Get rollout status for a deployment
    pub async fn rollout_status(&self, deployment_name: &str) -> Result<KubectlResult> {
        let args = vec![
            "rollout".to_string(),
            "status".to_string(),
            format!("deployment/{}", deployment_name),
            "--timeout=5m".to_string(),
        ];

        self.execute(args).await
    }

    /// Execute a kubectl command with the given arguments
    pub async fn execute(&self, mut args: Vec<String>) -> Result<KubectlResult> {
        // Add namespace if configured
        if let Some(namespace) = &self.config.namespace {
            args.push("-n".to_string());
            args.push(namespace.clone());
        }

        // Add context if configured
        if let Some(context) = &self.config.context {
            args.push("--context".to_string());
            args.push(context.clone());
        }

        // Add kubeconfig if configured
        if let Some(kubeconfig) = &self.config.kubeconfig {
            args.push("--kubeconfig".to_string());
            args.push(kubeconfig.display().to_string());
        }

        if self.config.verbose {
            debug!(
                "Executing kubectl command: {} {}",
                self.config.kubectl_path,
                args.join(" ")
            );
        }

        // Execute the command
        let output = Command::new(&self.config.kubectl_path)
            .args(&args)
            .output()
            .map_err(|e| Error::External(format!("Failed to execute kubectl: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        let success = output.status.success();

        if self.config.verbose {
            if !stdout.is_empty() {
                debug!("kubectl stdout: {stdout}");
            }
            if !stderr.is_empty() && !success {
                debug!("kubectl stderr: {stderr}");
            }
        }

        Ok(KubectlResult {
            exit_code,
            stdout,
            stderr,
            success,
        })
    }

    /// Wait for a deployment to be ready
    pub async fn wait_for_deployment(
        &self,
        deployment_name: &str,
        timeout_secs: u64,
    ) -> Result<()> {
        let args = vec![
            "wait".to_string(),
            format!("deployment/{}", deployment_name),
            "--for=condition=available".to_string(),
            format!("--timeout={}s", timeout_secs),
        ];

        let result = self.execute(args).await?;

        if !result.success {
            return Err(Error::External(format!(
                "Deployment {} did not become ready: {}",
                deployment_name, result.stderr
            )));
        }

        Ok(())
    }

    /// Scale a deployment to a specific number of replicas
    pub async fn scale(&self, deployment_name: &str, replicas: u32) -> Result<KubectlResult> {
        let args = vec![
            "scale".to_string(),
            format!("deployment/{}", deployment_name),
            format!("--replicas={}", replicas),
        ];

        self.execute(args).await
    }
}

/// Deployment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentInfo {
    pub name: String,
    pub namespace: String,
    pub replicas: u32,
    pub ready_replicas: Option<u32>,
    pub image: String,
    pub labels: std::collections::HashMap<String, String>,
}

/// Track deployment versions for rollback support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentVersion {
    pub deployment_name: String,
    pub namespace: String,
    pub version: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub manifest_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kubectl_config_default() {
        let config = KubectlConfig::default();
        assert_eq!(config.kubectl_path, "kubectl");
        assert!(!config.dry_run);
        assert!(config.namespace.is_none());
    }

    #[test]
    fn test_kubectl_builder() {
        let kubectl = Kubectl::default()
            .with_namespace("test-ns".to_string())
            .with_context("test-context".to_string())
            .dry_run(true);

        assert_eq!(kubectl.config.namespace, Some("test-ns".to_string()));
        assert_eq!(kubectl.config.context, Some("test-context".to_string()));
        assert!(kubectl.config.dry_run);
    }
}
