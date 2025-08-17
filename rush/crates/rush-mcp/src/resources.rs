//! MCP resources for accessing Rush data

use crate::error::{McpError, Result};
use crate::protocol::{ResourceContent, ResourceInfo, ResourceRead};
use rush_output::mcp_sink::McpLogEntry;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Resource registry for Rush MCP server
pub struct ResourceRegistry {
    resources: Vec<ResourceInfo>,
    log_buffer: Arc<RwLock<Vec<McpLogEntry>>>,
}

impl ResourceRegistry {
    /// Create a new resource registry
    pub fn new(log_buffer: Arc<RwLock<Vec<McpLogEntry>>>) -> Self {
        let mut registry = Self {
            resources: Vec::new(),
            log_buffer,
        };
        
        registry.register_resources();
        registry
    }

    /// Get list of available resources
    pub fn list_resources(&self) -> Vec<ResourceInfo> {
        self.resources.clone()
    }

    /// Read a resource
    pub async fn read(&self, request: ResourceRead) -> Result<ResourceContent> {
        let uri = &request.uri;
        log::debug!("Reading resource: {}", uri);

        // Parse URI
        if let Some(path) = uri.strip_prefix("logs://") {
            self.read_logs_resource(path).await
        } else if let Some(path) = uri.strip_prefix("status://") {
            self.read_status_resource(path).await
        } else if let Some(path) = uri.strip_prefix("config://") {
            self.read_config_resource(path).await
        } else {
            Err(McpError::ResourceNotFound(uri.to_string()))
        }
    }

    /// Register all available resources
    fn register_resources(&mut self) {
        // Log resources
        self.resources.push(ResourceInfo {
            uri: "logs://all".to_string(),
            name: "All Logs".to_string(),
            description: "All system and container logs".to_string(),
            mime_type: "application/json".to_string(),
        });

        self.resources.push(ResourceInfo {
            uri: "logs://system".to_string(),
            name: "System Logs".to_string(),
            description: "Rush system logs".to_string(),
            mime_type: "application/json".to_string(),
        });

        self.resources.push(ResourceInfo {
            uri: "logs://docker".to_string(),
            name: "Docker Logs".to_string(),
            description: "Container runtime logs".to_string(),
            mime_type: "application/json".to_string(),
        });

        self.resources.push(ResourceInfo {
            uri: "logs://script".to_string(),
            name: "Script Logs".to_string(),
            description: "Build script logs".to_string(),
            mime_type: "application/json".to_string(),
        });

        // Status resources
        self.resources.push(ResourceInfo {
            uri: "status://products".to_string(),
            name: "Products".to_string(),
            description: "List of available products".to_string(),
            mime_type: "application/json".to_string(),
        });

        self.resources.push(ResourceInfo {
            uri: "status://containers".to_string(),
            name: "Container Status".to_string(),
            description: "Status of all containers".to_string(),
            mime_type: "application/json".to_string(),
        });

        self.resources.push(ResourceInfo {
            uri: "status://builds".to_string(),
            name: "Build Status".to_string(),
            description: "Recent build status and history".to_string(),
            mime_type: "application/json".to_string(),
        });

        // Configuration resources
        self.resources.push(ResourceInfo {
            uri: "config://environments".to_string(),
            name: "Environments".to_string(),
            description: "Available deployment environments".to_string(),
            mime_type: "application/json".to_string(),
        });

        self.resources.push(ResourceInfo {
            uri: "config://settings".to_string(),
            name: "Settings".to_string(),
            description: "Rush configuration settings".to_string(),
            mime_type: "application/json".to_string(),
        });
    }

    /// Read logs resource
    async fn read_logs_resource(&self, path: &str) -> Result<ResourceContent> {
        let logs = self.log_buffer.read().await;
        
        let filtered_logs: Vec<&McpLogEntry> = match path {
            "all" => logs.iter().collect(),
            "system" => logs.iter().filter(|l| l.log_origin == "SYSTEM").collect(),
            "docker" => logs.iter().filter(|l| l.log_origin == "DOCKER").collect(),
            "script" => logs.iter().filter(|l| l.log_origin == "SCRIPT").collect(),
            component => {
                // Filter by component name
                logs.iter()
                    .filter(|l| l.component == component)
                    .collect()
            }
        };

        // Take last 100 logs
        let recent_logs: Vec<&McpLogEntry> = filtered_logs
            .into_iter()
            .rev()
            .take(100)
            .rev()
            .collect();

        Ok(ResourceContent {
            uri: format!("logs://{}", path),
            mime_type: "application/json".to_string(),
            text: Some(serde_json::to_string_pretty(&recent_logs)?),
            blob: None,
        })
    }

    /// Read status resource
    async fn read_status_resource(&self, path: &str) -> Result<ResourceContent> {
        let content = match path {
            "products" => {
                json!({
                    "products": [
                        {
                            "name": "io.wonop.helloworld",
                            "description": "Hello World application",
                            "components": ["frontend", "backend", "ingress"]
                        }
                    ]
                })
            }
            "containers" => {
                json!({
                    "containers": [
                        {
                            "name": "frontend",
                            "image": "io.wonop.helloworld-frontend:latest",
                            "status": "running",
                            "ports": ["3000"],
                            "uptime": "2h 15m"
                        },
                        {
                            "name": "backend",
                            "image": "io.wonop.helloworld-backend:latest",
                            "status": "running",
                            "ports": ["8080"],
                            "uptime": "2h 15m"
                        }
                    ]
                })
            }
            "builds" => {
                json!({
                    "builds": [
                        {
                            "id": "build-123",
                            "product": "io.wonop.helloworld",
                            "timestamp": "2024-01-15T10:30:00Z",
                            "status": "success",
                            "duration": "2m 30s"
                        }
                    ]
                })
            }
            _ => return Err(McpError::ResourceNotFound(format!("status://{}", path))),
        };

        Ok(ResourceContent {
            uri: format!("status://{}", path),
            mime_type: "application/json".to_string(),
            text: Some(serde_json::to_string_pretty(&content)?),
            blob: None,
        })
    }

    /// Read configuration resource
    async fn read_config_resource(&self, path: &str) -> Result<ResourceContent> {
        let content = match path {
            "environments" => {
                json!({
                    "environments": [
                        {
                            "name": "local",
                            "type": "development",
                            "vault": ".env"
                        },
                        {
                            "name": "dev",
                            "type": "development",
                            "vault": "1password"
                        },
                        {
                            "name": "staging",
                            "type": "staging",
                            "vault": "json"
                        },
                        {
                            "name": "prod",
                            "type": "production",
                            "vault": "json"
                        }
                    ]
                })
            }
            "settings" => {
                json!({
                    "docker_registry": "",
                    "k8s_version": "1.29.13",
                    "vault": ".env",
                    "mcp": {
                        "enabled": true,
                        "buffer_size": 1000
                    }
                })
            }
            _ => return Err(McpError::ResourceNotFound(format!("config://{}", path))),
        };

        Ok(ResourceContent {
            uri: format!("config://{}", path),
            mime_type: "application/json".to_string(),
            text: Some(serde_json::to_string_pretty(&content)?),
            blob: None,
        })
    }
}