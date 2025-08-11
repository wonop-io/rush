use crate::build::BuildType;
use crate::container::ServicesSpec;
use crate::toolchain::Platform;
use crate::toolchain::ToolchainContext;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str;

/// BuildContext contains all information needed to build a component
#[derive(Serialize, Deserialize, Debug)]
pub struct BuildContext {
    /// Build type configuration
    #[serde(flatten)]
    pub build_type: BuildType,

    /// Component location within the product directory
    pub location: Option<String>,

    /// Target platform (where the code will run)
    pub target: Platform,

    /// Host platform (where the build is running)
    pub host: Platform,

    /// Rust target triple for cross-compilation
    pub rust_target: String,

    /// Toolchain configuration for building
    pub toolchain: ToolchainContext,

    /// Services specification for container coordination
    pub services: ServicesSpec,

    /// Current environment (dev, prod, etc.)
    pub environment: String,

    /// Domain for the component
    pub domain: String,

    /// Name of the product
    pub product_name: String,

    /// URI-friendly product identifier
    pub product_uri: String,

    /// Name of the component
    pub component: String,

    /// Docker registry for container images
    pub docker_registry: String,

    /// Full image name with tag
    pub image_name: String,

    /// Component secrets for build time
    pub secrets: HashMap<String, String>,

    /// Domain mappings for different environments
    pub domains: HashMap<String, String>,

    /// Environment variables for the component
    pub env: HashMap<String, String>,

    /// Cross-compilation method ("native" or "cross-rs")
    pub cross_compile: String,
}
