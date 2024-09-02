mod platform;
use crate::toolchain::platform::{ArchType, OperatingSystem};
use crate::utils::{first_which, resolve_toolchain_path};
pub use platform::Platform;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::str;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolchainContext {
    host: Platform,
    target: Platform,

    // Main tools
    git: String,
    docker: String,
    trunk: String,
    kubectl: Option<String>,
    minikube: Option<String>,

    // Secondary
    cc: String,
    cxx: String,
    ar: String,
    ranlib: String,
    nm: String,
    strip: String,
    objdump: String,
    objcopy: String,
    ld: String,
}

impl ToolchainContext {
    pub fn default() -> Self {
        ToolchainContext {
            host: Platform::default(),
            target: Platform::default(),

            git: first_which(vec!["git"]).expect("git not found."),
            docker: first_which(vec!["docker"]).expect("docker not found."),
            trunk: first_which(vec![
                "$HOME/.cargo/bin/wasm-trunk",
                "$HOME/.cargo/bin/trunk",
                "wasm-trunk",
                "trunk",
            ])
            .expect("trunk not found."),
            kubectl: first_which(vec!["kubectl"]),
            minikube: first_which(vec!["minikube"]),

            cc: first_which(vec!["clang", "gcc"])
                .expect("None of the default toolchains are availablefor this architecture"),
            cxx: first_which(vec!["clang++", "g++"])
                .expect("None of the default toolchains are availablefor this architecture"),
            ar: first_which(vec!["ar", "libtool"])
                .expect("None of the default toolchains are availablefor this architecture"),
            ranlib: first_which(vec!["ranlib", "libtool"]).expect("None of the default for "),
            nm: first_which(vec!["nm", "libtool"]).expect("None of the default for "),
            strip: first_which(vec!["strip", "libtool"]).expect("None of the default for "),
            objdump: first_which(vec!["objdump", "libtool"]).expect("None of the default for "),
            objcopy: first_which(vec!["objcopy", "libtool"]).expect("None of the default for "),
            ld: first_which(vec!["ld", "libtool"]).expect("None of the default for "),
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        if std::path::Path::new(path).exists() {
            let cc = match resolve_toolchain_path(path, "gcc") {
                Some(path) => path,
                None => return None,
            };
            let cxx = match resolve_toolchain_path(path, "g++") {
                Some(path) => path,
                None => return None,
            };
            let ar = match resolve_toolchain_path(path, "ar") {
                Some(path) => path,
                None => return None,
            };
            let ranlib = match resolve_toolchain_path(path, "ranlib") {
                Some(path) => path,
                None => return None,
            };
            let nm = match resolve_toolchain_path(path, "nm") {
                Some(path) => path,
                None => return None,
            };
            let strip = match resolve_toolchain_path(path, "strip") {
                Some(path) => path,
                None => return None,
            };
            let objdump = match resolve_toolchain_path(path, "objdump") {
                Some(path) => path,
                None => return None,
            };
            let objcopy = match resolve_toolchain_path(path, "objcopy") {
                Some(path) => path,
                None => return None,
            };
            let ld = match resolve_toolchain_path(path, "ld") {
                Some(path) => path,
                None => return None,
            };

            Some(ToolchainContext {
                host: Platform::default(),
                target: Platform::default(),

                git: first_which(vec!["git"]).expect("git not found."),
                docker: first_which(vec!["docker"]).expect("docker not found."),
                trunk: first_which(vec![
                    "$HOME/.cargo/bin/wasm-trunk",
                    "$HOME/.cargo/bin/trunk",
                    "wasm-trunk",
                    "trunk",
                ])
                .expect("trunk not found."),
                kubectl: first_which(vec!["kubectl"]),
                minikube: first_which(vec!["minikube"]),

                cc,
                cxx,
                ar,
                ranlib,
                nm,
                strip,
                objdump,
                objcopy,
                ld,
            })
        } else {
            None
        }
    }

    pub fn setup_env(&self) {
        std::env::set_var("CC", self.cc.clone());
        std::env::set_var("CXX", self.cxx.clone());
        std::env::set_var("AR", self.ar.clone());
        std::env::set_var("RANLIB", self.ranlib.clone());
        std::env::set_var("NM", self.nm.clone());
        std::env::set_var("STRIP", self.strip.clone());
        std::env::set_var("OBJDUMP", self.objdump.clone());
        std::env::set_var("OBJCOPY", self.objcopy.clone());
        std::env::set_var("LD", self.ld.clone());
    }

    pub fn host(&self) -> &Platform {
        &self.host
    }

    pub fn target(&self) -> &Platform {
        &self.target
    }

    pub fn from_first_path(paths: Vec<&str>) -> Option<Self> {
        for path in &paths {
            if let Some(toolchain) = Self::from_path(path) {
                return Some(toolchain);
            }
        }
        None
    }

    pub fn new(host: Platform, target: Platform) -> Self {
        let mut ret = if host.arch == target.arch && host.os == target.os {
            Self::default()
        } else if host.os == OperatingSystem::MacOS {
            if target.arch == ArchType::X86_64 {
                Self::from_first_path(vec![
                    "/opt/homebrew/Cellar/x86_64-unknown-linux-gnu/7.2.0/bin/",
                ])
                .expect("No suitable toolchain found")
            } else if target.arch == ArchType::AARCH64 {
                Self::from_first_path(vec![
                    "/opt/homebrew/Cellar/aarch64-unknown-linux-gnu/7.2.0/bin/",
                ])
                .expect("No suitable toolchain found")
            } else {
                panic!("Unsupported target architecture: {}", target.to_string());
            }
        } else {
            panic!("Unsupported host OS: {}", host.to_string());
        };
        ret.host = host;
        ret.target = target;
        ret
    }

    pub fn has_minikube(&self) -> bool {
        self.minikube.is_some()
    }

    pub fn minikube(&self) -> Option<String> {
        self.minikube.clone()
    }

    pub fn docker(&self) -> &str {
        &self.docker
    }

    pub fn trunk(&self) -> &str {
        &self.trunk
    }

    pub fn has_kubectl(&self) -> bool {
        self.kubectl.is_some()
    }

    pub fn kubectl(&self) -> &str {
        self.kubectl.as_ref().expect("kubectl not found")
    }

    pub fn git(&self) -> &str {
        &self.git
    }
    // Git
    pub fn get_git_folder_hash(&self, subdirectory_path: &str) -> Result<String, String> {
        let hash_output = Command::new(&self.git)
            .args(["log", "-n", "1", "--format=%H", "--", subdirectory_path])
            .output()
            .map_err(|e| e.to_string())?;

        let hash = str::from_utf8(&hash_output.stdout)
            .map_err(|e| e.to_string())?
            .trim()
            .to_string();

        if !hash_output.status.success() || hash.is_empty() {
            return Ok("precommit".to_string());
            /*
            return Err(format!(
                "Failed computing hash for directory {}: {}",
                subdirectory_path,
                String::from_utf8_lossy(&hash_output.stderr).to_string()
            ));
            */
        }

        Ok(hash)
    }

    pub fn get_git_wip(&self, subdirectory_path: &str) -> Result<String, String> {
        let dirty_output = Command::new(&self.git)
            .args(["diff", subdirectory_path])
            .output()
            .map_err(|e| e.to_string())?;

        let diff = str::from_utf8(&dirty_output.stdout)
            .map_err(|e| e.to_string())?
            .trim()
            .to_string();

        if !diff.is_empty() {
            return Ok("-wip".to_string());
        }

        Ok("".to_string())
    }
}
