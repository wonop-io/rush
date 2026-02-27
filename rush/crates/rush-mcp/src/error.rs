//! Error types for the MCP server

use thiserror::Error;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Container error: {0}")]
    ContainerError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Rush core error: {0}")]
    RushCore(#[from] rush_core::error::Error),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, McpError>;
