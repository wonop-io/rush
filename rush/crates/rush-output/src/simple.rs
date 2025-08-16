//! Simplified output system for Rush
//!
//! This module provides a clean, simple abstraction for handling output
//! from containers and build processes.

use async_trait::async_trait;
use chrono::{DateTime, Local, Utc};
use colored::*;
use rush_core::error::Result;
use std::io::{self, Write};

/// A log entry from a container or build process
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// The component that generated this log (e.g., "frontend", "backend")
    pub component: String,
    /// The content of the log message
    pub content: String,
    /// When the log was generated
    pub timestamp: DateTime<Utc>,
    /// Whether this is from stderr (vs stdout)
    pub is_error: bool,
    /// The origin of the log
    pub log_origin: LogOrigin,
}

/// The origin of a log entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogOrigin {
    Script,
    Docker,
    System,
}

impl LogEntry {
    /// Create a new script log entry
    pub fn script(component: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            content: content.into(),
            timestamp: Utc::now(),
            is_error: false,
            log_origin: LogOrigin::Script,
        }
    }

    /// Create a new docker log entry
    pub fn docker(component: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            content: content.into(),
            timestamp: Utc::now(),
            is_error: false,
            log_origin: LogOrigin::Docker,
        }
    }

    /// Create a new system log entry
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            component: "system".to_string(),
            content: content.into(),
            timestamp: Utc::now(),
            is_error: false,
            log_origin: LogOrigin::System,
        }
    }

    /// Mark this entry as an error
    pub fn as_error(mut self) -> Self {
        self.is_error = true;
        self
    }
}

/// Trait for output sinks
#[async_trait]
pub trait Sink: Send + Sync {
    /// Write a log entry to the sink
    async fn write(&mut self, entry: LogEntry) -> Result<()>;

    /// Flush any buffered output
    async fn flush(&mut self) -> Result<()>;

    /// Close the sink
    async fn close(&mut self) -> Result<()> {
        self.flush().await
    }
}

/// Standard output sink with colors
pub struct StdoutSink {
    stdout: io::Stdout,
    show_timestamp: bool,
    component_colors: std::collections::HashMap<String, Color>,
    next_color_index: usize,
}

impl Default for StdoutSink {
    fn default() -> Self {
        Self::new()
    }
}

impl StdoutSink {
    /// Create a new stdout sink
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            show_timestamp: true,
            component_colors: std::collections::HashMap::new(),
            next_color_index: 0,
        }
    }

    /// Set whether to show timestamps
    pub fn with_timestamps(mut self, show: bool) -> Self {
        self.show_timestamp = show;
        self
    }

    /// Get a color for a component
    fn get_component_color(&mut self, component: &str) -> Color {
        if let Some(color) = self.component_colors.get(component) {
            *color
        } else {
            // Assign colors in a predictable order
            let colors = [
                Color::Cyan,
                Color::Magenta,
                Color::Yellow,
                Color::Blue,
                Color::Green,
                Color::Red,
            ];
            let color = colors[self.next_color_index % colors.len()];
            self.next_color_index += 1;
            self.component_colors.insert(component.to_string(), color);
            color
        }
    }

    /// Format a log entry with colors
    fn format_entry(&mut self, entry: &LogEntry) -> String {
        let timestamp = if self.show_timestamp {
            entry
                .timestamp
                .with_timezone(&Local)
                .format("%H:%M:%S")
                .to_string()
                .bright_black()
                .to_string()
        } else {
            String::new()
        };

        // Get origin label
        let origin_label = match entry.log_origin {
            LogOrigin::Script => "[SCRIPT]".yellow().to_string(),
            LogOrigin::Docker => "[DOCKER]".green().to_string(),
            LogOrigin::System => "[SYSTEM]".cyan().to_string(),
        };

        // Special formatting for system messages from the "system" component
        if entry.log_origin == LogOrigin::System && entry.component == "system" {
            let content = if entry.is_error {
                entry.content.red().to_string()
            } else {
                // System messages in dim white
                entry.content.bright_black().to_string()
            };
            
            if self.show_timestamp {
                format!("{timestamp} {origin_label} {content}")
            } else {
                format!("{origin_label} {content}")
            }
        } else {
            let component_color = self.get_component_color(&entry.component);
            let component = entry.component.color(component_color);

            let content = if entry.is_error {
                entry.content.red().to_string()
            } else {
                // Preserve any existing ANSI codes in the content
                entry.content.clone()
            };

            if self.show_timestamp {
                format!("{timestamp} {origin_label} {component} | {content}")
            } else {
                format!("{origin_label} {component} | {content}")
            }
        }
    }
}

#[async_trait]
impl Sink for StdoutSink {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        let formatted = self.format_entry(&entry);
        writeln!(self.stdout, "{formatted}")?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.stdout.flush()?;
        Ok(())
    }
}

/// Standard output sink without colors
pub struct NoColorStdoutSink {
    stdout: io::Stdout,
    show_timestamp: bool,
}

impl Default for NoColorStdoutSink {
    fn default() -> Self {
        Self::new()
    }
}

