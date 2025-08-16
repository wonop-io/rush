//! Transport layer for MCP communication

use crate::error::{McpError, Result};
use crate::protocol::{McpRequest, McpResponse};
use async_trait::async_trait;
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Write};
use tokio::sync::mpsc;
use tokio::task;

/// Transport trait for MCP communication
#[async_trait]
pub trait Transport: Send + Sync {
    /// Receive a request from the client
    async fn receive(&mut self) -> Result<Option<McpRequest>>;
    
    /// Send a response to the client
    async fn send(&mut self, response: McpResponse) -> Result<()>;
    
    /// Close the transport
    async fn close(&mut self) -> Result<()>;
}

/// Stdio transport for subprocess mode
pub struct StdioTransport {
    rx: mpsc::Receiver<McpRequest>,
    tx: mpsc::Sender<McpResponse>,
    _reader_handle: task::JoinHandle<()>,
    _writer_handle: task::JoinHandle<()>,
}

impl StdioTransport {
    /// Create a new stdio transport
    pub fn new() -> Result<Self> {
        let (request_tx, request_rx) = mpsc::channel(100);
        let (response_tx, response_rx) = mpsc::channel(100);

        // Spawn reader task
        let reader_handle = task::spawn_blocking(move || {
            let stdin = io::stdin();
            let reader = BufReader::new(stdin);
            
            for line in reader.lines() {
                match line {
                    Ok(line) if !line.trim().is_empty() => {
                        // Parse JSON-RPC request
                        match serde_json::from_str::<McpRequest>(&line) {
                            Ok(request) => {
                                if request_tx.blocking_send(request).is_err() {
                                    break; // Channel closed
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to parse request: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read from stdin: {}", e);
                        break;
                    }
                    _ => {} // Empty line, ignore
                }
            }
        });

        // Spawn writer task
        let mut response_rx = response_rx;
        let writer_handle = task::spawn_blocking(move || {
            let mut stdout = io::stdout();
            
            while let Some(response) = response_rx.blocking_recv() {
                match serde_json::to_string(&response) {
                    Ok(json) => {
                        if writeln!(stdout, "{}", json).is_err() {
                            break;
                        }
                        if stdout.flush().is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to serialize response: {}", e);
                    }
                }
            }
        });

        Ok(Self {
            rx: request_rx,
            tx: response_tx,
            _reader_handle: reader_handle,
            _writer_handle: writer_handle,
        })
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn receive(&mut self) -> Result<Option<McpRequest>> {
        Ok(self.rx.recv().await)
    }

    async fn send(&mut self, response: McpResponse) -> Result<()> {
        self.tx
            .send(response)
            .await
            .map_err(|_| McpError::Transport("Failed to send response".into()))
    }

    async fn close(&mut self) -> Result<()> {
        // Channels will close when dropped
        Ok(())
    }
}

/// Helper to create error response
pub fn error_response(id: Option<Value>, code: i32, message: String) -> McpResponse {
    McpResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(crate::protocol::McpError {
            code,
            message,
            data: None,
        }),
    }
}

/// Helper to create success response
pub fn success_response(id: Option<Value>, result: Value) -> McpResponse {
    McpResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}