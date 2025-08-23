//! Container status types

use std::fmt;

/// Status of a Docker container
#[derive(Debug, Clone, PartialEq)]
pub enum ContainerStatus {
    /// Container is running
    Running,
    /// Container has exited with a status code
    Exited(i32),
    /// Container status couldn't be determined
    Unknown,
}

impl fmt::Display for ContainerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContainerStatus::Running => write!(f, "Running"),
            ContainerStatus::Exited(code) => write!(f, "Exited({})", code),
            ContainerStatus::Unknown => write!(f, "Unknown"),
        }
    }
}