//! Container status types

use std::fmt;

/// Status of a Docker container
#[derive(Debug, Clone, PartialEq)]
pub enum ContainerStatus {
    /// Container was just created
    Created,
    /// Container is running
    Running,
    /// Container is restarting
    Restarting,
    /// Container has exited with a status code
    Exited(i32),
    /// Container is paused
    Paused,
    /// Container is dead
    Dead,
    /// Container status couldn't be determined
    Unknown,
}

impl ContainerStatus {
    /// Check if the container is running
    pub fn is_running(&self) -> bool {
        matches!(self, ContainerStatus::Running)
    }
}

impl fmt::Display for ContainerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContainerStatus::Created => write!(f, "Created"),
            ContainerStatus::Running => write!(f, "Running"),
            ContainerStatus::Restarting => write!(f, "Restarting"),
            ContainerStatus::Exited(code) => write!(f, "Exited({})", code),
            ContainerStatus::Paused => write!(f, "Paused"),
            ContainerStatus::Dead => write!(f, "Dead"),
            ContainerStatus::Unknown => write!(f, "Unknown"),
        }
    }
}