//! Rush Container - Docker and container orchestration
//!
//! This crate provides container lifecycle management through the Reactor pattern.
//! The primary implementation uses a modular architecture with separate components
//! for build orchestration, lifecycle management, file watching, and Docker integration.

pub mod build;
pub mod dependency_graph;
pub mod dev_environment;
pub mod docker;
pub mod events;
pub mod git_ops;
pub mod health_check_manager;
pub mod image_builder;
pub mod kubernetes;
pub mod lifecycle;
pub mod metrics;
pub mod network;
pub mod profiling;
pub mod reactor;
pub mod recovery;
pub mod service;
pub mod simple_docker;
pub mod simple_lifecycle;
pub mod simple_output;
pub mod status;
pub mod stripe_handler;
pub mod tagging;
pub mod watcher;

// Testing modules
#[cfg(test)]
pub mod testing;

// TODO: Re-enable when test utils are updated to match current interfaces
// #[cfg(test)]
// pub mod test_utils;

#[cfg(test)]
pub mod tests;

#[cfg(test)]
mod naming_test;

pub use dev_environment::DevEnvironment;
pub use docker::{DockerCliClient, DockerClient, DockerImage, DockerService, ManagedDockerService};
pub use image_builder::{BuildConfig, ImageBuilder};
// Primary reactor export
pub use reactor::modular_core::Reactor;
pub use reactor::ContainerReactorConfig;
// Simple Docker implementation
pub use simple_docker::{RunOptions, SimpleDocker};
pub use simple_lifecycle::{SimpleLifecycleConfig, SimpleLifecycleManager};

// Type alias for backward compatibility
pub type ContainerReactor = Reactor;
pub use service::{
    ContainerService, ManagedContainerService, ServiceCollection, ServiceConfig, ServicesSpec,
};
pub use status::Status;

// Type aliases
pub type ContainerHandle = docker::DockerService;
