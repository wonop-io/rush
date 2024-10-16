use crate::builder::BuildType;
use crate::container::ServicesSpec;
use crate::toolchain::Platform;
use crate::ToolchainContext;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str;

#[derive(Serialize, Deserialize, Debug)]
pub struct BuildContext {
    pub build_type: BuildType,
    pub location: Option<String>,
    pub target: Platform,
    pub host: Platform,
    pub rust_target: String,
    pub toolchain: ToolchainContext,
    pub services: ServicesSpec,

    pub environment: String,
    pub domain: String,
    pub product_name: String,
    pub product_uri: String,
    pub component: String,
    pub docker_registry: String,
    pub image_name: String,

    pub domains: HashMap<String, String>,
    pub env: HashMap<String, String>,
    pub secrets: HashMap<String, String>,
}
