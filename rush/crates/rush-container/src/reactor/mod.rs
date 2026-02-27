//! Container reactor module
//!
//! This module provides the container lifecycle management and orchestration
//! through an event-driven reactor pattern.

pub mod config;
pub mod docker_integration;
pub mod errors;
pub mod factory;
#[cfg(test)]
pub mod integration_tests;
pub mod modular_core;
pub mod state;
pub mod watcher_integration;

// Re-export main types
pub use config::ContainerReactorConfig;
pub use docker_integration::{
    DockerIntegration, DockerIntegrationBuilder, DockerIntegrationConfig,
};
pub use errors::{ReactorError, ReactorResult};
pub use factory::{
    ModularReactorConfigBuilder, ReactorFactory, ReactorImplementation, ReactorStatusInfo,
};
pub use modular_core::{ModularReactorConfig, Reactor, ReactorStatus};
pub use state::{
    ComponentState, ComponentStatus, ReactorPhase, ReactorState, SharedReactorState, StateError,
};
pub use watcher_integration::{WatcherIntegration, WatcherIntegrationConfig};

// ContainerReactor is now a type alias for Reactor in lib.rs
