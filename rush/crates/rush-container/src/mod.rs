mod build;
mod docker;
mod image_builder;
mod lifecycle;
mod network;
mod reactor;
mod service;
mod status;
mod watcher;

pub use build::{BuildError, BuildProcessor};
pub use docker::{
    ContainerStatus, DockerCliClient, DockerClient, DockerService, DockerServiceConfig,
};
pub use image_builder::{BuildConfig, ImageBuilder};
pub use lifecycle::{launch_containers, LifecycleManager, LifecycleMonitor, ShutdownManager};
pub use network::DockerNetwork;
pub use reactor::{ContainerReactor, ContainerReactorConfig};
pub use service::{ContainerService, ServiceCollection, ServiceConfig, ServicesSpec};
pub use status::Status;

// Re-export container-related types and functions
pub type Container = docker::DockerService;
pub type ContainerHandle = docker::DockerService;
pub type DockerImage = image_builder::ImageBuilder;

// Re-export functions
pub use network::setup_network;
pub use watcher::{create_component_matcher, setup_file_watcher, ChangeProcessor, WatcherConfig};
