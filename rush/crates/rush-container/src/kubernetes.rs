//! Kubernetes client trait and implementations
//!
//! This module defines the interface for Kubernetes operations
//! that will be implemented in the kubernetes crate.

use async_trait::async_trait;
use rush_core::error::Result;

/// Trait for Kubernetes operations
#[async_trait]
pub trait KubernetesClient: Send + Sync {
    /// Apply a Kubernetes manifest
    async fn apply_manifest(&self, manifest: &str) -> Result<()>;
    
    /// Delete a Kubernetes manifest
    async fn delete_manifest(&self, manifest: &str) -> Result<()>;
    
    /// Get pods in a namespace
    async fn get_pods(&self, namespace: &str) -> Result<Vec<String>>;
    
    /// Get services in a namespace
    async fn get_services(&self, namespace: &str) -> Result<Vec<String>>;
    
    /// Get rollout status of a deployment
    async fn rollout_status(&self, deployment: &str, namespace: &str) -> Result<String>;
    
    /// Set the kubectl context
    async fn set_context(&self, context: &str) -> Result<()>;
    
    /// Get the current kubectl context
    async fn current_context(&self) -> Result<String>;
}