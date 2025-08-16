//! MCP tools for controlling Rush

use crate::error::{McpError, Result};
use crate::protocol::{ToolCall, ToolInfo, ToolResult};
use log::{debug, error, info};
use rush_core::error::Error as RushError;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Tool registry for Rush MCP server
pub struct ToolRegistry {
    tools: HashMap<String, ToolInfo>,
    handlers: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
            handlers: HashMap::new(),
        };
        
        // Register all tools
        registry.register_build_tools();
        registry.register_container_tools();
        registry.register_log_tools();
        registry.register_secret_tools();
        
        registry
    }

    /// Get list of available tools
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools.values().cloned().collect()
    }

    /// Execute a tool
    pub async fn execute(&self, call: ToolCall) -> Result<ToolResult> {
        let handler = self
            .handlers
            .get(&call.name)
            .ok_or_else(|| McpError::ToolNotFound(call.name.clone()))?;

        handler.execute(call.arguments).await
    }

    /// Register build management tools
    fn register_build_tools(&mut self) {
        // rush_build tool
        self.tools.insert(
            "rush_build".to_string(),
            ToolInfo {
                name: "rush_build".to_string(),
                description: "Build container images for a product".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Product to build"
                        },
                        "components": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Specific components to build"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Force rebuild even if cached",
                            "default": false
                        }
                    },
                    "required": ["product_name"]
                }),
            },
        );
        self.handlers.insert(
            "rush_build".to_string(),
            Arc::new(BuildToolHandler::new()),
        );

        // rush_dev tool
        self.tools.insert(
            "rush_dev".to_string(),
            ToolInfo {
                name: "rush_dev".to_string(),
                description: "Start development environment".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Product to run"
                        },
                        "output_format": {
                            "type": "string",
                            "enum": ["default", "split", "mcp"],
                            "description": "Output format",
                            "default": "mcp"
                        },
                        "redirect": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Component redirects (format: component@host:port)"
                        }
                    },
                    "required": ["product_name"]
                }),
            },
        );
        self.handlers.insert(
            "rush_dev".to_string(),
            Arc::new(DevToolHandler::new()),
        );

        // rush_deploy tool
        self.tools.insert(
            "rush_deploy".to_string(),
            ToolInfo {
                name: "rush_deploy".to_string(),
                description: "Deploy to an environment".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Product to deploy"
                        },
                        "environment": {
                            "type": "string",
                            "enum": ["dev", "staging", "prod"],
                            "description": "Target environment"
                        },
                        "dry_run": {
                            "type": "boolean",
                            "description": "Preview without deploying",
                            "default": false
                        }
                    },
                    "required": ["product_name", "environment"]
                }),
            },
        );
        self.handlers.insert(
            "rush_deploy".to_string(),
            Arc::new(DeployToolHandler::new()),
        );
    }

    /// Register container management tools
    fn register_container_tools(&mut self) {
        // rush_status tool
        self.tools.insert(
            "rush_status".to_string(),
            ToolInfo {
                name: "rush_status".to_string(),
                description: "Get status of running containers".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Filter by product"
                        }
                    }
                }),
            },
        );
        self.handlers.insert(
            "rush_status".to_string(),
            Arc::new(StatusToolHandler::new()),
        );

        // rush_stop tool
        self.tools.insert(
            "rush_stop".to_string(),
            ToolInfo {
                name: "rush_stop".to_string(),
                description: "Stop running containers".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Product to stop"
                        },
                        "components": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Specific components to stop"
                        }
                    },
                    "required": ["product_name"]
                }),
            },
        );
        self.handlers.insert(
            "rush_stop".to_string(),
            Arc::new(StopToolHandler::new()),
        );

        // rush_restart tool
        self.tools.insert(
            "rush_restart".to_string(),
            ToolInfo {
                name: "rush_restart".to_string(),
                description: "Restart containers".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Product to restart"
                        },
                        "components": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Specific components to restart"
                        }
                    },
                    "required": ["product_name"]
                }),
            },
        );
        self.handlers.insert(
            "rush_restart".to_string(),
            Arc::new(RestartToolHandler::new()),
        );
    }

    /// Register log management tools
    fn register_log_tools(&mut self) {
        // rush_logs tool
        self.tools.insert(
            "rush_logs".to_string(),
            ToolInfo {
                name: "rush_logs".to_string(),
                description: "Retrieve container logs".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Product name"
                        },
                        "component": {
                            "type": "string",
                            "description": "Specific component"
                        },
                        "lines": {
                            "type": "number",
                            "description": "Number of lines",
                            "default": 100
                        },
                        "follow": {
                            "type": "boolean",
                            "description": "Stream logs",
                            "default": false
                        },
                        "since": {
                            "type": "string",
                            "description": "Time filter (e.g., '10m', '1h')"
                        }
                    },
                    "required": ["product_name"]
                }),
            },
        );
        self.handlers.insert(
            "rush_logs".to_string(),
            Arc::new(LogsToolHandler::new()),
        );

        // rush_clear_logs tool
        self.tools.insert(
            "rush_clear_logs".to_string(),
            ToolInfo {
                name: "rush_clear_logs".to_string(),
                description: "Clear stored logs".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Product to clear logs for"
                        }
                    }
                }),
            },
        );
        self.handlers.insert(
            "rush_clear_logs".to_string(),
            Arc::new(ClearLogsToolHandler::new()),
        );
    }

    /// Register secret management tools
    fn register_secret_tools(&mut self) {
        // rush_secrets_init tool
        self.tools.insert(
            "rush_secrets_init".to_string(),
            ToolInfo {
                name: "rush_secrets_init".to_string(),
                description: "Initialize secrets for a product".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Product name"
                        },
                        "vault": {
                            "type": "string",
                            "enum": ["json", "1password", ".env"],
                            "description": "Vault type"
                        }
                    },
                    "required": ["product_name"]
                }),
            },
        );
        self.handlers.insert(
            "rush_secrets_init".to_string(),
            Arc::new(SecretsInitToolHandler::new()),
        );

        // rush_secrets_list tool
        self.tools.insert(
            "rush_secrets_list".to_string(),
            ToolInfo {
                name: "rush_secrets_list".to_string(),
                description: "List required secrets".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "product_name": {
                            "type": "string",
                            "description": "Product name"
                        }
                    },
                    "required": ["product_name"]
                }),
            },
        );
        self.handlers.insert(
            "rush_secrets_list".to_string(),
            Arc::new(SecretsListToolHandler::new()),
        );
    }
}

