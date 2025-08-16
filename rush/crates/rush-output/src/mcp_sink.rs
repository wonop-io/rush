//! MCP (Model Context Protocol) sink for routing logs to MCP clients
//!
//! This sink buffers logs and makes them available to MCP clients,
//! supporting both retrieval and streaming modes.

use crate::simple::{LogEntry, LogOrigin, Sink};
use async_trait::async_trait;
use rush_core::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

/// Configuration for the MCP sink
#[derive(Debug, Clone)]
pub struct McpSinkConfig {
    /// Maximum number of log entries to buffer
    pub max_buffer_size: usize,
    /// Whether to emit JSON format
    pub json_format: bool,
    /// Product name for metadata
    pub product_name: Option<String>,
}

impl Default for McpSinkConfig {
    fn default() -> Self {
        Self {
            max_buffer_size: 1000,
            json_format: true,
            product_name: None,
        }
    }
}

/// MCP log entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpLogEntry {
    pub timestamp: String,
    pub log_origin: String,
    pub component: String,
    pub content: String,
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<McpMetadata>,
}

/// Metadata for MCP log entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
}

/// MCP sink that buffers logs and supports streaming
pub struct McpSink {
    /// Configuration
    config: McpSinkConfig,
    /// Buffered log entries
    buffer: Arc<RwLock<VecDeque<McpLogEntry>>>,
    /// Broadcast channel for streaming logs
    log_broadcaster: broadcast::Sender<McpLogEntry>,
    /// Stats tracking
    stats: Arc<Mutex<McpSinkStats>>,
}

/// Statistics for the MCP sink
#[derive(Debug, Default)]
pub struct McpSinkStats {
    pub total_logs: u64,
    pub errors: u64,
    pub buffer_overflows: u64,
}

impl McpSink {
    /// Create a new MCP sink
    pub fn new(config: McpSinkConfig) -> Self {
        let (tx, _) = broadcast::channel(100);
        let buffer_size = config.max_buffer_size;
        Self {
            config,
            buffer: Arc::new(RwLock::new(VecDeque::with_capacity(buffer_size))),
            log_broadcaster: tx,
            stats: Arc::new(Mutex::new(McpSinkStats::default())),
        }
    }

    /// Get a receiver for streaming logs
    pub fn subscribe(&self) -> broadcast::Receiver<McpLogEntry> {
        self.log_broadcaster.subscribe()
    }

    /// Get buffered logs
    pub async fn get_logs(&self, count: Option<usize>) -> Vec<McpLogEntry> {
        let buffer = self.buffer.read().await;
        let count = count.unwrap_or(buffer.len()).min(buffer.len());
        buffer.iter().rev().take(count).cloned().collect()
    }

    /// Get logs filtered by component
    pub async fn get_component_logs(&self, component: &str, count: Option<usize>) -> Vec<McpLogEntry> {
        let buffer = self.buffer.read().await;
        buffer
            .iter()
            .rev()
            .filter(|entry| entry.component == component)
            .take(count.unwrap_or(100))
            .cloned()
            .collect()
    }

    /// Clear the log buffer
    pub async fn clear_buffer(&self) {
        let mut buffer = self.buffer.write().await;
        buffer.clear();
    }

    /// Get sink statistics
    pub async fn get_stats(&self) -> McpSinkStats {
        let stats = self.stats.lock().await;
        McpSinkStats {
            total_logs: stats.total_logs,
            errors: stats.errors,
            buffer_overflows: stats.buffer_overflows,
        }
    }

    /// Convert LogEntry to McpLogEntry
    fn to_mcp_entry(&self, entry: LogEntry) -> McpLogEntry {
        McpLogEntry {
            timestamp: entry.timestamp.to_rfc3339(),
            log_origin: match entry.log_origin {
                LogOrigin::System => "SYSTEM".to_string(),
                LogOrigin::Script => "SCRIPT".to_string(),
                LogOrigin::Docker => "DOCKER".to_string(),
            },
            component: entry.component,
            content: entry.content.trim_end().to_string(),
            is_error: entry.is_error,
            metadata: self.config.product_name.as_ref().map(|product| McpMetadata {
                product: Some(product.clone()),
                container_id: None,
                build_id: None,
            }),
        }
    }

    /// Add entry to buffer with overflow handling
    async fn buffer_entry(&self, entry: McpLogEntry) {
        let mut buffer = self.buffer.write().await;
        
        // Remove oldest entries if at capacity
        while buffer.len() >= self.config.max_buffer_size {
            buffer.pop_front();
            let mut stats = self.stats.lock().await;
            stats.buffer_overflows += 1;
        }
        
        buffer.push_back(entry);
    }
}

#[async_trait]
impl Sink for McpSink {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        // Update stats
        {
            let mut stats = self.stats.lock().await;
            stats.total_logs += 1;
            if entry.is_error {
                stats.errors += 1;
            }
        }

        // Convert to MCP format
        let mcp_entry = self.to_mcp_entry(entry);

        // Buffer the entry
        self.buffer_entry(mcp_entry.clone()).await;

        // Broadcast to subscribers (ignore if no receivers)
        let _ = self.log_broadcaster.send(mcp_entry);

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        // MCP sink doesn't need explicit flushing
        // as it maintains an in-memory buffer
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.flush().await
    }
}

/// MCP sink builder for convenient configuration
pub struct McpSinkBuilder {
    config: McpSinkConfig,
}

impl McpSinkBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: McpSinkConfig::default(),
        }
    }

    /// Set the maximum buffer size
    pub fn max_buffer_size(mut self, size: usize) -> Self {
        self.config.max_buffer_size = size;
        self
    }

    /// Set JSON format mode
    pub fn json_format(mut self, enabled: bool) -> Self {
        self.config.json_format = enabled;
        self
    }

    /// Set the product name for metadata
    pub fn product_name(mut self, name: impl Into<String>) -> Self {
        self.config.product_name = Some(name.into());
        self
    }

    /// Build the MCP sink
    pub fn build(self) -> McpSink {
        McpSink::new(self.config)
    }
}

impl Default for McpSinkBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_mcp_sink_buffering() {
        let mut sink = McpSinkBuilder::new()
            .max_buffer_size(10)
            .product_name("test-product")
            .build();

        // Write some entries
        for i in 0..15 {
            let entry = LogEntry {
                component: format!("component-{}", i),
                content: format!("Log message {}", i),
                timestamp: Utc::now(),
                is_error: i % 3 == 0,
                log_origin: LogOrigin::Docker,
            };
            sink.write(entry).await.unwrap();
        }

        // Check buffer size (should be capped at 10)
        let logs = sink.get_logs(None).await;
        assert_eq!(logs.len(), 10);

        // Check stats
        let stats = sink.get_stats().await;
        assert_eq!(stats.total_logs, 15);
        assert_eq!(stats.errors, 5); // 0, 3, 6, 9, 12
        assert_eq!(stats.buffer_overflows, 5);
    }

    #[tokio::test]
    async fn test_component_filtering() {
        let mut sink = McpSinkBuilder::new().build();

        // Write entries for different components
        for component in &["frontend", "backend", "database"] {
            for i in 0..3 {
                let entry = LogEntry {
                    component: component.to_string(),
                    content: format!("Message {}", i),
                    timestamp: Utc::now(),
                    is_error: false,
                    log_origin: LogOrigin::Docker,
                };
                sink.write(entry).await.unwrap();
            }
        }

        // Get logs for specific component
        let backend_logs = sink.get_component_logs("backend", None).await;
        assert_eq!(backend_logs.len(), 3);
        for log in backend_logs {
            assert_eq!(log.component, "backend");
        }
    }
}