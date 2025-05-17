// File: src/core/environment/types.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Represents environment definitions for a product
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicEnvironmentDefinitions {
    pub(crate) product_name: String,
    pub(crate) components: HashMap<String, ComponentEnvironment>,
    pub(crate) product_dir: PathBuf,
}

impl PublicEnvironmentDefinitions {
    // Getters for private fields
    pub fn get_product_name(&self) -> &str {
        &self.product_name
    }

    pub fn get_product_dir(&self) -> &PathBuf {
        &self.product_dir
    }

    pub fn get_components(&self) -> &HashMap<String, ComponentEnvironment> {
        &self.components
    }
}

/// Represents environment variables for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentEnvironment {
    pub(crate) environment_variables: HashMap<String, GenerationMethod>,
}

/// Methods for generating environment variable values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerationMethod {
    /// A static value
    Static(String),
    /// Prompt the user for a value
    Ask(String),
    /// Prompt the user for a value with a default
    AskWithDefault(String, String),
    /// Generate a timestamp with a specified format
    Timestamp(String),
}
