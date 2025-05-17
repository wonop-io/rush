//! Core data types used throughout the Rush CLI
//!
//! This module contains fundamental data types that are used by various
//! components of the Rush CLI.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Represents the build status of a component
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildStatus {
    /// Component has not been built yet
    NotBuilt,
    /// Component is currently building
    Building,
    /// Component was successfully built
    Built,
    /// Component build failed
    Failed,
    /// Component build was skipped
    Skipped,
}

/// Represents a dependency relationship between components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// The name of the component that is depended upon
    pub component_name: String,
    /// Whether this is a build-time dependency
    pub build_time: bool,
    /// Whether this is a run-time dependency
    pub run_time: bool,
}

/// Represents a port mapping for a container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    /// The host port
    pub host: u16,
    /// The container port
    pub container: u16,
    /// Optional protocol (defaults to tcp)
    pub protocol: Option<String>,
}

/// Defines resource requirements for a component
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// CPU limit (in cores or millicores)
    pub cpu: Option<String>,
    /// Memory limit (with unit suffix, e.g., "512Mi")
    pub memory: Option<String>,
    /// Disk space requirement (with unit suffix, e.g., "1Gi")
    pub disk: Option<String>,
}

/// Represents a file path pattern for watching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchPattern {
    /// The glob pattern to match files
    pub pattern: String,
    /// Whether to include files matching this pattern
    pub include: bool,
}

/// Result of a validation operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether the validation was successful
    pub is_valid: bool,
    /// List of validation errors if any
    pub errors: Vec<String>,
    /// List of validation warnings if any
    pub warnings: Vec<String>,
}

/// Represents environment-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    /// Environment variables specific to an environment
    pub variables: HashMap<String, String>,
    /// Resource requirements for the environment
    pub resources: Option<ResourceRequirements>,
    /// Domain configuration for the environment
    pub domain: Option<String>,
}
