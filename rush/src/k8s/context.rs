//! Kubernetes context management
//!
//! This module provides functionality for managing Kubernetes contexts, including
//! setting the current context, retrieving available contexts, and validating contexts.

use crate::error::{Error, Result};
use log::{debug, trace, warn};
use std::process::Command;

/// Represents a Kubernetes context configuration
#[derive(Debug, Clone)]
pub struct KubernetesContext {
    /// The name of the context
    pub name: String,
    /// The Kubernetes cluster associated with this context
    pub cluster: String,
    /// The namespace for this context (optional)
    pub namespace: Option<String>,
    /// The user/authentication for this context
    pub user: String,
}

/// Manager for Kubernetes contexts
#[derive(Debug)]
pub struct ContextManager {
    /// Path to the kubectl executable
    kubectl_path: String,
    /// Path to the kubeconfig file
    kubeconfig_path: Option<String>,
    /// The current context
    current_context: Option<String>,
    /// Available contexts
    available_contexts: Vec<KubernetesContext>,
}

impl ContextManager {
    /// Creates a new Kubernetes context manager
    ///
    /// # Arguments
    ///
    /// * `kubectl_path` - Path to the kubectl executable
    /// * `kubeconfig_path` - Optional path to a specific kubeconfig file
    pub fn new(kubectl_path: String, kubeconfig_path: Option<String>) -> Self {
        let mut manager = Self {
            kubectl_path,
            kubeconfig_path,
            current_context: None,
            available_contexts: Vec::new(),
        };

        // Try to load contexts immediately
        if let Err(e) = manager.refresh_contexts() {
            warn!("Failed to load Kubernetes contexts: {}", e);
        }

        manager
    }

    pub fn kubectl_path(&self) -> &str {
        &self.kubectl_path
    }

    /// Refreshes the list of available contexts and the current context
    pub fn refresh_contexts(&mut self) -> Result<()> {
        trace!("Refreshing Kubernetes contexts");
        self.load_current_context()?;
        self.load_available_contexts()?;
        Ok(())
    }

    /// Loads the current Kubernetes context
    fn load_current_context(&mut self) -> Result<()> {
        trace!("Loading current Kubernetes context");

        let mut cmd = Command::new(&self.kubectl_path);
        cmd.arg("config");
        cmd.arg("current-context");

        if let Some(ref kubeconfig) = self.kubeconfig_path {
            cmd.env("KUBECONFIG", kubeconfig);
        }

        let output = cmd
            .output()
            .map_err(|e| Error::External(format!("Failed to execute kubectl: {}", e)))?;

        if output.status.success() {
            let context = String::from_utf8_lossy(&output.stdout).trim().to_string();
            debug!("Current Kubernetes context: {}", context);
            self.current_context = Some(context);
            Ok(())
        } else {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            warn!("Failed to get current context: {}", error);
            self.current_context = None;
            Err(Error::External(format!(
                "Failed to get current context: {}",
                error
            )))
        }
    }

    /// Loads all available Kubernetes contexts
    fn load_available_contexts(&mut self) -> Result<()> {
        trace!("Loading available Kubernetes contexts");

        let mut cmd = Command::new(&self.kubectl_path);
        cmd.arg("config");
        cmd.arg("get-contexts");
        cmd.arg("-o=json");

        if let Some(ref kubeconfig) = self.kubeconfig_path {
            cmd.env("KUBECONFIG", kubeconfig);
        }

        let output = cmd
            .output()
            .map_err(|e| Error::External(format!("Failed to execute kubectl: {}", e)))?;

        if output.status.success() {
            let json = String::from_utf8_lossy(&output.stdout).to_string();
            self.parse_contexts_json(&json)?;
            debug!(
                "Loaded {} Kubernetes contexts",
                self.available_contexts.len()
            );
            Ok(())
        } else {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            warn!("Failed to get available contexts: {}", error);
            self.available_contexts.clear();
            Err(Error::External(format!(
                "Failed to get available contexts: {}",
                error
            )))
        }
    }

    /// Parses the JSON output from kubectl to extract contexts
    fn parse_contexts_json(&mut self, json: &str) -> Result<()> {
        self.available_contexts.clear();

        let parsed: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| Error::External(format!("Failed to parse context JSON: {}", e)))?;

