//! Multi-cluster Kubernetes support
//!
//! This module provides support for deploying to multiple Kubernetes clusters,
//! with cluster selection, health checking, and failover capabilities.

use rush_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;
use log::info;

/// Cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Cluster name
    pub name: String,
    /// Kubernetes context name
    pub context: String,
    /// Cluster region or location
    pub region: String,
    /// Cluster type (production, staging, dev)
    pub cluster_type: ClusterType,
    /// API server endpoint
    pub api_server: String,
    /// Whether this is the primary cluster
    pub is_primary: bool,
    /// Cluster-specific configuration
    pub config: HashMap<String, String>,
}

/// Cluster type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ClusterType {
    Production,
    Staging,
    Development,
}

/// Cluster health status
#[derive(Debug, Clone)]
pub struct ClusterHealth {
    pub cluster_name: String,
    pub is_healthy: bool,
    pub api_server_reachable: bool,
    pub nodes_ready: bool,
    pub control_plane_healthy: bool,
    pub message: String,
}

/// Multi-cluster manager
pub struct MultiClusterManager {
    clusters: Vec<ClusterConfig>,
    current_cluster: Option<String>,
}

impl MultiClusterManager {
    pub fn new() -> Self {
        Self {
            clusters: Vec::new(),
            current_cluster: None,
        }
    }
    
    /// Add a cluster configuration
    pub fn add_cluster(&mut self, config: ClusterConfig) {
        if self.current_cluster.is_none() && config.is_primary {
            self.current_cluster = Some(config.name.clone());
        }
        self.clusters.push(config);
    }
    
    /// Load clusters from configuration file
    pub fn load_from_config(config_path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(config_path)
            .map_err(|e| Error::Configuration(format!("Failed to read cluster config: {}", e)))?;
        
        let configs: Vec<ClusterConfig> = serde_yaml::from_str(&content)
            .map_err(|e| Error::Configuration(format!("Failed to parse cluster config: {}", e)))?;
        
        let mut manager = Self::new();
        for config in configs {
            manager.add_cluster(config);
        }
        
        Ok(manager)
    }
    
    /// Get current cluster
    pub fn current_cluster(&self) -> Option<&ClusterConfig> {
        self.current_cluster.as_ref()
            .and_then(|name| self.get_cluster(name))
    }
    
    /// Get cluster by name
    pub fn get_cluster(&self, name: &str) -> Option<&ClusterConfig> {
        self.clusters.iter().find(|c| c.name == name)
    }
    
    /// Set current cluster
    pub fn set_current_cluster(&mut self, name: String) -> Result<()> {
        if self.get_cluster(&name).is_none() {
            return Err(Error::Configuration(format!("Cluster '{}' not found", name)));
        }
        self.current_cluster = Some(name);
        Ok(())
    }
    
    /// Get clusters by type
    pub fn get_clusters_by_type(&self, cluster_type: ClusterType) -> Vec<&ClusterConfig> {
        self.clusters.iter()
            .filter(|c| c.cluster_type == cluster_type)
            .collect()
    }
    
    /// Check health of all clusters
    pub async fn check_health(&self) -> Vec<ClusterHealth> {
        let mut health_results = Vec::new();
        
        for cluster in &self.clusters {
            let health = self.check_cluster_health(cluster).await;
            health_results.push(health);
        }
        
        health_results
    }
    
    /// Check health of a specific cluster
    async fn check_cluster_health(&self, cluster: &ClusterConfig) -> ClusterHealth {
        info!("Checking health of cluster: {}", cluster.name);
        
        // Check API server connectivity
        let api_server_reachable = self.check_api_server(cluster).await.unwrap_or(false);
        
        if !api_server_reachable {
            return ClusterHealth {
                cluster_name: cluster.name.clone(),
                is_healthy: false,
                api_server_reachable: false,
                nodes_ready: false,
                control_plane_healthy: false,
                message: "API server not reachable".to_string(),
            };
        }
        
        // Check nodes status
        let nodes_ready = self.check_nodes_ready(cluster).await.unwrap_or(false);
        
        // Check control plane components
        let control_plane_healthy = self.check_control_plane(cluster).await.unwrap_or(false);
        
        let is_healthy = api_server_reachable && nodes_ready && control_plane_healthy;
        
        ClusterHealth {
            cluster_name: cluster.name.clone(),
            is_healthy,
            api_server_reachable,
            nodes_ready,
            control_plane_healthy,
            message: if is_healthy {
                "Cluster is healthy".to_string()
            } else {
                "Cluster has issues".to_string()
            },
        }
    }
    
