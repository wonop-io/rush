//! Enhanced container log streaming
//!
//! This module provides improved log streaming capabilities with buffering,
//! filtering, and real-time processing.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, info};
use rush_core::error::Result;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;

use crate::docker::DockerClient;
use crate::events::{Event, EventBus};

/// Configuration for log streaming
#[derive(Debug, Clone)]
pub struct LogStreamConfig {
    /// Buffer size for log lines
    pub buffer_size: usize,
    /// Maximum lines to retrieve at once
    pub batch_size: usize,
    /// Interval between log fetches
    pub fetch_interval: Duration,
    /// Whether to follow logs (tail -f behavior)
    pub follow: bool,
    /// Number of recent lines to keep in memory
    pub recent_lines: usize,
    /// Enable timestamp parsing
    pub parse_timestamps: bool,
    /// Log level filtering
    pub min_level: LogLevel,
}

impl Default for LogStreamConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            batch_size: 100,
            fetch_interval: Duration::from_secs(1),
            follow: true,
            recent_lines: 500,
            parse_timestamps: true,
            min_level: LogLevel::Debug,
        }
    }
}

/// Log levels for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Parse log level from a log line
    fn from_line(line: &str) -> Self {
        let lower = line.to_lowercase();
        if lower.contains("error") || lower.contains("fatal") {
            LogLevel::Error
        } else if lower.contains("warn") {
            LogLevel::Warn
        } else if lower.contains("info") {
            LogLevel::Info
        } else if lower.contains("debug") {
            LogLevel::Debug
        } else {
            LogLevel::Trace
        }
    }
}

/// A parsed log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// The container ID
    pub container_id: String,
    /// The component name
    pub component: String,
    /// Timestamp of the log entry
    pub timestamp: Option<Instant>,
    /// Log level
    pub level: LogLevel,
    /// The log message
    pub message: String,
    /// Raw log line
    pub raw: String,
}

/// Log stream for a container
pub struct LogStream {
    /// Container ID
    container_id: String,
    /// Component name
    component: String,
    /// Configuration
    config: LogStreamConfig,
    /// Docker client
    docker_client: Arc<dyn DockerClient>,
    /// Event bus for publishing events
    event_bus: Option<EventBus>,
    /// Buffer of recent log entries
    buffer: Arc<RwLock<VecDeque<LogEntry>>>,
    /// Channel for sending log entries
    sender: mpsc::UnboundedSender<LogEntry>,
    /// Last processed line count
    last_line_count: Arc<RwLock<usize>>,
}

impl LogStream {
    /// Create a new log stream
    pub fn new(
        container_id: String,
        component: String,
        config: LogStreamConfig,
        docker_client: Arc<dyn DockerClient>,
    ) -> (Self, mpsc::UnboundedReceiver<LogEntry>) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let buffer_size = config.buffer_size;
        let stream = Self {
            container_id,
            component,
            config,
            docker_client,
            event_bus: None,
            buffer: Arc::new(RwLock::new(VecDeque::with_capacity(buffer_size))),
            sender,
            last_line_count: Arc::new(RwLock::new(0)),
        };

