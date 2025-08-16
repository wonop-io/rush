//! Simplified output system for Rush
//! 
//! This module provides a clean, simple abstraction for handling output
//! from containers and build processes.

use async_trait::async_trait;
use chrono::{DateTime, Local, Utc};
use colored::*;
use std::io::{self, Write};
use rush_core::error::Result;

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
    /// The phase of execution
    pub phase: LogPhase,
}

/// The phase of execution for a log entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogPhase {
    Build,
    Runtime,
    System,
}

impl LogEntry {
    /// Create a new build log entry
    pub fn build(component: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            content: content.into(),
            timestamp: Utc::now(),
            is_error: false,
            phase: LogPhase::Build,
        }
    }

    /// Create a new runtime log entry
    pub fn runtime(component: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            content: content.into(),
            timestamp: Utc::now(),
            is_error: false,
            phase: LogPhase::Runtime,
        }
    }

    /// Create a new system log entry
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            component: "system".to_string(),
            content: content.into(),
            timestamp: Utc::now(),
            is_error: false,
            phase: LogPhase::System,
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
            entry.timestamp
                .with_timezone(&Local)
                .format("%H:%M:%S")
                .to_string()
                .bright_black()
                .to_string()
        } else {
            String::new()
        };

        let component_color = self.get_component_color(&entry.component);
        let component = entry.component.color(component_color);

        let content = if entry.is_error {
            entry.content.red().to_string()
        } else {
            // Preserve any existing ANSI codes in the content
            entry.content.clone()
        };

        if self.show_timestamp {
            format!("{} {} | {}", timestamp, component, content)
        } else {
            format!("{} | {}", component, content)
        }
    }
}

#[async_trait]
impl Sink for StdoutSink {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        let formatted = self.format_entry(&entry);
        writeln!(self.stdout, "{}", formatted)?;
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
            entry.timestamp
                .with_timezone(&Local)
                .format("%H:%M:%S")
                .to_string()
        } else {
            String::new()
        };

        let content = entry.content.trim_end();

        if self.show_timestamp {
            format!("{} {} | {}", timestamp, entry.component, content)
        } else {
            format!("{} | {}", entry.component, content)
        }
    }
}

#[async_trait]
impl Sink for NoColorStdoutSink {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        let formatted = self.format_entry(&entry);
        writeln!(self.stdout, "{}", formatted)?;
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
            entry.timestamp
                .with_timezone(&Local)
                .format("%H:%M:%S")
                .to_string()
                .bright_black()
                .to_string()
        } else {
            String::new()
        };

        let phase_label = match entry.phase {
            LogPhase::Build => "[BUILD]  ".yellow().to_string(),
            LogPhase::Runtime => "[RUNTIME]".green().to_string(),
            LogPhase::System => "[SYSTEM] ".magenta().to_string(),
        };

        let component_color = self.get_component_color(&entry.component);
        let component = entry.component.color(component_color);

        let content = if entry.is_error {
            entry.content.red().to_string()
        } else {
            entry.content.clone()
        };

        if self.show_timestamp {
            format!("{} {} {} | {}", timestamp, phase_label, component, content)
        } else {
            format!("{} {} | {}", phase_label, component, content)
        }
    }
}

#[async_trait]
impl Sink for SplitSink {
    async fn write(&mut self, entry: LogEntry) -> Result<()> {
        let formatted = self.format_entry(&entry);
        writeln!(self.stdout, "{}", formatted)?;
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
                "phase": match entry.phase {
                    LogPhase::Build => "build",
                    LogPhase::Runtime => "runtime",
                    LogPhase::System => "system",
                }
            });
            writeln!(self.file, "{}", json)?;
        } else {
            let timestamp = entry.timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S");
            writeln!(self.file, "{} {} | {}", timestamp, entry.component, entry.content)?;
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
        _ if no_color => Box::new(NoColorStdoutSink::new()),
        _ => Box::new(StdoutSink::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_entry_creation() {
        let entry = LogEntry::build("frontend", "Building...");
        assert_eq!(entry.component, "frontend");
        assert_eq!(entry.phase, LogPhase::Build);
        assert!(!entry.is_error);

        let error_entry = LogEntry::runtime("backend", "Error!").as_error();
        assert!(error_entry.is_error);
    }

    #[tokio::test]
    async fn test_stdout_sink() {
        let mut sink = StdoutSink::new();
        let entry = LogEntry::runtime("test", "Hello, World!");
        // This would write to stdout in a real scenario
        assert!(sink.write(entry).await.is_ok());
    }
}