/// Trait for tool handlers
#[async_trait::async_trait]
trait ToolHandler: Send + Sync {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult>;
}

// Tool handler implementations (simplified for now)

struct BuildToolHandler;
impl BuildToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for BuildToolHandler {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult> {
        let product_name = args
            .get("product_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("product_name required".into()))?;

        info!("Building product: {}", product_name);
        
        // TODO: Integrate with actual Rush build system
        Ok(ToolResult {
            tool_result: json!({
                "status": "success",
                "message": format!("Build initiated for {}", product_name),
                "components": ["frontend", "backend", "ingress"]
            }),
            is_error: None,
        })
    }
}

struct DevToolHandler;
impl DevToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for DevToolHandler {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult> {
        let product_name = args
            .get("product_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("product_name required".into()))?;

        info!("Starting dev environment for: {}", product_name);
        
        // TODO: Integrate with actual Rush dev command
        Ok(ToolResult {
            tool_result: json!({
                "status": "success",
                "message": format!("Dev environment started for {}", product_name),
                "containers": {
                    "frontend": "running",
                    "backend": "running",
                    "database": "running"
                }
            }),
            is_error: None,
        })
    }
}

struct DeployToolHandler;
impl DeployToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for DeployToolHandler {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult> {
        let product_name = args
            .get("product_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("product_name required".into()))?;
        
        let environment = args
            .get("environment")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("environment required".into()))?;

        info!("Deploying {} to {}", product_name, environment);
        
