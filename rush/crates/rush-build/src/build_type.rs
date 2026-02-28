//! Build type definitions for different component build strategies
//!
//! This module defines the various build types that Rush supports, including
//! web applications, binary applications, and container-based deployments.

use std::collections::HashMap;

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

    /// A persistent local service for development
    LocalService {
        /// Service type identifier
        service_type: String,
        /// Optional version specification
        version: Option<String>,
        /// Environment variables (including port configuration)
        env: Option<HashMap<String, String>>,
        /// Whether to persist data between runs
        persist_data: bool,
        /// Health check command
        health_check: Option<String>,
        /// Initialization scripts or commands
        init_scripts: Option<Vec<String>>,
        /// Post-startup tasks to run after service is healthy
        post_startup_tasks: Option<Vec<String>>,
        /// Dependencies on other local services
        depends_on: Option<Vec<String>>,
        /// Command override
        command: Option<String>,
    },

    /// A Bazel-based build that produces an OCI image using rules_oci
    Bazel {
        /// Path to the Bazel workspace directory
        location: String,
        /// Output directory for build artifacts (relative or absolute)
        output_dir: String,
        /// Optional context directory for Docker build (legacy, not used with oci_load_target)
        context_dir: Option<String>,
        /// Optional list of Bazel targets to build (legacy, not used with oci_load_target)
        targets: Option<Vec<String>>,
        /// Optional additional Bazel arguments
        additional_args: Option<Vec<String>>,
        /// Optional base image for OCI generation (legacy, not used with oci_load_target)
        base_image: Option<String>,
        /// Bazel target that loads OCI image into Docker (e.g., "//src:load")
        /// When set, Rush uses `bazel run <oci_load_target>` instead of docker build
        oci_load_target: Option<String>,
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
            BuildType::Bazel { location, .. } => Some(location),
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
            BuildType::LocalService { .. } => false, // LocalServices use pre-built images
            _ => true,
        }
    }

    /// Returns whether this build type has server-side rendering
    pub fn has_ssr(&self) -> bool {
        matches!(self, BuildType::TrunkWasm { ssr: true, .. })
    }

    /// Returns whether this build type has a local source directory to watch
    pub fn has_local_directory(&self) -> bool {
        match self {
            BuildType::PureKubernetes => false,
            BuildType::KubernetesInstallation { .. } => false,
            BuildType::PureDockerImage { .. } => false,
            BuildType::LocalService { .. } => false,
            _ => true,
        }
    }
}
