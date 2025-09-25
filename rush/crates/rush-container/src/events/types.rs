//! Event type definitions for the container system
//!
//! This module defines all events that can occur in the container lifecycle,
//! build process, and file watching system.

use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Core events that can occur in the container system
#[derive(Debug, Clone)]
pub enum ContainerEvent {
    /// A build has started for a component
    BuildStarted {
        component: String,
        timestamp: Instant,
    },

    /// A build has completed (successfully or with error)
    BuildCompleted {
        component: String,
        success: bool,
        duration: Duration,
        error: Option<String>,
    },

    /// A container has been started
    ContainerStarted {
        component: String,
        container_id: String,
        timestamp: Instant,
    },

    /// A container has stopped
    ContainerStopped {
        component: String,
        container_id: String,
        exit_code: Option<i32>,
        reason: StopReason,
    },

    /// Container health status changed
    ContainerHealthChanged {
        component: String,
        container_id: String,
        healthy: bool,
    },

    /// Files have changed triggering a potential rebuild
    FilesChanged {
        component: String,
        paths: Vec<PathBuf>,
        timestamp: Instant,
    },

    /// File changes detected (new watcher system)
    FileChangesDetected {
        files: Vec<PathBuf>,
        components: Vec<String>,
    },

    /// A rebuild has been triggered
    RebuildTriggered {
        component: String,
        reason: RebuildReason,
    },

    /// Network has been created or verified
    NetworkReady { network_name: String },

    /// Reactor has started
    ReactorStarted,

    /// Shutdown has been initiated
    ShutdownInitiated { reason: ShutdownReason },

    /// An error occurred that doesn't fit other categories
    Error {
        component: Option<String>,
        error: String,
        recoverable: bool,
    },
}

/// Reasons why a container stopped
#[derive(Debug, Clone)]
pub enum StopReason {
    /// Normal shutdown requested
    Shutdown,
    /// Container exited on its own
    Exited,
    /// Container was killed due to error
    Killed,
    /// Container crashed
    Crashed,
    /// Stopped for rebuild
    Rebuild,
}

/// Reasons for triggering a rebuild
#[derive(Debug, Clone)]
pub enum RebuildReason {
    /// Source files changed
    FileChange(Vec<PathBuf>),
    /// Manual rebuild requested
    Manual,
    /// Build failed, retrying
    RetryAfterFailure,
    /// Initial build
    Initial,
}

/// Reasons for system shutdown
#[derive(Debug, Clone)]
pub enum ShutdownReason {
    /// User requested shutdown (e.g., Ctrl+C)
    UserRequested,
    /// Error caused shutdown
    Error(String),
    /// All containers exited
    AllContainersExited,
    /// Timeout reached
    Timeout,
}

/// Event metadata
#[derive(Debug, Clone)]
pub struct EventMetadata {
    /// Unique event ID
    pub id: String,
    /// When the event was created
    pub timestamp: Instant,
    /// Source component that generated the event
    pub source: String,
    /// Event severity level
    pub level: EventLevel,
}

/// Event severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

/// Complete event with metadata
#[derive(Debug, Clone)]
pub struct Event {
    pub metadata: EventMetadata,
    pub payload: ContainerEvent,
}

impl Event {
    /// Create a new event with the given payload
    pub fn new(source: impl Into<String>, payload: ContainerEvent) -> Self {
        let level = match &payload {
            ContainerEvent::Error { .. } => EventLevel::Error,
            ContainerEvent::ShutdownInitiated { .. } => EventLevel::Warning,
            ContainerEvent::ContainerStopped { reason, .. } => match reason {
                StopReason::Crashed | StopReason::Killed => EventLevel::Warning,
                _ => EventLevel::Info,
            },
            _ => EventLevel::Info,
        };

        Self {
            metadata: EventMetadata {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: Instant::now(),
                source: source.into(),
                level,
            },
            payload,
        }
    }

    /// Create an error event
    pub fn error(source: impl Into<String>, error: impl Into<String>, recoverable: bool) -> Self {
        Self::new(
            source,
            ContainerEvent::Error {
                component: None,
                error: error.into(),
                recoverable,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            "test",
            ContainerEvent::BuildStarted {
                component: "frontend".to_string(),
                timestamp: Instant::now(),
            },
        );

        assert_eq!(event.metadata.source, "test");
        assert_eq!(event.metadata.level, EventLevel::Info);
        assert!(!event.metadata.id.is_empty());
    }

    #[test]
    fn test_error_event() {
        let event = Event::error("test", "Something went wrong", true);

        assert_eq!(event.metadata.level, EventLevel::Error);
        if let ContainerEvent::Error {
            error, recoverable, ..
        } = event.payload
        {
            assert_eq!(error, "Something went wrong");
            assert!(recoverable);
        } else {
            panic!("Expected Error event");
        }
    }

    #[test]
    fn test_event_level_assignment() {
        // Error events should have Error level
        let error_event = Event::new(
            "test",
            ContainerEvent::Error {
                component: None,
                error: "test".to_string(),
                recoverable: false,
            },
        );
        assert_eq!(error_event.metadata.level, EventLevel::Error);

        // Crashed containers should have Warning level
        let crash_event = Event::new(
            "test",
            ContainerEvent::ContainerStopped {
                component: "test".to_string(),
                container_id: "123".to_string(),
                exit_code: Some(1),
                reason: StopReason::Crashed,
            },
        );
        assert_eq!(crash_event.metadata.level, EventLevel::Warning);
    }
}