        // TODO: Integrate with actual Rush deploy command
        Ok(ToolResult {
            tool_result: json!({
                "status": "success",
                "message": format!("Deployed {} to {}", product_name, environment),
                "environment": environment,
                "version": "v1.0.0"
            }),
            is_error: None,
        })
    }
}

struct StatusToolHandler;
impl StatusToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for StatusToolHandler {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult> {
        let product_name = args
            .get("product_name")
            .and_then(|v| v.as_str());

        info!("Getting container status");
        
        // TODO: Integrate with actual Rush status
        Ok(ToolResult {
            tool_result: json!({
                "containers": [
                    {
                        "name": "frontend",
                        "status": "running",
                        "uptime": "2h 15m"
                    },
                    {
                        "name": "backend",
                        "status": "running",
                        "uptime": "2h 15m"
                    }
                ]
            }),
            is_error: None,
        })
    }
}

struct StopToolHandler;
impl StopToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for StopToolHandler {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult> {
        let product_name = args
            .get("product_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("product_name required".into()))?;

        info!("Stopping containers for: {}", product_name);
        
        // TODO: Integrate with actual Rush stop
        Ok(ToolResult {
            tool_result: json!({
                "status": "success",
                "message": format!("Stopped containers for {}", product_name)
            }),
            is_error: None,
        })
    }
}

struct RestartToolHandler;
impl RestartToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for RestartToolHandler {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult> {
        let product_name = args
            .get("product_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("product_name required".into()))?;

        info!("Restarting containers for: {}", product_name);
        
        // TODO: Integrate with actual Rush restart
        Ok(ToolResult {
            tool_result: json!({
                "status": "success",
                "message": format!("Restarted containers for {}", product_name)
            }),
            is_error: None,
        })
    }
}

struct LogsToolHandler;
impl LogsToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for LogsToolHandler {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult> {
        let product_name = args
            .get("product_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("product_name required".into()))?;

        let lines = args
            .get("lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(100);

        info!("Getting logs for: {}", product_name);
        
        // TODO: Integrate with actual log retrieval
        Ok(ToolResult {
            tool_result: json!({
                "logs": [
                    "[2024-01-15 10:30:45] [DOCKER] backend | Server started",
                    "[2024-01-15 10:30:46] [DOCKER] frontend | App ready",
                    "[2024-01-15 10:30:47] [SYSTEM] rush | All containers running"
                ]
            }),
            is_error: None,
        })
    }
}

struct ClearLogsToolHandler;
impl ClearLogsToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for ClearLogsToolHandler {
    async fn execute(&self, _args: HashMap<String, Value>) -> Result<ToolResult> {
        info!("Clearing logs");
        
        // TODO: Integrate with actual log clearing
        Ok(ToolResult {
            tool_result: json!({
                "status": "success",
                "message": "Logs cleared"
            }),
            is_error: None,
        })
    }
}

struct SecretsInitToolHandler;
impl SecretsInitToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for SecretsInitToolHandler {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult> {
        let product_name = args
            .get("product_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("product_name required".into()))?;

        info!("Initializing secrets for: {}", product_name);
        
        // TODO: Integrate with actual secrets init
        Ok(ToolResult {
            tool_result: json!({
                "status": "success",
                "message": format!("Secrets initialized for {}", product_name)
            }),
            is_error: None,
        })
    }
}

struct SecretsListToolHandler;
impl SecretsListToolHandler {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ToolHandler for SecretsListToolHandler {
    async fn execute(&self, args: HashMap<String, Value>) -> Result<ToolResult> {
        let product_name = args
            .get("product_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("product_name required".into()))?;

        info!("Listing secrets for: {}", product_name);
        
        // TODO: Integrate with actual secrets list
        Ok(ToolResult {
            tool_result: json!({
                "secrets": [
                    {"name": "DATABASE_URL", "required": true},
                    {"name": "API_KEY", "required": true},
                    {"name": "JWT_SECRET", "required": false}
                ]
            }),
            is_error: None,
        })
    }
}