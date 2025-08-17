//! Main MCP server implementation

use crate::error::Result;
use crate::protocol::*;
use crate::resources::ResourceRegistry;
use crate::tools::ToolRegistry;
use crate::transport::{error_response, success_response, Transport};
use log::{debug, error, info, warn};
use rush_output::mcp_sink::{McpLogEntry, McpSink, McpSinkBuilder};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// MCP server configuration
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub version: String,
    pub buffer_size: usize,
    pub enable_experimental: bool,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: "rush-mcp".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            buffer_size: 1000,
            enable_experimental: false,
        }
    }
}

/// MCP server state
struct ServerState {
    initialized: bool,
    client_info: Option<ClientInfo>,
    session_id: String,
}

/// Main MCP server
pub struct McpServer {
    config: McpServerConfig,
    state: Arc<RwLock<ServerState>>,
    tools: Arc<ToolRegistry>,
    resources: Arc<ResourceRegistry>,
    log_buffer: Arc<RwLock<Vec<McpLogEntry>>>,
    mcp_sink: Option<McpSink>,
}

impl McpServer {
    /// Create a new MCP server
    pub fn new(config: McpServerConfig) -> Self {
        let log_buffer = Arc::new(RwLock::new(Vec::with_capacity(config.buffer_size)));
        
        Self {
            config: config.clone(),
            state: Arc::new(RwLock::new(ServerState {
                initialized: false,
                client_info: None,
                session_id: Uuid::new_v4().to_string(),
            })),
            tools: Arc::new(ToolRegistry::new()),
            resources: Arc::new(ResourceRegistry::new(log_buffer.clone())),
            log_buffer,
            mcp_sink: Some(
                McpSinkBuilder::new()
                    .max_buffer_size(config.buffer_size)
                    .build()
            ),
        }
    }

    /// Get the MCP sink for output routing
    pub fn get_sink(&mut self) -> Option<McpSink> {
        self.mcp_sink.take()
    }

    /// Run the server with the given transport
    pub async fn run<T: Transport>(self, mut transport: T) -> Result<()> {
        info!("Starting MCP server: {} v{}", self.config.name, self.config.version);

        loop {
            match transport.receive().await? {
                Some(request) => {
                    let response = self.handle_request(request).await;
                    transport.send(response).await?;
                }
                None => {
                    info!("Client disconnected");
                    break;
                }
            }
        }

        transport.close().await?;
        Ok(())
    }

    /// Handle an MCP request
    async fn handle_request(&self, request: McpRequest) -> McpResponse {
        debug!("Handling request: {} (id: {:?})", request.method, request.id);

        let method = McpMethod::from(request.method.as_str());
        
        match method {
            McpMethod::Initialize => self.handle_initialize(request).await,
            McpMethod::Initialized => self.handle_initialized(request).await,
            McpMethod::ToolsList => self.handle_tools_list(request).await,
            McpMethod::ToolsCall => self.handle_tools_call(request).await,
            McpMethod::ResourcesList => self.handle_resources_list(request).await,
            McpMethod::ResourcesRead => self.handle_resources_read(request).await,
            McpMethod::Ping => self.handle_ping(request).await,
            _ => {
                warn!("Unhandled method: {}", request.method);
                error_response(
                    request.id,
                    -32601,
                    format!("Method not found: {}", request.method),
                )
            }
        }
    }

    /// Handle initialize request
    async fn handle_initialize(&self, request: McpRequest) -> McpResponse {
        let params: InitializeParams = match serde_json::from_value(request.params) {
            Ok(p) => p,
            Err(e) => {
                return error_response(
                    request.id,
                    -32602,
                    format!("Invalid params: {}", e),
                );
            }
        };

        let mut state = self.state.write().await;
        state.initialized = true;
        state.client_info = Some(params.client_info.clone());

        info!(
            "Client connected: {} v{}", 
            params.client_info.name, 
            params.client_info.version
        );

        let result = InitializeResult {
            protocol_version: MCP_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: Some(ResourcesCapability {
                    subscribe: Some(true),
                    list_changed: Some(false),
                }),
                prompts: None,
                logging: Some(LoggingCapability {}),
                experimental: if self.config.enable_experimental {
                    let mut exp = HashMap::new();
                    exp.insert("rush_streaming".to_string(), json!(true));
                    exp
                } else {
                    HashMap::new()
                },
            },
            server_info: ServerInfo {
                name: self.config.name.clone(),
                version: self.config.version.clone(),
            },
        };

        success_response(request.id, serde_json::to_value(result).unwrap())
    }

    /// Handle initialized notification
    async fn handle_initialized(&self, request: McpRequest) -> McpResponse {
        info!("Client initialization complete");
        // This is a notification, no response needed
        success_response(request.id, json!({}))
    }

    /// Handle tools/list request
    async fn handle_tools_list(&self, request: McpRequest) -> McpResponse {
        let tools = self.tools.list_tools();
        success_response(
            request.id,
            json!({
                "tools": tools
            }),
        )
    }

    /// Handle tools/call request
    async fn handle_tools_call(&self, request: McpRequest) -> McpResponse {
        let call: ToolCall = match serde_json::from_value(request.params) {
            Ok(c) => c,
            Err(e) => {
                return error_response(
                    request.id,
                    -32602,
                    format!("Invalid params: {}", e),
                );
            }
        };

        info!("Executing tool: {}", call.name);

        match self.tools.execute(call).await {
            Ok(result) => success_response(request.id, serde_json::to_value(result).unwrap()),
            Err(e) => {
                error!("Tool execution failed: {}", e);
                error_response(
                    request.id,
                    -32000,
                    format!("Tool execution failed: {}", e),
                )
            }
        }
    }

    /// Handle resources/list request
    async fn handle_resources_list(&self, request: McpRequest) -> McpResponse {
        let resources = self.resources.list_resources();
        success_response(
            request.id,
            json!({
                "resources": resources
            }),
        )
    }

    /// Handle resources/read request
    async fn handle_resources_read(&self, request: McpRequest) -> McpResponse {
        let read_request: ResourceRead = match serde_json::from_value(request.params) {
            Ok(r) => r,
            Err(e) => {
                return error_response(
                    request.id,
                    -32602,
                    format!("Invalid params: {}", e),
                );
            }
        };

        info!("Reading resource: {}", read_request.uri);

        match self.resources.read(read_request).await {
            Ok(content) => {
                success_response(request.id, serde_json::to_value(content).unwrap())
            }
            Err(e) => {
                error!("Resource read failed: {}", e);
                error_response(
                    request.id,
                    -32000,
                    format!("Resource read failed: {}", e),
                )
            }
        }
    }

    /// Handle ping request
    async fn handle_ping(&self, request: McpRequest) -> McpResponse {
        success_response(
            request.id,
            json!({
                "pong": true,
                "session_id": self.state.read().await.session_id
            }),
        )
    }

    /// Update log buffer with new entries
    pub async fn update_logs(&self, entries: Vec<McpLogEntry>) {
        let mut buffer = self.log_buffer.write().await;
        
        // Remove old entries if buffer is full
        let available_space = self.config.buffer_size.saturating_sub(buffer.len());
        if entries.len() > available_space {
            let to_remove = entries.len() - available_space;
            buffer.drain(0..to_remove);
        }
        
        buffer.extend(entries);
    }
}