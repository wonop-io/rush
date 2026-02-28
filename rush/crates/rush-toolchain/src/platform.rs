use std::str::FromStr;
use std::{env, fmt};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum OperatingSystem {
    Linux,
    MacOS,
}

impl Default for OperatingSystem {
    fn default() -> Self {
        Self::from_str(env::consts::OS).expect("Invalid OS")
    }
}

impl OperatingSystem {
    pub fn to_docker_target(&self) -> String {
        match self {
            OperatingSystem::Linux => "linux".to_string(),
            OperatingSystem::MacOS => "linux".to_string(), // The docker target for platform macos is linux since the docker image is linux
        }
    }
}

impl FromStr for OperatingSystem {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "linux" => Ok(Self::Linux),
            "macos" => Ok(Self::MacOS),
            _ => Err(format!("Invalid platform type: {s}")),
        }
    }
}

impl fmt::Display for OperatingSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            OperatingSystem::Linux => "linux",
            OperatingSystem::MacOS => "macos",
        };
        write!(f, "{s}")
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ArchType {
    X86_64,
    AARCH64,
}

impl Default for ArchType {
    fn default() -> Self {
        Self::from_str(env::consts::ARCH).expect("Invalid architecture")
    }
}

impl ArchType {
    pub fn to_docker_target(&self) -> String {
        match self {
            ArchType::X86_64 => "amd64".to_string(),
            ArchType::AARCH64 => "arm64".to_string(),
        }
    }
}

impl FromStr for ArchType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "x86_64" => Ok(Self::X86_64),
            "aarch64" => Ok(Self::AARCH64),
            _ => Err(format!("Invalid architecture type: {s}")),
        }
    }
}

impl fmt::Display for ArchType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ArchType::X86_64 => "x86_64",
            ArchType::AARCH64 => "aarch64",
        };
        write!(f, "{s}")
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Platform {
    pub os: OperatingSystem,
    pub arch: ArchType,
}

impl Platform {
    pub fn new(os: &str, arch: &str) -> Self {
        Self {
            os: OperatingSystem::from_str(os).expect("Invalid OS"),
            arch: ArchType::from_str(arch).expect("Invalid architecture"),
        }
    }

    /// Returns a Platform configured for Docker containers.
    /// Docker containers always run Linux, so this returns Linux with native architecture.
    pub fn for_docker() -> Self {
        Self {
            os: OperatingSystem::Linux,
            arch: ArchType::default(), // Use native architecture
        }
    }

    pub fn to_rust_target(&self) -> String {
        match self.os {
            OperatingSystem::Linux => format!("{}-unknown-linux-gnu", self.arch),
            OperatingSystem::MacOS => format!("{}-apple-darwin", self.arch),
        }
    }

    pub fn to_docker_target(&self) -> String {
        format!(
            "{}/{}",
            self.os.to_docker_target(),
            self.arch.to_docker_target()
        )
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.os, self.arch)
    }
}