        (stream, receiver)
    }

    /// Set the event bus
    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Start streaming logs
    pub async fn start_streaming(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut fetch_interval = interval(self.config.fetch_interval);

            loop {
                fetch_interval.tick().await;

                if let Err(e) = self.fetch_and_process_logs().await {
                    error!(
                        "Error fetching logs for {} ({}): {}",
                        self.component, self.container_id, e
                    );

                    // Publish error event if we have event bus
                    if let Some(event_bus) = &self.event_bus {
                        let _ = event_bus
                            .publish(Event::error(
                                "log_streamer",
                                format!("Log fetch error for {}: {}", self.component, e),
                                false,
                            ))
                            .await;
                    }

                    // Back off on errors
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }

                if !self.config.follow {
                    break;
                }
            }
        })
    }

    /// Fetch and process new log lines
    async fn fetch_and_process_logs(&self) -> Result<()> {
        // Fetch logs from Docker
        let logs = self
            .docker_client
            .container_logs(&self.container_id, self.config.batch_size)
            .await?;

        // Split into lines
        let lines: Vec<&str> = logs.lines().collect();

        // Check if we have new lines
        let last_count = *self.last_line_count.read().await;
        if lines.len() <= last_count {
            return Ok(()); // No new logs
        }

        // Process new lines
        let new_lines = &lines[last_count..];
        let mut buffer = self.buffer.write().await;

        for line in new_lines {
            if line.is_empty() {
                continue;
            }

            // Parse the log entry
            let entry = self.parse_log_entry(line);

            // Check log level filter
            if entry.level < self.config.min_level {
                continue;
            }

            // Add to buffer
            if buffer.len() >= self.config.buffer_size {
                buffer.pop_front();
            }
            buffer.push_back(entry.clone());

            // Send to channel
            if let Err(e) = self.sender.send(entry) {
                debug!("Failed to send log entry: {e}");
            }
        }

        // Update last line count
        *self.last_line_count.write().await = lines.len();

        Ok(())
    }

    /// Parse a log line into a LogEntry
    fn parse_log_entry(&self, line: &str) -> LogEntry {
        let level = LogLevel::from_line(line);

        // Try to parse timestamp if enabled
        let timestamp = if self.config.parse_timestamps {
            self.parse_timestamp(line)
        } else {
            None
        };

        // Extract message (remove timestamp if found)
        let message = if self.config.parse_timestamps {
            self.extract_message(line)
        } else {
            line.to_string()
        };

        LogEntry {
            container_id: self.container_id.clone(),
            component: self.component.clone(),
            timestamp,
            level,
            message,
            raw: line.to_string(),
        }
    }

    /// Parse timestamp from log line
    fn parse_timestamp(&self, line: &str) -> Option<Instant> {
        // Simple heuristic: look for common timestamp patterns
        // This is a simplified implementation
        // Real implementation would use regex or proper parsing

        if line.len() > 20 {
            // Check if line starts with a timestamp-like pattern
            let potential_timestamp = &line[..20];
            if potential_timestamp.contains(':')
                && (potential_timestamp.contains('-') || potential_timestamp.contains('/'))
            {
                return Some(Instant::now()); // Placeholder
            }
        }

        None
    }

    /// Extract message from log line (removing timestamp)
    fn extract_message(&self, line: &str) -> String {
        // Simple implementation: if line has timestamp pattern, skip it
        if line.len() > 20 && line.contains('[') {
            if let Some(idx) = line.find(']') {
                return line[idx + 1..].trim().to_string();
            }
        }

        line.to_string()
    }

    /// Get recent log entries
    pub async fn get_recent_logs(&self, count: usize) -> Vec<LogEntry> {
        let buffer = self.buffer.read().await;
        let start = if buffer.len() > count {
            buffer.len() - count
        } else {
            0
        };

        buffer.iter().skip(start).cloned().collect()
    }

    /// Clear the log buffer
    pub async fn clear_buffer(&self) {
        let mut buffer = self.buffer.write().await;
        buffer.clear();
        *self.last_line_count.write().await = 0;
    }

    /// Search logs for a pattern
    pub async fn search_logs(&self, pattern: &str) -> Vec<LogEntry> {
        let buffer = self.buffer.read().await;
        buffer
            .iter()
            .filter(|entry| entry.message.contains(pattern) || entry.raw.contains(pattern))
            .cloned()
            .collect()
    }

    /// Get logs by level
    pub async fn get_logs_by_level(&self, min_level: LogLevel) -> Vec<LogEntry> {
        let buffer = self.buffer.read().await;
        buffer
            .iter()
            .filter(|entry| entry.level >= min_level)
            .cloned()
            .collect()
    }
}

