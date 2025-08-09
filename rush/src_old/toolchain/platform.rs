use serde::{Deserialize, Serialize};
use std::env;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum OperatingSystem {
    Linux,
    MacOS,
}

impl OperatingSystem {
    pub fn default() -> Self {
        Self::from_str(env::consts::OS)
    }

    pub fn to_docker_target(&self) -> String {
        match self {
            OperatingSystem::Linux => "linux".to_string(),
            OperatingSystem::MacOS => "linux".to_string(), // The docker target for platform macos is linux since the docker image is linux
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "linux" => Self::Linux,
            "macos" => Self::MacOS,
            _ => panic!("Invalid platform type: {}", s),
        }
    }
}

impl ToString for OperatingSystem {
    fn to_string(&self) -> String {
        match self {
            OperatingSystem::Linux => "linux".to_string(),
            OperatingSystem::MacOS => "macos".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]

pub enum ArchType {
    X86_64,
    AARCH64,
}

impl ToString for ArchType {
    fn to_string(&self) -> String {
        match self {
            ArchType::X86_64 => "x86_64".to_string(),
            ArchType::AARCH64 => "aarch64".to_string(),
        }
    }
}

impl ArchType {
    pub fn default() -> Self {
        Self::from_str(env::consts::ARCH)
    }

    pub fn to_docker_target(&self) -> String {
        match self {
            ArchType::X86_64 => "amd64".to_string(),
            ArchType::AARCH64 => "arm64".to_string(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "x86_64" => Self::X86_64,
            "aarch64" => Self::AARCH64,
            _ => panic!("Invalid architecture type: {}", s),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Platform {
    pub os: OperatingSystem,
    pub arch: ArchType,
}

impl Platform {
    pub fn default() -> Self {
        let os = OperatingSystem::default();
        let arch = ArchType::default();

        Self { os, arch }
    }

    pub fn new(os: &str, arch: &str) -> Self {
        Self {
            os: OperatingSystem::from_str(os),
            arch: ArchType::from_str(arch),
        }
    }

    pub fn to_rust_target(&self) -> String {
        format!(
            "{}-unknown-{}-gnu",
            self.arch.to_string(),
            self.os.to_string()
        )
    }

    pub fn to_docker_target(&self) -> String {
        format!(
            "{}/{}",
            self.os.to_docker_target(),
            self.arch.to_docker_target()
        )
    }
}

impl ToString for Platform {
    fn to_string(&self) -> String {
        format!("{}-{}", self.os.to_string(), self.arch.to_string())
    }
}
