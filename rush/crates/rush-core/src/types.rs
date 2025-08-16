//! Core types used throughout Rush

use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents different environments
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum Environment {
    #[default]
    Development,
    Staging,
    Production,
    Custom(String),
}


impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Environment::Development => write!(f, "dev"),
            Environment::Staging => write!(f, "staging"),
            Environment::Production => write!(f, "prod"),
            Environment::Custom(s) => write!(f, "{s}"),
        }
    }
}

impl From<&str> for Environment {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "dev" | "development" => Environment::Development,
            "staging" | "stage" => Environment::Staging,
            "prod" | "production" => Environment::Production,
            custom => Environment::Custom(custom.to_string()),
        }
    }
}

/// Platform information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    pub os: String,
    pub arch: String,
}

impl Platform {
    pub fn new(os: &str, arch: &str) -> Self {
        Platform {
            os: os.to_string(),
            arch: arch.to_string(),
        }
    }

    pub fn current() -> Self {
        Platform {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

impl Default for Platform {
    fn default() -> Self {
        Self::current()
    }
}
