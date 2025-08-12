//! Rush Container - Docker and container orchestration

pub mod build;
pub mod docker;
pub mod image_builder;
pub mod lifecycle;
pub mod network;
pub mod reactor;
pub mod watcher;

pub use docker::{DockerClient, DockerCliClient, DockerService};
pub use image_builder::ImageBuilder;
pub use reactor::{ContainerReactor, ContainerReactorConfig};

// Re-export build types
pub use build::processor::BuildProcessor;

pub type ContainerService = docker::ContainerService;
pub type ServiceCollection = std::collections::HashMap<String, Vec<std::sync::Arc<ContainerService>>>;
