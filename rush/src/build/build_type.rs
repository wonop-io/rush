//! Build type definitions for different component build strategies
//!
//! This module defines the various build types that Rush supports, including
//! web applications, binary applications, and container-based deployments.

use serde::{Deserialize, Serialize};

/// Represents the different types of component builds supported by Rush
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum BuildType {
    /// A Trunk-based WebAssembly application build
    TrunkWasm {
        /// The location of the source code
        location: String,
        /// Path to the Dockerfile for containerization
        dockerfile_path: String,
        /// Optional context directory for Docker build
        context_dir: Option<String>,
        /// Whether server-side rendering is enabled
        ssr: bool,
        /// Optional Rust features to enable during build
        features: Option<Vec<String>>,
        /// Optional commands to run before compilation
        precompile_commands: Option<Vec<String>>,
    },

    /// A standard Rust binary application
    RustBinary {
        /// The location of the source code
        location: String,
        /// Path to the Dockerfile for containerization
        dockerfile_path: String,
        /// Optional context directory for Docker build
        context_dir: Option<String>,
        /// Optional Rust features to enable during build
        features: Option<Vec<String>>,
        /// Optional commands to run before compilation
        precompile_commands: Option<Vec<String>>,
    },

    /// A Dixious WebAssembly application build
    DixiousWasm {
        /// The location of the source code
        location: String,
        /// Path to the Dockerfile for containerization
        dockerfile_path: String,
        /// Optional context directory for Docker build
        context_dir: Option<String>,
    },

    /// A generic script-based build
    Script {
        /// The location of the source code
        location: String,
        /// Path to the Dockerfile for containerization
        dockerfile_path: String,
        /// Optional context directory for Docker build
        context_dir: Option<String>,
    },

    /// A Zola static site generator build
    Zola {
        /// The location of the source code
        location: String,
        /// Path to the Dockerfile for containerization
        dockerfile_path: String,
        /// Optional context directory for Docker build
        context_dir: Option<String>,
    },

    /// An mdbook documentation build
    Book {
        /// The location of the source code
        location: String,
        /// Path to the Dockerfile for containerization
        dockerfile_path: String,
        /// Optional context directory for Docker build
        context_dir: Option<String>,
    },

    /// An ingress configuration for routing to multiple components
    Ingress {
        /// List of component names to include in the ingress
        components: Vec<String>,
        /// Path to the Dockerfile for containerization
        dockerfile_path: String,
        /// Optional context directory for Docker build
        context_dir: Option<String>,
    },

    /// A pre-built Docker image with optional configuration
    PureDockerImage {
        /// The full image name with tag (e.g., "nginx:latest")
        image_name_with_tag: String,
        /// Optional command to run in the container
        command: Option<String>,
        /// Optional entrypoint for the container
        entrypoint: Option<String>,
    },

    /// A Kubernetes-only component with no container build
    PureKubernetes,

    /// A Kubernetes installation package
    KubernetesInstallation {
        /// Target namespace for the installation
        namespace: String,
    },
}

impl BuildType {
    /// Returns the location path for the build if applicable
    pub fn location(&self) -> Option<&str> {
        match self {
            BuildType::TrunkWasm { location, .. } => Some(location),
            BuildType::RustBinary { location, .. } => Some(location),
            BuildType::DixiousWasm { location, .. } => Some(location),
            BuildType::Script { location, .. } => Some(location),
            BuildType::Zola { location, .. } => Some(location),
            BuildType::Book { location, .. } => Some(location),
            _ => None,
        }
    }

    /// Returns the dockerfile path for the build if applicable
    pub fn dockerfile_path(&self) -> Option<&str> {
        match self {
            BuildType::TrunkWasm {
                dockerfile_path, ..
            } => Some(dockerfile_path),
            BuildType::RustBinary {
                dockerfile_path, ..
            } => Some(dockerfile_path),
            BuildType::DixiousWasm {
                dockerfile_path, ..
            } => Some(dockerfile_path),
            BuildType::Script {
                dockerfile_path, ..
            } => Some(dockerfile_path),
            BuildType::Zola {
                dockerfile_path, ..
            } => Some(dockerfile_path),
            BuildType::Book {
                dockerfile_path, ..
            } => Some(dockerfile_path),
            BuildType::Ingress {
                dockerfile_path, ..
            } => Some(dockerfile_path),
            _ => None,
        }
    }

    /// Returns whether this build type requires a Docker build
    pub fn requires_docker_build(&self) -> bool {
        match self {
            BuildType::PureKubernetes => false,
            BuildType::KubernetesInstallation { .. } => false,
            BuildType::PureDockerImage { .. } => false,
            _ => true,
        }
    }

    /// Returns whether this build type has server-side rendering
    pub fn has_ssr(&self) -> bool {
        matches!(self, BuildType::TrunkWasm { ssr: true, .. })
    }
}
