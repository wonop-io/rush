use crate::event::OutputEvent;
use crate::formatter::{OutputFormatter, PlainFormatter};
use async_trait::async_trait;
use crossterm::terminal;
use rush_core::error::{Error, Result};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Capabilities of an output sink
#[derive(Debug, Clone, Default)]
pub struct SinkCapabilities {
    pub supports_color: bool,
    pub supports_unicode: bool,
    pub is_interactive: bool,
    pub max_width: Option<usize>,
}

/// Trait for output destinations
#[async_trait]
pub trait OutputSink: Send + Sync {
    /// Write an event to the sink
    async fn write(&mut self, event: OutputEvent) -> Result<()>;

    /// Flush any buffered data
    async fn flush(&mut self) -> Result<()>;

    /// Get sink capabilities
    fn capabilities(&self) -> SinkCapabilities;

    /// Close the sink
    async fn close(&mut self) -> Result<()>;
}

/// Terminal output with rich formatting
pub struct TerminalSink {
    formatter: Box<dyn OutputFormatter>,
    color_enabled: bool,
    layout: TerminalLayout,
    stdout: io::Stdout,
}

/// Terminal layout options
pub enum TerminalLayout {
    /// Traditional line-by-line output
    Linear,

    /// Split screen with multiple panes
    Split { panes: Vec<PaneConfig> },

    /// Dashboard-style with widgets
    Dashboard { widgets: Vec<WidgetConfig> },

    /// Tree view for hierarchical output
    Tree,

    /// Web view (placeholder)
    Web,
}

/// Configuration for a pane in split layout
pub struct PaneConfig {
    pub title: String,
    pub height_ratio: f32,
}

impl PaneConfig {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            height_ratio: 1.0,
        }
    }
}

/// Configuration for a widget in dashboard layout
#[derive(Clone)]
pub struct WidgetConfig {
    pub widget_type: WidgetType,
    pub position: WidgetPosition,
}

#[derive(Clone)]
pub enum WidgetType {
    Log,
    Progress,
    Metrics,
    ComponentTree,
}

#[derive(Clone)]
pub struct WidgetPosition {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl TerminalSink {
    /// Create a new terminal sink with default settings
    pub fn new() -> Self {
        eprintln!("DEBUG sink.rs: Creating new TerminalSink with Linear layout");
        Self {
            formatter: Box::new(PlainFormatter::default()),
            color_enabled: atty::is(atty::Stream::Stdout),
            layout: TerminalLayout::Linear,
            stdout: io::stdout(),
        }
    }

    /// Set the formatter
    pub fn with_formatter(mut self, formatter: Box<dyn OutputFormatter>) -> Self {
        self.formatter = formatter;
        self
    }

    /// Set the layout
    pub fn with_layout(mut self, layout: TerminalLayout) -> Self {
        eprintln!(
            "DEBUG sink.rs: Setting layout to: {:?}",
            match &layout {
                TerminalLayout::Linear => "Linear",
                TerminalLayout::Split { .. } => "Split",
                TerminalLayout::Dashboard { .. } => "Dashboard",
                TerminalLayout::Tree => "Tree",
                TerminalLayout::Web => "Web",
            }
        );
        self.layout = layout;
        self
    }

    /// Write to linear layout
    fn write_linear(&mut self, event: &OutputEvent) -> Result<()> {
        let formatted = self.formatter.format(event);
        writeln!(self.stdout, "{formatted}").map_err(Error::Io)?;
        Ok(())
    }

    /// Write to split layout
    fn write_split(&mut self, event: &OutputEvent) -> Result<()> {
        let formatted = self.formatter.format(event);
        let phase_prefix = match &event.phase {
            crate::event::ExecutionPhase::CompileTime { .. } => "[BUILD]  ",
            crate::event::ExecutionPhase::Runtime { .. } => "[RUNTIME]",
            crate::event::ExecutionPhase::System { .. } => "[SYSTEM] ",
        };
        writeln!(self.stdout, "{phase_prefix} {formatted}").map_err(Error::Io)?;
        Ok(())
    }

    /// Write to dashboard layout
    fn write_dashboard(&mut self, event: &OutputEvent) -> Result<()> {
        let formatted = self.formatter.format(event);
        // Dashboard mode would normally show a TUI, for now just format nicely
        writeln!(self.stdout, "{formatted}").map_err(Error::Io)?;
        Ok(())
    }

