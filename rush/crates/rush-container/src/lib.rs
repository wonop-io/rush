//! Rush Container - Docker and container orchestration

pub mod build;
pub mod dev_environment;
pub mod docker;
pub mod image_builder;
pub mod lifecycle;
pub mod network;
pub mod reactor;
pub mod service;
pub mod simple_output;
pub mod status;
pub mod stripe_handler;
pub mod watcher;

#[cfg(test)]
pub mod tests;

pub use dev_environment::DevEnvironment;
pub use docker::{DockerCliClient, DockerClient, DockerImage, DockerService};
pub use image_builder::{BuildConfig, ImageBuilder};
pub use reactor::{ContainerReactor, ContainerReactorConfig};
pub use service::{ContainerService, ServiceCollection, ServiceConfig, ServicesSpec};
pub use status::Status;

// Re-export build types
pub use build::BuildProcessor;

// Type aliases
pub type ContainerHandle = docker::DockerService;