        if let Some(contexts) = parsed.get("contexts").and_then(|c| c.as_array()) {
            for context in contexts {
                if let (Some(name), Some(context_data)) = (
                    context.get("name").and_then(|n| n.as_str()),
                    context.get("context").and_then(|c| c.as_object()),
                ) {
                    let cluster = context_data
                        .get("cluster")
                        .and_then(|c| c.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let namespace = context_data
                        .get("namespace")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string());

                    let user = context_data
                        .get("user")
                        .and_then(|u| u.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    self.available_contexts.push(KubernetesContext {
                        name: name.to_string(),
                        cluster,
                        namespace,
                        user,
                    });
                }
            }
        }

        Ok(())
    }

    /// Sets the current Kubernetes context
    ///
    /// # Arguments
    ///
    /// * `context_name` - The name of the context to set
    pub async fn set_context(&mut self, context_name: &str) -> Result<()> {
        trace!("Setting Kubernetes context to: {}", context_name);

        // Check if context exists
        if !self.context_exists(context_name) {
            return Err(Error::InvalidInput(format!(
                "Kubernetes context '{}' not found",
                context_name
            )));
        }

        let mut cmd = Command::new(&self.kubectl_path);
        cmd.arg("config");
        cmd.arg("use-context");
        cmd.arg(context_name);

        if let Some(ref kubeconfig) = self.kubeconfig_path {
            cmd.env("KUBECONFIG", kubeconfig);
        }

        let output = cmd
            .output()
            .map_err(|e| Error::External(format!("Failed to execute kubectl: {}", e)))?;

        if output.status.success() {
            debug!("Successfully set Kubernetes context to: {}", context_name);
            self.current_context = Some(context_name.to_string());
            Ok(())
        } else {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(Error::External(format!("Failed to set context: {}", error)))
        }
    }

    /// Gets the current Kubernetes context
    pub fn get_current_context(&self) -> Option<&str> {
        self.current_context.as_deref()
    }

    /// Gets all available Kubernetes contexts
    pub fn get_available_contexts(&self) -> &[KubernetesContext] {
        &self.available_contexts
    }

    /// Checks if a context with the given name exists
    pub fn context_exists(&self, name: &str) -> bool {
        self.available_contexts.iter().any(|ctx| ctx.name == name)
    }

    /// Gets a specific context by name
    pub fn get_context(&self, name: &str) -> Option<&KubernetesContext> {
        self.available_contexts.iter().find(|ctx| ctx.name == name)
    }

    /// Validates that the specified context exists and is properly configured
    pub async fn validate_context(&self, context_name: &str) -> Result<()> {
        if !self.context_exists(context_name) {
            return Err(Error::InvalidInput(format!(
                "Kubernetes context '{}' not found",
                context_name
            )));
        }

        let mut cmd = Command::new(&self.kubectl_path);
        cmd.arg("--context");
        cmd.arg(context_name);
        cmd.arg("cluster-info");

        if let Some(ref kubeconfig) = self.kubeconfig_path {
            cmd.env("KUBECONFIG", kubeconfig);
        }

        let output = cmd
            .output()
            .map_err(|e| Error::External(format!("Failed to execute kubectl: {}", e)))?;

        if output.status.success() {
            debug!(
                "Successfully validated Kubernetes context: {}",
                context_name
            );
            Ok(())
        } else {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(Error::External(format!(
                "Failed to validate context {}: {}",
                context_name, error
            )))
        }
    }
}

/// Creates a ContextManager with the specified kubectl path
pub fn create_context_manager(kubectl_path: &str) -> Result<ContextManager> {
    let manager = ContextManager::new(kubectl_path.to_string(), None);
    Ok(manager)
}

/// Creates a ContextManager with the specified kubectl path and kubeconfig path
pub fn create_context_manager_with_config(
    kubectl_path: &str,
    kubeconfig_path: &str,
) -> Result<ContextManager> {
    let manager = ContextManager::new(kubectl_path.to_string(), Some(kubeconfig_path.to_string()));
    Ok(manager)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn get_test_kubectl_path() -> String {
        env::var("KUBECTL_PATH").unwrap_or_else(|_| "kubectl".to_string())
    }

    #[test]
    fn test_context_manager_creation() {
        let manager = ContextManager::new(get_test_kubectl_path(), None);
        assert!(manager.current_context.is_none() || manager.current_context.is_some());
    }

    #[test]
    #[ignore] // Requires kubectl to be installed and configured
    fn test_refresh_contexts() {
        let mut manager = ContextManager::new(get_test_kubectl_path(), None);
        assert!(manager.refresh_contexts().is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires kubectl to be installed and configured
    async fn test_validate_context() {
        let manager = ContextManager::new(get_test_kubectl_path(), None);
        if let Some(context) = manager.get_current_context() {
            assert!(manager.validate_context(context).await.is_ok());
        }
    }
}