    /// Write to tree layout
    fn write_tree(&mut self, event: &OutputEvent) -> Result<()> {
        let formatted = self.formatter.format(event);
        let indent = "  "; // Simple indentation for now
        writeln!(self.stdout, "{indent}{formatted}").map_err(Error::Io)?;
        Ok(())
    }

    /// Write for web mode
    fn write_web(&mut self, event: &OutputEvent) -> Result<()> {
        let formatted = self.formatter.format(event);
        // Web mode would normally serve via HTTP, for now just output
        writeln!(self.stdout, "{formatted}").map_err(Error::Io)?;
        Ok(())
    }
}

impl Default for TerminalSink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OutputSink for TerminalSink {
    async fn write(&mut self, event: OutputEvent) -> Result<()> {
        match &self.layout {
            TerminalLayout::Linear => self.write_linear(&event),
            TerminalLayout::Split { .. } => self.write_split(&event),
            TerminalLayout::Dashboard { .. } => self.write_dashboard(&event),
            TerminalLayout::Tree => self.write_tree(&event),
            TerminalLayout::Web => self.write_web(&event),
        }
    }

    async fn flush(&mut self) -> Result<()> {
        self.stdout.flush().map_err(Error::Io)?;
        Ok(())
    }

    fn capabilities(&self) -> SinkCapabilities {
        let (width, _height) = terminal::size().unwrap_or((80, 24));

        SinkCapabilities {
            supports_color: self.color_enabled,
            supports_unicode: true, // Assume UTF-8 support
            is_interactive: atty::is(atty::Stream::Stdout),
            max_width: Some(width as usize),
        }
    }

    async fn close(&mut self) -> Result<()> {
        self.flush().await
    }
}

/// File format for output
#[derive(Clone)]
pub enum FileFormat {
    PlainText,
    Json,
    JsonLines,
}

/// Rotation configuration for file output
#[derive(Clone)]
pub struct RotationConfig {
    pub max_size_bytes: u64,
    pub max_files: usize,
}

/// Compression type for file output
#[derive(Clone)]
pub enum CompressionType {
    None,
    Gzip,
}

/// File-based output
pub struct FileSink {
    path: PathBuf,
    file: Option<File>,
    formatter: Box<dyn OutputFormatter>,
    format: FileFormat,
    rotation: Option<RotationConfig>,
    current_size: u64,
}

impl FileSink {
    /// Create a new file sink
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Filesystem(format!("Failed to create log directory: {e}")))?;
        }

        Ok(Self {
            path,
            file: None,
            formatter: Box::new(PlainFormatter::default()),
            format: FileFormat::PlainText,
            rotation: None,
            current_size: 0,
        })
    }

    /// Set the formatter
    pub fn with_formatter(mut self, formatter: Box<dyn OutputFormatter>) -> Self {
        self.formatter = formatter;
        self
    }

    /// Set the format
    pub fn with_format(mut self, format: FileFormat) -> Self {
        self.format = format;
        self
    }

    /// Open or create the file
    fn ensure_file_open(&mut self) -> Result<()> {
        if self.file.is_none() {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .map_err(Error::Io)?;

            self.current_size = file.metadata().map(|m| m.len()).unwrap_or(0);

            self.file = Some(file);
        }
        Ok(())
    }

    /// Check if rotation is needed
    fn should_rotate(&self) -> bool {
        if let Some(rotation) = &self.rotation {
            self.current_size >= rotation.max_size_bytes
        } else {
            false
        }
    }

    /// Rotate the log file
    fn rotate(&mut self) -> Result<()> {
        // Close current file
        if let Some(mut file) = self.file.take() {
            file.flush().map_err(Error::Io)?;
        }

        // Rename existing files
        if let Some(rotation) = &self.rotation {
            for i in (1..rotation.max_files).rev() {
                let from = if i == 1 {
                    self.path.clone()
                } else {
                    self.path.with_extension(format!("{}.log", i - 1))
                };

                let to = self.path.with_extension(format!("{i}.log"));

                if from.exists() {
                    std::fs::rename(&from, &to).ok();
                }
            }
        }

        // Reset for new file
        self.current_size = 0;
        self.file = None;
        self.ensure_file_open()
    }
}

