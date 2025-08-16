use crate::{OutputSource, OutputStream};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Enhanced output event with comprehensive metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEvent {
    /// Unique identifier for this event
    pub id: Uuid,

    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,

    /// Source information
    pub source: OutputSource,

    /// Execution phase
    pub phase: ExecutionPhase,

    /// The actual output data
    pub stream: OutputStream,

    /// Additional metadata
    pub metadata: OutputMetadata,

    /// Correlation ID for grouping related events
    pub correlation_id: Option<Uuid>,
}

impl OutputEvent {
    /// Create a new output event
    pub fn new(source: OutputSource, phase: ExecutionPhase, stream: OutputStream) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            source,
            phase,
            stream,
            metadata: OutputMetadata::default(),
            correlation_id: None,
        }
    }

    /// Create an event with a correlation ID
    pub fn with_correlation(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Add metadata to the event
    pub fn with_metadata(mut self, metadata: OutputMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Create a compile-time event
    pub fn compile_time(
        source: OutputSource,
        stage: CompileStage,
        target: String,
        stream: OutputStream,
    ) -> Self {
        Self::new(
            source,
            ExecutionPhase::CompileTime { stage, target },
            stream,
        )
    }

    /// Create a runtime event
    pub fn runtime(
        source: OutputSource,
        stream: OutputStream,
        container_id: Option<String>,
    ) -> Self {
        Self::new(
            source,
            ExecutionPhase::Runtime {
                container_id,
                process_id: None,
            },
            stream,
        )
    }

    /// Create a system event
    pub fn system(source: OutputSource, subsystem: String, stream: OutputStream) -> Self {
        Self::new(source, ExecutionPhase::System { subsystem }, stream)
    }
}

/// Execution phase of the output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionPhase {
    /// Build/compilation phase
    CompileTime { stage: CompileStage, target: String },

    /// Runtime/execution phase
    Runtime {
        container_id: Option<String>,
        process_id: Option<u32>,
    },

    /// System-level events
    System { subsystem: String },
}

impl ExecutionPhase {
    /// Check if this is a compile-time phase
    pub fn is_compile_time(&self) -> bool {
        matches!(self, ExecutionPhase::CompileTime { .. })
    }

    /// Check if this is a runtime phase
    pub fn is_runtime(&self) -> bool {
        matches!(self, ExecutionPhase::Runtime { .. })
    }

    /// Check if this is a system phase
    pub fn is_system(&self) -> bool {
        matches!(self, ExecutionPhase::System { .. })
    }
}

/// Compilation stages
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompileStage {
    Dependency,
    Compilation,
    Linking,
    Optimization,
    Packaging,
    DockerBuild,
}

impl CompileStage {
    /// Get a human-readable name for the stage
    pub fn as_str(&self) -> &'static str {
        match self {
            CompileStage::Dependency => "Dependencies",
            CompileStage::Compilation => "Compiling",
            CompileStage::Linking => "Linking",
            CompileStage::Optimization => "Optimizing",
            CompileStage::Packaging => "Packaging",
            CompileStage::DockerBuild => "Docker Build",
        }
    }
}

/// Log levels for output events
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Convert from the log crate's Level
    pub fn from_log_level(level: log::Level) -> Self {
        match level {
            log::Level::Trace => LogLevel::Trace,
            log::Level::Debug => LogLevel::Debug,
            log::Level::Info => LogLevel::Info,
            log::Level::Warn => LogLevel::Warn,
            log::Level::Error => LogLevel::Error,
        }
    }
}

/// Performance metrics for an event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// CPU usage percentage (0-100)
    pub cpu_usage: Option<f32>,

    /// Memory usage in bytes
    pub memory_bytes: Option<u64>,

    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
}

/// Additional metadata for output events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct OutputMetadata {
    /// Log level if applicable
    pub level: Option<LogLevel>,

    /// Key-value pairs for additional context
    pub tags: HashMap<String, String>,

    /// Whether this output is from a retry attempt
    pub retry_count: u32,

    /// Performance metrics
    pub metrics: Option<PerformanceMetrics>,
}


impl OutputMetadata {
    /// Create metadata with a log level
    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.level = Some(level);
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Set retry count
    pub fn with_retry(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }

    /// Set performance metrics
    pub fn with_metrics(mut self, metrics: PerformanceMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::OutputStreamType;

    #[test]
    fn test_output_event_creation() {
        let source = OutputSource::new("test", "container");
        let stream = OutputStream::stdout(b"Hello, World!\n".to_vec());
        let event = OutputEvent::new(
            source.clone(),
            ExecutionPhase::Runtime {
                container_id: Some("abc123".to_string()),
                process_id: None,
            },
            stream.clone(),
        );

        assert_eq!(event.source.name, "test");
        assert!(event.phase.is_runtime());
        assert_eq!(event.stream.stream_type, OutputStreamType::Stdout);
    }

    #[test]
    fn test_compile_time_event() {
        let source = OutputSource::new("backend", "rust");
        let stream = OutputStream::stdout(b"Compiling backend...\n".to_vec());
        let event = OutputEvent::compile_time(
            source,
            CompileStage::Compilation,
            "backend".to_string(),
            stream,
        );

        assert!(event.phase.is_compile_time());
        if let ExecutionPhase::CompileTime { stage, target } = event.phase {
            assert_eq!(stage, CompileStage::Compilation);
            assert_eq!(target, "backend");
        }
    }

    #[test]
    fn test_metadata_builder() {
        let metadata = OutputMetadata::default()
            .with_level(LogLevel::Info)
            .with_tag("component", "frontend")
            .with_tag("version", "1.0.0")
            .with_retry(2);

        assert_eq!(metadata.level, Some(LogLevel::Info));
        assert_eq!(
            metadata.tags.get("component"),
            Some(&"frontend".to_string())
        );
        assert_eq!(metadata.retry_count, 2);
    }
}