impl NoColorStdoutSink {
    /// Create a new no-color stdout sink
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            show_timestamp: true,
        }
    }

    /// Set whether to show timestamps
    pub fn with_timestamps(mut self, show: bool) -> Self {
        self.show_timestamp = show;
        self
    }

    /// Format a log entry without colors
    fn format_entry(&self, entry: &LogEntry) -> String {
        let timestamp = if self.show_timestamp {
            entry
                .timestamp
                .with_timezone(&Local)
                .format("%H:%M:%S")
                .to_string()
        } else {
            String::new()
        };

        // Get origin label without colors
        let origin_label = match entry.log_origin {
            LogOrigin::Script => "[SCRIPT]",
            LogOrigin::Docker => "[DOCKER]",
            LogOrigin::System => "[SYSTEM]",
        };

        let content = entry.content.trim_end();

        if self.show_timestamp {
            format!("{} {} {} | {}", timestamp, origin_label, entry.component, content)
        } else {
            format!("{} {} | {}", origin_label, entry.component, content)
        }
    }
}

#[async_trait]
impl Sink for NoColorStdoutSink {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        let formatted = self.format_entry(&entry);
        writeln!(self.stdout, "{formatted}")?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.stdout.flush()?;
        Ok(())
    }
}

/// Split view sink that shows build and runtime logs separately
pub struct SplitSink {
    stdout: io::Stdout,
    show_timestamp: bool,
    component_colors: std::collections::HashMap<String, Color>,
    next_color_index: usize,
}

impl Default for SplitSink {
    fn default() -> Self {
        Self::new()
    }
}

impl SplitSink {
    /// Create a new split sink
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
            show_timestamp: true,
            component_colors: std::collections::HashMap::new(),
            next_color_index: 0,
        }
    }

    /// Set whether to show timestamps
    pub fn with_timestamps(mut self, show: bool) -> Self {
        self.show_timestamp = show;
        self
    }

    /// Get a color for a component
    fn get_component_color(&mut self, component: &str) -> Color {
        if let Some(color) = self.component_colors.get(component) {
            *color
        } else {
            let colors = [
                Color::Cyan,
                Color::Magenta,
                Color::Yellow,
                Color::Blue,
                Color::Green,
                Color::Red,
            ];
            let color = colors[self.next_color_index % colors.len()];
            self.next_color_index += 1;
            self.component_colors.insert(component.to_string(), color);
            color
        }
    }

    /// Format a log entry for split view
    fn format_entry(&mut self, entry: &LogEntry) -> String {
        let timestamp = if self.show_timestamp {
            entry
                .timestamp
                .with_timezone(&Local)
                .format("%H:%M:%S")
                .to_string()
                .bright_black()
                .to_string()
        } else {
            String::new()
        };

        let origin_label = match entry.log_origin {
            LogOrigin::Script => "[SCRIPT]".yellow().to_string(),
            LogOrigin::Docker => "[DOCKER]".green().to_string(),
            LogOrigin::System => "[SYSTEM]".cyan().to_string(),
        };

        let component_color = self.get_component_color(&entry.component);
        let component = entry.component.color(component_color);

        let content = if entry.is_error {
            entry.content.red().to_string()
        } else {
            entry.content.clone()
        };

        if self.show_timestamp {
            format!("{timestamp} {origin_label} {component} | {content}")
        } else {
            format!("{origin_label} {component} | {content}")
        }
    }
}

#[async_trait]
impl Sink for SplitSink {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        let formatted = self.format_entry(&entry);
        writeln!(self.stdout, "{formatted}")?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.stdout.flush()?;
        Ok(())
    }
}

/// File sink for logging to a file
pub struct FileSink {
    file: std::fs::File,
    format_json: bool,
}

impl FileSink {
    /// Create a new file sink
    pub fn new(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            file,
            format_json: false,
        })
    }

    /// Set whether to format as JSON
    pub fn with_json(mut self, json: bool) -> Self {
        self.format_json = json;
        self
    }
}

#[async_trait]
impl Sink for FileSink {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        if self.format_json {
            let json = serde_json::json!({
                "timestamp": entry.timestamp.to_rfc3339(),
                "component": entry.component,
                "content": entry.content,
                "is_error": entry.is_error,
                "log_origin": match entry.log_origin {
                    LogOrigin::Script => "script",
                    LogOrigin::Docker => "docker",
                    LogOrigin::System => "system",
                }
            });
            writeln!(self.file, "{json}")?;
        } else {
            let timestamp = entry
                .timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S");
            writeln!(
                self.file,
                "{} {} | {}",
                timestamp, entry.component, entry.content
            )?;
        }
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.file.flush()?;
        Ok(())
    }
}

/// Create a sink based on output format
pub fn create_sink(format: &str, no_color: bool) -> Box<dyn Sink> {
    match format {
        "split" => Box::new(SplitSink::new()),
        "no-color" | "plain" => Box::new(NoColorStdoutSink::new()),
        "mcp" => {
            use crate::mcp_sink::McpSinkBuilder;
            Box::new(McpSinkBuilder::new().build())
        }
        _ if no_color => Box::new(NoColorStdoutSink::new()),
        _ => Box::new(StdoutSink::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_entry_creation() {
        let entry = LogEntry::script("frontend", "Building...");
        assert_eq!(entry.component, "frontend");
        assert_eq!(entry.log_origin, LogOrigin::Script);
        assert!(!entry.is_error);

        let error_entry = LogEntry::docker("backend", "Error!").as_error();
        assert!(error_entry.is_error);
    }

    #[tokio::test]
    async fn test_stdout_sink() {
        let mut sink = StdoutSink::new();
        let entry = LogEntry::docker("test", "Hello, World!");
        // This would write to stdout in a real scenario
        assert!(sink.write(entry).await.is_ok());
    }
}