    /// Check if API server is reachable
    async fn check_api_server(&self, cluster: &ClusterConfig) -> Result<bool> {
        let output = Command::new("kubectl")
            .arg("--context")
            .arg(&cluster.context)
            .arg("version")
            .arg("--short")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Command(format!("Failed to check API server: {}", e)))?;
        
        Ok(output.status.success())
    }
    
    /// Check if nodes are ready
    async fn check_nodes_ready(&self, cluster: &ClusterConfig) -> Result<bool> {
        let output = Command::new("kubectl")
            .arg("--context")
            .arg(&cluster.context)
            .arg("get")
            .arg("nodes")
            .arg("-o")
            .arg("json")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Command(format!("Failed to check nodes: {}", e)))?;
        
        if !output.status.success() {
            return Ok(false);
        }
        
        // Parse JSON output to check node status
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| Error::Serialization(format!("Failed to parse nodes JSON: {}", e)))?;
        
        if let Some(items) = json["items"].as_array() {
            for node in items {
                if let Some(conditions) = node["status"]["conditions"].as_array() {
                    let ready = conditions.iter().any(|c| {
                        c["type"] == "Ready" && c["status"] == "True"
                    });
                    
                    if !ready {
                        return Ok(false);
                    }
                }
            }
        }
        
        Ok(true)
    }
    
    /// Check control plane components
    async fn check_control_plane(&self, cluster: &ClusterConfig) -> Result<bool> {
        let output = Command::new("kubectl")
            .arg("--context")
            .arg(&cluster.context)
            .arg("-n")
            .arg("kube-system")
            .arg("get")
            .arg("pods")
            .arg("-l")
            .arg("tier=control-plane")
            .arg("-o")
            .arg("json")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Command(format!("Failed to check control plane: {}", e)))?;
        
        if !output.status.success() {
            return Ok(false);
        }
        
        // Parse JSON to check pod status
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| Error::Serialization(format!("Failed to parse pods JSON: {}", e)))?;
        
        if let Some(items) = json["items"].as_array() {
            for pod in items {
                if let Some(phase) = pod["status"]["phase"].as_str() {
                    if phase != "Running" {
                        return Ok(false);
                    }
                }
            }
        }
        
        Ok(true)
    }
    
    /// Deploy to multiple clusters with strategy
    pub async fn deploy_to_clusters(
        &self,
        deployment_strategy: MultiClusterStrategy,
        deploy_fn: impl Fn(&ClusterConfig) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> + Clone + Send + Sync + 'static,
    ) -> Result<()> {
        match deployment_strategy {
            MultiClusterStrategy::Sequential => {
                for cluster in &self.clusters {
                    info!("Deploying to cluster: {}", cluster.name);
                    deploy_fn(cluster).await?;
                }
            }
            MultiClusterStrategy::Parallel => {
                let mut tasks = Vec::new();
                
                for cluster in &self.clusters {
                    let cluster = cluster.clone();
                    let deploy_fn = deploy_fn.clone();
                    
                    let task = tokio::spawn(async move {
                        info!("Deploying to cluster: {}", cluster.name);
                        deploy_fn(&cluster).await
                    });
                    
                    tasks.push(task);
                }
                
                // Wait for all deployments
                for task in tasks {
                    task.await
                        .map_err(|e| Error::Async(format!("Deployment task failed: {}", e)))??;
                }
            }
            MultiClusterStrategy::Canary { percentage } => {
                // Deploy to a percentage of clusters first
                let canary_count = (self.clusters.len() as f32 * percentage as f32 / 100.0).ceil() as usize;
                let canary_count = canary_count.max(1);
                
                info!("Deploying to {} canary clusters", canary_count);
                
                for cluster in self.clusters.iter().take(canary_count) {
                    deploy_fn(cluster).await?;
                }
                
                // Wait for verification
                info!("Canary deployment complete, waiting for verification...");
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                
                // Deploy to remaining clusters
                for cluster in self.clusters.iter().skip(canary_count) {
                    deploy_fn(cluster).await?;
                }
            }
            MultiClusterStrategy::BlueGreen => {
                // Deploy to inactive clusters first
                let inactive_clusters: Vec<_> = self.clusters.iter()
                    .filter(|c| !c.is_primary)
                    .collect();
                
                for cluster in inactive_clusters {
                    deploy_fn(cluster).await?;
                }
                
                // Switch traffic (would need load balancer integration)
                info!("Switching traffic to new deployment");
                
                // Deploy to primary clusters
                let primary_clusters: Vec<_> = self.clusters.iter()
                    .filter(|c| c.is_primary)
                    .collect();
                
                for cluster in primary_clusters {
                    deploy_fn(cluster).await?;
                }
            }
        }
        
        Ok(())
    }
}

