//! Container status tracking
//!
//! This module provides a Status enum for tracking the lifecycle state of containers.

/// Represents the current status of a container
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Container is waiting to be started
    Awaiting,
    /// Container is starting up
    InProgress,
    /// Container has completed startup
    StartupCompleted,
    /// Container is being reinitialized
    Reinitializing,
    /// Container has finished with exit code
    Finished(i32),
    /// Container is being terminated
    Terminate,
}
