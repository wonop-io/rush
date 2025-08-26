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
pub mod migration;
pub mod modular_core;
pub mod state;
pub mod watcher_integration;

// Re-export main types
pub use config::ContainerReactorConfig;
pub use docker_integration::{DockerIntegration, DockerIntegrationConfig, DockerIntegrationBuilder};
pub use errors::{ReactorError, ReactorResult};
pub use factory::{ReactorFactory, ReactorImplementation, ReactorStatusInfo, ModularReactorConfigBuilder};
pub use migration::{ReactorMigrator, MigrationConfig, MigrationStrategy, MigrationStepTracker};
pub use modular_core::{Reactor, ModularReactorConfig, ReactorStatus};
pub use state::{ReactorPhase, ReactorState, SharedReactorState, ComponentState, ComponentStatus, StateError};
pub use watcher_integration::{WatcherIntegration, WatcherIntegrationConfig};

// ContainerReactor is now a type alias for Reactor in lib.rs