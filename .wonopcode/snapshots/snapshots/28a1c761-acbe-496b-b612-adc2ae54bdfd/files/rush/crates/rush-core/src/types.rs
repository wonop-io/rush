//! Core types used throughout Rush

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::constants::{docker_platform_for_arch, docker_platform_native};

/// Target architecture for Docker image builds.
///
/// This controls what architecture Docker images will be built for.
/// By default, images are built for the native (host) architecture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TargetArchitecture {
    /// Build for the native host architecture (default)
    #[default]
    Native,
    /// Build for x86_64/amd64 architecture
    Amd64,
    /// Build for arm64/aarch64 architecture
    Arm64,
}

impl TargetArchitecture {
    /// Returns the Docker platform string for this architecture (e.g., "linux/amd64")
    pub fn to_docker_platform(&self) -> &'static str {
        match self {
            TargetArchitecture::Native => docker_platform_native(),
            TargetArchitecture::Amd64 => docker_platform_for_arch("x86_64"),
            TargetArchitecture::Arm64 => docker_platform_for_arch("aarch64"),
        }
    }

    /// Returns the Rust target triple for this architecture
    pub fn to_rust_target(&self) -> String {
        match self {
            TargetArchitecture::Native => {
                // Detect native architecture
                let arch = std::env::consts::ARCH;
                format!("{arch}-unknown-linux-gnu")
            }
            TargetArchitecture::Amd64 => "x86_64-unknown-linux-gnu".to_string(),
            TargetArchitecture::Arm64 => "aarch64-unknown-linux-gnu".to_string(),
        }
    }

    /// Returns the Bazel `--platforms=` flag value for this architecture.
    ///
    /// Returns `None` for native architecture (lets Bazel use host platform).
    /// Returns the path to a platform target for cross-compilation.
    pub fn to_bazel_platform(&self) -> Option<String> {
        match self {
            TargetArchitecture::Native => None,
            TargetArchitecture::Amd64 => Some("//platforms:linux_amd64".to_string()),
            TargetArchitecture::Arm64 => Some("//platforms:linux_arm64".to_string()),
        }
    }

    /// Returns the architecture name (e.g., "x86_64", "aarch64")
    pub fn arch_name(&self) -> &'static str {
        match self {
            TargetArchitecture::Native => std::env::consts::ARCH,
            TargetArchitecture::Amd64 => "x86_64",
            TargetArchitecture::Arm64 => "aarch64",
        }
    }

    /// Returns true if this is the native host architecture
    pub fn is_native(&self) -> bool {
        matches!(self, TargetArchitecture::Native)
    }

    /// Returns true if building for this architecture requires cross-compilation
    /// on the current host
    pub fn requires_cross_compilation(&self) -> bool {
        let host_arch = std::env::consts::ARCH;
        match self {
            TargetArchitecture::Native => false,
            TargetArchitecture::Amd64 => host_arch != "x86_64",
            TargetArchitecture::Arm64 => host_arch != "aarch64",
        }
    }
}

impl fmt::Display for TargetArchitecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetArchitecture::Native => write!(f, "native"),
            TargetArchitecture::Amd64 => write!(f, "amd64"),
            TargetArchitecture::Arm64 => write!(f, "arm64"),
        }
    }
}

impl FromStr for TargetArchitecture {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "native" => Ok(TargetArchitecture::Native),
            "amd64" | "x86_64" | "x86" => Ok(TargetArchitecture::Amd64),
            "arm64" | "aarch64" => Ok(TargetArchitecture::Arm64),
            _ => Err(format!(
                "Unknown architecture: '{}'. Valid values: native, amd64, arm64",
                s
            )),
        }
    }
}

impl From<&str> for TargetArchitecture {
    fn from(s: &str) -> Self {
        TargetArchitecture::from_str(s).unwrap_or_default()
    }
}

/// Represents different environments
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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