/// Manager for multiple log streams
pub struct LogStreamManager {
    /// Active log streams
    streams: Arc<RwLock<Vec<Arc<LogStream>>>>,
    /// Docker client
    docker_client: Arc<dyn DockerClient>,
    /// Event bus
    event_bus: Option<EventBus>,
    /// Default configuration
    default_config: LogStreamConfig,
}

impl LogStreamManager {
    /// Create a new log stream manager
    pub fn new(docker_client: Arc<dyn DockerClient>, default_config: LogStreamConfig) -> Self {
        Self {
            streams: Arc::new(RwLock::new(Vec::new())),
            docker_client,
            event_bus: None,
            default_config,
        }
    }

    /// Set the event bus
    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Start streaming logs for a container
    pub async fn start_streaming(
        &self,
        container_id: String,
        component: String,
    ) -> mpsc::UnboundedReceiver<LogEntry> {
        let (mut stream, receiver) = LogStream::new(
            container_id,
            component,
            self.default_config.clone(),
            self.docker_client.clone(),
        );

        if let Some(event_bus) = &self.event_bus {
            stream = stream.with_event_bus(event_bus.clone());
        }

        let stream = Arc::new(stream);

        // Start streaming
        let stream_clone = stream.clone();
        stream_clone.start_streaming().await;

        // Store the stream
        let mut streams = self.streams.write().await;
        streams.push(stream);

        receiver
    }

    /// Stop streaming for a container
    pub async fn stop_streaming(&self, container_id: &str) {
        let mut streams = self.streams.write().await;
        streams.retain(|s| s.container_id != container_id);

        info!("Stopped log streaming for container {container_id}");
    }

    /// Get all active streams
    pub async fn get_streams(&self) -> Vec<Arc<LogStream>> {
        let streams = self.streams.read().await;
        streams.clone()
    }

    /// Search all logs
    pub async fn search_all_logs(&self, pattern: &str) -> Vec<LogEntry> {
        let streams = self.streams.read().await;
        let mut results = Vec::new();

        for stream in streams.iter() {
            let logs = stream.search_logs(pattern).await;
            results.extend(logs);
        }

        results
    }

    /// Get error logs from all streams
    pub async fn get_all_errors(&self) -> Vec<LogEntry> {
        let streams = self.streams.read().await;
        let mut errors = Vec::new();

        for stream in streams.iter() {
            let logs = stream.get_logs_by_level(LogLevel::Error).await;
            errors.extend(logs);
        }

        errors
    }

    /// Clear all log buffers
    pub async fn clear_all_buffers(&self) {
        let streams = self.streams.read().await;

        for stream in streams.iter() {
            stream.clear_buffer().await;
        }

        info!("Cleared all log buffers");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_parsing() {
        assert_eq!(
            LogLevel::from_line("ERROR: Something went wrong"),
            LogLevel::Error
        );
        assert_eq!(
            LogLevel::from_line("WARN: This is a warning"),
            LogLevel::Warn
        );
        assert_eq!(
            LogLevel::from_line("INFO: Application started"),
            LogLevel::Info
        );
        assert_eq!(
            LogLevel::from_line("DEBUG: Variable x = 5"),
            LogLevel::Debug
        );
        assert_eq!(LogLevel::from_line("Some random log line"), LogLevel::Trace);
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Error > LogLevel::Warn);
        assert!(LogLevel::Warn > LogLevel::Info);
        assert!(LogLevel::Info > LogLevel::Debug);
        assert!(LogLevel::Debug > LogLevel::Trace);
    }

    #[tokio::test]
    async fn test_log_stream_config_default() {
        let config = LogStreamConfig::default();
        assert_eq!(config.buffer_size, 1000);
        assert_eq!(config.batch_size, 100);
        assert!(config.follow);
        assert_eq!(config.min_level, LogLevel::Debug);
    }
}
