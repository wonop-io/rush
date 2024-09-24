use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSpec {
    pub name: String,
    pub port: u16,
    pub target_port: u16,
    pub mount_point: Option<String>,
    pub domain: String,
}

pub type ServicesSpec = HashMap<String, Vec<ServiceSpec>>;
