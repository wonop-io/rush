//! Docker client re-exports for local services
//!
//! This module re-exports the Docker client trait from rush-core
//! to avoid circular dependencies with rush-container.

// Re-export the DockerClient trait and ContainerStatus from rush-core
pub use rush_core::docker::{ContainerStatus, DockerClient};