#[async_trait]
impl OutputSink for FileSink {
    async fn write(&mut self, event: OutputEvent) -> Result<()> {
        self.ensure_file_open()?;

        let formatted = match self.format {
            FileFormat::PlainText => self.formatter.format(&event),
            FileFormat::Json => serde_json::to_string_pretty(&event)
                .map_err(|e| Error::Internal(format!("Failed to serialize event: {e}")))?,
            FileFormat::JsonLines => serde_json::to_string(&event)
                .map_err(|e| Error::Internal(format!("Failed to serialize event: {e}")))?,
        };

        let bytes = format!("{formatted}\n").into_bytes();
        self.current_size += bytes.len() as u64;

        if let Some(file) = &mut self.file {
            file.write_all(&bytes).map_err(Error::Io)?;
        }

        if self.should_rotate() {
            self.rotate()?;
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        if let Some(file) = &mut self.file {
            file.flush().map_err(Error::Io)?;
        }
        Ok(())
    }

    fn capabilities(&self) -> SinkCapabilities {
        SinkCapabilities {
            supports_color: false,
            supports_unicode: true,
            is_interactive: false,
            max_width: None,
        }
    }

    async fn close(&mut self) -> Result<()> {
        self.flush().await?;
        self.file = None;
        Ok(())
    }
}

/// Policy for handling buffer overflow
#[derive(Clone)]
pub enum OverflowPolicy {
    DropOldest,
    DropNewest,
    Block,
}

/// In-memory buffer sink
pub struct BufferSink {
    capacity: usize,
    events: VecDeque<OutputEvent>,
    overflow_policy: OverflowPolicy,
}

impl BufferSink {
    /// Create a new buffer sink
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            events: VecDeque::with_capacity(capacity),
            overflow_policy: OverflowPolicy::DropOldest,
        }
    }

    /// Set overflow policy
    pub fn with_overflow_policy(mut self, policy: OverflowPolicy) -> Self {
        self.overflow_policy = policy;
        self
    }

    /// Get all events
    pub fn events(&self) -> &VecDeque<OutputEvent> {
        &self.events
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Get the number of events
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[async_trait]
impl OutputSink for BufferSink {
    async fn write(&mut self, event: OutputEvent) -> Result<()> {
        if self.events.len() >= self.capacity {
            match self.overflow_policy {
                OverflowPolicy::DropOldest => {
                    self.events.pop_front();
                }
                OverflowPolicy::DropNewest => {
                    return Ok(()); // Don't add the new event
                }
                OverflowPolicy::Block => {
                    // In a real implementation, we might wait
                    return Err(Error::Other("Buffer full".to_string()));
                }
            }
        }

        self.events.push_back(event);
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(()) // Nothing to flush for in-memory buffer
    }

    fn capabilities(&self) -> SinkCapabilities {
        SinkCapabilities::default()
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A sink that can be shared across threads
pub struct SharedSink {
    inner: Arc<Mutex<Box<dyn OutputSink>>>,
}

impl SharedSink {
    /// Create a new shared sink
    pub fn new(sink: Box<dyn OutputSink>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(sink)),
        }
    }
}

impl Clone for SharedSink {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[async_trait]
impl OutputSink for SharedSink {
    async fn write(&mut self, event: OutputEvent) -> Result<()> {
        let mut sink = self.inner.lock().await;
        sink.write(event).await
    }

    async fn flush(&mut self) -> Result<()> {
        let mut sink = self.inner.lock().await;
        sink.flush().await
    }

    fn capabilities(&self) -> SinkCapabilities {
        // We can't access the inner sink synchronously, so return defaults
        SinkCapabilities::default()
    }

    async fn close(&mut self) -> Result<()> {
        let mut sink = self.inner.lock().await;
        sink.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OutputSource, OutputStream};

    #[tokio::test]
    async fn test_buffer_sink() {
        let mut sink = BufferSink::new(3);

        for i in 0..5 {
            let source = OutputSource::new("test", "container");
            let event = OutputEvent::runtime(
                source,
                OutputStream::stdout(format!("event {i}").into_bytes()),
                None,
            );
            sink.write(event).await.unwrap();
        }

        // Should only have 3 events (oldest dropped)
        assert_eq!(sink.len(), 3);
    }

    #[tokio::test]
    async fn test_file_sink() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("test.log");

        let mut sink = FileSink::new(&log_path).unwrap();

        let source = OutputSource::new("test", "container");
        let event = OutputEvent::runtime(
            source,
            OutputStream::stdout(b"test log entry".to_vec()),
            None,
        );

        sink.write(event).await.unwrap();
        sink.flush().await.unwrap();

        // Verify file was created and contains data
        assert!(log_path.exists());
        let contents = std::fs::read_to_string(&log_path).unwrap();
        assert!(contents.contains("test log entry"));
    }
}
