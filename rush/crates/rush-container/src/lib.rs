//! Rush Container - Docker and container orchestration
//! 
//! This crate provides container lifecycle management through the Reactor pattern.
//! The primary implementation uses a modular architecture with separate components
//! for build orchestration, lifecycle management, file watching, and Docker integration.

pub mod build;
pub mod dev_environment;
pub mod docker;
pub mod events;
pub mod image_builder;
pub mod kubernetes;
pub mod lifecycle;
pub mod reactor;
pub mod service;
pub mod simple_output;
pub mod status;
pub mod stripe_handler;
pub mod watcher;

// TODO: Re-enable when test utils are updated to match current interfaces
// #[cfg(test)]
// pub mod test_utils;

#[cfg(test)]
pub mod tests;

pub use dev_environment::DevEnvironment;
pub use docker::{DockerCliClient, DockerClient, DockerImage, DockerService};
pub use image_builder::{BuildConfig, ImageBuilder};
pub use reactor::{ContainerReactor, ContainerReactorConfig};
pub use reactor::modular_core::Reactor;
pub use service::{ContainerService, ServiceCollection, ServiceConfig, ServicesSpec};
pub use status::Status;

// Type aliases
pub type ContainerHandle = docker::DockerService;
