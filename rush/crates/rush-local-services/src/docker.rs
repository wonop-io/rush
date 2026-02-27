//! Docker client re-exports for local services
//!
//! This module re-exports the Docker client trait from rush-docker
//! to avoid circular dependencies with rush-container.

// Re-export the DockerClient trait and ContainerStatus from rush-docker
pub use rush_docker::{ContainerStatus, DockerClient};
