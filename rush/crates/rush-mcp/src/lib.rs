//! Rush MCP (Model Context Protocol) Server
//!
//! This crate provides an MCP server implementation that exposes Rush
//! functionality to AI assistants and other MCP clients.

pub mod error;
pub mod protocol;
pub mod resources;
pub mod server;
pub mod tools;
pub mod transport;

pub use error::{McpError, Result};
// Re-export commonly used types
pub use protocol::{McpRequest, McpResponse};
pub use server::{McpServer, McpServerConfig};
pub use transport::{StdioTransport, Transport};