/// Multi-cluster deployment strategy
#[derive(Debug, Clone)]
pub enum MultiClusterStrategy {
    /// Deploy to clusters one by one
    Sequential,
    /// Deploy to all clusters in parallel
    Parallel,
    /// Deploy to a percentage of clusters first
    Canary { percentage: u32 },
    /// Blue-green deployment across clusters
    BlueGreen,
}

/// Namespace manager for multi-tenancy
pub struct NamespaceManager {
    default_namespace: String,
    namespace_mapping: HashMap<String, String>,
}

impl NamespaceManager {
    pub fn new(default_namespace: String) -> Self {
        Self {
            default_namespace,
            namespace_mapping: HashMap::new(),
        }
    }
    
    /// Get namespace for environment
    pub fn get_namespace(&self, environment: &str) -> String {
        self.namespace_mapping.get(environment)
            .cloned()
            .unwrap_or_else(|| self.default_namespace.clone())
    }
    
    /// Create namespace if it doesn't exist
    pub async fn ensure_namespace(&self, namespace: &str, cluster: &ClusterConfig) -> Result<()> {
        // Check if namespace exists
        let check_output = Command::new("kubectl")
            .arg("--context")
            .arg(&cluster.context)
            .arg("get")
            .arg("namespace")
            .arg(namespace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| Error::Command(format!("Failed to check namespace: {}", e)))?;
        
        if !check_output.status.success() {
            // Create namespace
            info!("Creating namespace: {}", namespace);
            
            let create_output = Command::new("kubectl")
                .arg("--context")
                .arg(&cluster.context)
                .arg("create")
                .arg("namespace")
                .arg(namespace)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
                .map_err(|e| Error::Command(format!("Failed to create namespace: {}", e)))?;
            
            if !create_output.status.success() {
                let stderr = String::from_utf8_lossy(&create_output.stderr);
                return Err(Error::Command(format!("Failed to create namespace: {}", stderr)));
            }
        }
        
        Ok(())
    }
    
    /// Apply resource quotas to namespace
    pub async fn apply_resource_quota(
        &self,
        namespace: &str,
        cluster: &ClusterConfig,
        quota: &ResourceQuota,
    ) -> Result<()> {
        let quota_yaml = format!(r#"
apiVersion: v1
kind: ResourceQuota
metadata:
  name: {}-quota
  namespace: {}
spec:
  hard:
    requests.cpu: "{}"
    requests.memory: {}Gi
    limits.cpu: "{}"
    limits.memory: {}Gi
    persistentvolumeclaims: "{}"
    services.nodeports: "{}"
"#,
            namespace, namespace,
            quota.cpu_request, quota.memory_request_gb,
            quota.cpu_limit, quota.memory_limit_gb,
            quota.pvc_count, quota.nodeport_count
        );
        
        // Apply quota using kubectl
        let mut child = Command::new("kubectl")
            .arg("--context")
            .arg(&cluster.context)
            .arg("apply")
            .arg("-f")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Command(format!("Failed to apply resource quota: {}", e)))?;
        
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(quota_yaml.as_bytes()).await
                .map_err(|e| Error::Configuration(format!("Failed to write quota YAML: {}", e)))?;
        }
        
        let output = child.wait_with_output().await
            .map_err(|e| Error::Command(format!("Failed to apply resource quota: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Command(format!("Failed to apply resource quota: {}", stderr)));
        }
        
        info!("Applied resource quota to namespace: {}", namespace);
        Ok(())
    }
}

/// Resource quota configuration
#[derive(Debug, Clone)]
pub struct ResourceQuota {
    pub cpu_request: String,
    pub cpu_limit: String,
    pub memory_request_gb: u32,
    pub memory_limit_gb: u32,
    pub pvc_count: u32,
    pub nodeport_count: u32,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            cpu_request: "10".to_string(),
            cpu_limit: "20".to_string(),
            memory_request_gb: 20,
            memory_limit_gb: 40,
            pvc_count: 10,
            nodeport_count: 5,
        }
    }
}