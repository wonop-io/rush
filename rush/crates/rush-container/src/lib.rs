//! Rush Container - Docker and container orchestration

pub mod build;
pub mod docker;
pub mod docker_adapter;
pub mod image_builder;
pub mod lifecycle;
pub mod network;
pub mod output_integration;
pub mod reactor;
pub mod service;
pub mod status;
pub mod watcher;

pub use docker::{DockerClient, DockerCliClient, DockerImage, DockerService};
pub use image_builder::{BuildConfig, ImageBuilder};
pub use reactor::{ContainerReactor, ContainerReactorConfig};
pub use service::{ContainerService, ServiceCollection, ServiceConfig, ServicesSpec};
pub use status::Status;

// Re-export build types
pub use build::BuildProcessor;

// Type aliases
pub type ContainerHandle = docker::DockerService;
