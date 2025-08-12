use crate::toolchain::Platform;
use crate::utils::{first_which, resolve_toolchain_path};
use log::{debug, trace, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;
use std::str;

/// ToolchainContext provides access to the development toolchain
/// and handles cross-compilation settings between different platforms.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolchainContext {
    /// Host platform (where Rush is running)
    host: Platform,
    /// Target platform (where the built artifacts will run)
    target: Platform,

    // Primary tools
    git: String,
    docker: String,
    trunk: String,
    kubectl: Option<String>,
    kubectx: Option<String>,
    minikube: Option<String>,
    kubeconform: Option<String>,
    kubeval: Option<String>,

    // Compiler and build tools
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
    /// Creates a new ToolchainContext for the specified host and target platforms
    pub fn new(host: Platform, target: Platform) -> Self {
        trace!(
            "Creating new ToolchainContext with host: {:?}, target: {:?}",
            host,
            target
        );

        let mut ret = if host.arch == target.arch && host.os == target.os {
            Self::default()
        } else {
            // Handle cross-compilation toolchains
            Self::find_cross_compilation_toolchain(&host, &target)
                .unwrap_or_else(|| panic!("No suitable toolchain found for {target:?}"))
        };

        ret.host = host;
        ret.target = target;
        ret
    }

    /// Returns the default toolchain for the current platform
    pub fn default() -> Self {
        trace!("Creating default ToolchainContext");

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
            kubectx: first_which(vec!["kubectx"]),
            minikube: first_which(vec!["minikube"]),
            kubeconform: first_which(vec!["kubeconform"]),
            kubeval: first_which(vec!["kubeval"]),

            cc: first_which(vec!["clang", "gcc"]).expect("No C compiler found"),
            cxx: first_which(vec!["clang++", "g++"]).expect("No C++ compiler found"),
            ar: first_which(vec!["ar", "libtool"]).expect("No archive tool found"),
            ranlib: first_which(vec!["ranlib", "libtool"]).expect("No ranlib tool found"),
            nm: first_which(vec!["nm", "libtool"]).expect("No nm tool found"),
            strip: first_which(vec!["strip", "libtool"]).expect("No strip tool found"),
            objdump: first_which(vec!["objdump", "libtool"]).expect("No objdump tool found"),
            objcopy: first_which(vec!["objcopy", "libtool"]).expect("No objcopy tool found"),
            ld: first_which(vec!["ld", "libtool"]).expect("No linker found"),
        }
    }

    /// Creates a ToolchainContext from a specific directory path containing tools
    pub fn from_path(path: &str) -> Option<Self> {
        trace!("Attempting to create ToolchainContext from path: {}", path);

        if !Path::new(path).exists() {
            warn!("Toolchain path does not exist: {}", path);
            return None;
        }

        let cc = resolve_toolchain_path(path, "gcc")?;
        let cxx = resolve_toolchain_path(path, "g++")?;
        let ar = resolve_toolchain_path(path, "ar")?;
        let ranlib = resolve_toolchain_path(path, "ranlib")?;
        let nm = resolve_toolchain_path(path, "nm")?;
        let strip = resolve_toolchain_path(path, "strip")?;
        let objdump = resolve_toolchain_path(path, "objdump")?;
        let objcopy = resolve_toolchain_path(path, "objcopy")?;
        let ld = resolve_toolchain_path(path, "ld")?;

        debug!("Found toolchain in {}: cc={}, cxx={}", path, cc, cxx);

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
            kubectx: first_which(vec!["kubectx"]),
            minikube: first_which(vec!["minikube"]),
            kubeconform: first_which(vec!["kubeconform"]),
            kubeval: first_which(vec!["kubeval"]),

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
    }

    /// Tries to find a toolchain from a list of potential paths
    pub fn from_first_path(paths: Vec<&str>) -> Option<Self> {
        trace!("Searching for toolchain in paths: {:?}", paths);

        for path in &paths {
            debug!("Checking path: {}", path);
            if let Some(toolchain) = Self::from_path(path) {
                debug!("Found toolchain at path: {}", path);
                return Some(toolchain);
            }
        }

        None
    }

    /// Finds an appropriate cross-compilation toolchain for the given host and target
    fn find_cross_compilation_toolchain(host: &Platform, target: &Platform) -> Option<Self> {
        match (
            host.os.to_string().as_str(),
            target.os.to_string().as_str(),
            host.arch.to_string().as_str(),
            target.arch.to_string().as_str(),
        ) {
            // MacOS host to Linux targets
            ("macos", "linux", _, "x86_64") => {
                debug!("Looking for x86_64-linux toolchain on macOS");
                Self::from_first_path(vec![
                    "/opt/homebrew/Cellar/x86_64-unknown-linux-gnu/7.2.0/bin/",
                    "/usr/local/opt/x86_64-unknown-linux-gnu/bin/",
                ])
            }
            ("macos", "linux", _, "aarch64") => {
                debug!("Looking for aarch64-linux toolchain on macOS");
                Self::from_first_path(vec![
                    "/opt/homebrew/Cellar/aarch64-unknown-linux-gnu/7.2.0/bin/",
                    "/usr/local/opt/aarch64-unknown-linux-gnu/bin/",
                ])
            }
            // Add other cross-compilation toolchain paths as needed
            _ => {
                warn!(
                    "No known cross-compilation toolchain for host {:?}/{:?} to target {:?}/{:?}",
                    host.os, host.arch, target.os, target.arch
                );
                None
            }
        }
    }

    /// Sets up environment variables for the toolchain
    pub fn setup_env(&self) {
        trace!("Setting up environment variables for toolchain");

        std::env::set_var("CC", &self.cc);
        std::env::set_var("CXX", &self.cxx);
        std::env::set_var("AR", &self.ar);
        std::env::set_var("RANLIB", &self.ranlib);
        std::env::set_var("NM", &self.nm);
        std::env::set_var("STRIP", &self.strip);
        std::env::set_var("OBJDUMP", &self.objdump);
        std::env::set_var("OBJCOPY", &self.objcopy);
        std::env::set_var("LD", &self.ld);

        debug!(
            "Environment variables set: CC={}, CXX={}",
            self.cc, self.cxx
        );
    }

    // Getters for various tools and platforms

    /// Returns the host platform
    pub fn host(&self) -> &Platform {
        &self.host
    }

    /// Returns the target platform
    pub fn target(&self) -> &Platform {
        &self.target
    }

    /// Returns the git executable path
    pub fn git(&self) -> &str {
        &self.git
    }

    /// Returns the docker executable path
    pub fn docker(&self) -> &str {
        &self.docker
    }

    /// Returns the trunk executable path
    pub fn trunk(&self) -> &str {
        &self.trunk
    }

    /// Returns whether kubectl is available
    pub fn has_kubectl(&self) -> bool {
        self.kubectl.is_some()
    }

    /// Returns the kubectl executable path
    pub fn kubectl(&self) -> &str {
        self.kubectl.as_ref().expect("kubectl not found")
    }

    /// Returns whether kubectx is available
    pub fn has_kubectx(&self) -> bool {
        self.kubectx.is_some()
    }

    /// Returns the kubectx executable path
    pub fn kubectx(&self) -> &str {
        self.kubectx.as_ref().expect("kubectx not found")
    }

    /// Returns whether minikube is available
    pub fn has_minikube(&self) -> bool {
        self.minikube.is_some()
    }

    /// Returns the minikube executable path
    pub fn minikube(&self) -> Option<String> {
        self.minikube.clone()
    }

    /// Returns whether kubeconform is available
    pub fn has_kubeconform(&self) -> bool {
        self.kubeconform.is_some()
    }

    /// Returns the kubeconform executable path
    pub fn kubeconform(&self) -> &str {
        self.kubeconform.as_ref().expect("kubeconform not found")
    }

    /// Returns whether kubeval is available
    pub fn has_kubeval(&self) -> bool {
        self.kubeval.is_some()
    }

    /// Returns the kubeval executable path
    pub fn kubeval(&self) -> &str {
        self.kubeval.as_ref().expect("kubeval not found")
    }

    /// Returns the C compiler path
    pub fn cc(&self) -> &str {
        &self.cc
    }

    // Git-related utility methods

    /// Gets the git folder hash for a given subdirectory path
    pub fn get_git_folder_hash(&self, subdirectory_path: &str) -> Result<String, String> {
        trace!("Getting git folder hash for: {}", subdirectory_path);

        // First try to get hash for the specific subdirectory
        let hash_output = Command::new(&self.git)
            .args(["log", "-n", "1", "--format=%H", "--", subdirectory_path])
            .output()
            .map_err(|e| e.to_string())?;

        let hash = str::from_utf8(&hash_output.stdout)
            .map_err(|e| e.to_string())?
            .trim()
            .to_string();

        if !hash_output.status.success() || hash.is_empty() {
            debug!(
                "No git hash found for subdirectory '{}', trying HEAD",
                subdirectory_path
            );

            // Fall back to HEAD commit hash if subdirectory has no commits
            let head_output = Command::new(&self.git)
                .args(["rev-parse", "HEAD"])
                .output()
                .map_err(|e| e.to_string())?;

            let head_hash = str::from_utf8(&head_output.stdout)
                .map_err(|e| e.to_string())?
                .trim()
                .to_string();

            if !head_output.status.success() || head_hash.is_empty() {
                debug!("No git HEAD found, using 'precommit'");
                return Ok("precommit".to_string());
            }

            trace!("Using HEAD hash: {}", head_hash);
            return Ok(head_hash);
        }

        trace!("Git folder hash: {}", hash);
        Ok(hash)
    }

    /// Checks if a git directory has work-in-progress changes and returns a hash suffix if it does
    pub fn get_git_wip(&self, subdirectory_path: &str) -> Result<String, String> {
        trace!("Checking for WIP changes in: {}", subdirectory_path);

        // Use git status --porcelain to check for changes, respecting .gitignore
        // The --porcelain flag gives us machine-readable output
        // We only care about tracked files that have changes
        let status_output = Command::new(&self.git)
            .args(["status", "--porcelain", "--untracked-files=no", subdirectory_path])
            .output()
            .map_err(|e| e.to_string())?;

        let status = str::from_utf8(&status_output.stdout)
            .map_err(|e| e.to_string())?
            .trim()
            .to_string();

        if !status.is_empty() {
            // There are changes to tracked files
            // Get the actual diff for hashing (only tracked files)
            let diff_output = Command::new(&self.git)
                .args(["diff", "HEAD", subdirectory_path])
                .output()
                .map_err(|e| e.to_string())?;

            let diff = str::from_utf8(&diff_output.stdout)
                .map_err(|e| e.to_string())?
                .trim()
                .to_string();

            if !diff.is_empty() {
                let mut hasher = Sha256::new();
                hasher.update(diff.as_bytes());
                let wip_hash = hex::encode(hasher.finalize());
                let suffix = format!("-wip-{}", &wip_hash[..8]);

                trace!("WIP changes detected, suffix: {}", suffix);
                return Ok(suffix);
            }
        }

        trace!("No WIP changes detected");
        Ok("".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_default_toolchain() {
        let toolchain = ToolchainContext::default();
        assert_eq!(toolchain.host(), &Platform::default());
        assert_eq!(toolchain.target(), &Platform::default());

        // These should be available on any system running tests
        assert!(!toolchain.git().is_empty());
        assert!(!toolchain.cc().is_empty());
    }

    #[test]
    fn test_setup_env() {
        let toolchain = ToolchainContext::default();
        toolchain.setup_env();

        assert_eq!(env::var("CC").unwrap(), toolchain.cc);
        assert_eq!(env::var("CXX").unwrap(), toolchain.cxx);
        assert_eq!(env::var("AR").unwrap(), toolchain.ar);
    }

    #[test]
    fn test_from_path_nonexistent() {
        let result = ToolchainContext::from_path("/path/that/does/not/exist");
        assert!(result.is_none());
    }

    #[test]
    #[ignore] // This test requires git to be installed and available
    fn test_get_git_wip() {
        // Create a temporary directory for git testing
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to init git repo");

        // Configure git for test
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to configure git");

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to configure git");

        // Create and commit a test file
        let test_file_path = temp_path.join("test.txt");
        let mut file = File::create(&test_file_path).unwrap();
        writeln!(file, "Initial content").unwrap();

        Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to add file");

        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to commit");

        // Create toolchain
        let toolchain = ToolchainContext::default();

        // Check clean state
        let result = toolchain.get_git_wip(temp_path.to_str().unwrap());
        assert_eq!(result.unwrap(), "");

        // Modify the file and check WIP state
        let mut file = fs::OpenOptions::new()
            
            .append(true)
            .open(test_file_path)
            .unwrap();
        writeln!(file, "Modified content").unwrap();

        let result = toolchain.get_git_wip(temp_path.to_str().unwrap());
        let wip = result.unwrap();
        assert!(wip.starts_with("-wip-"));
        assert_eq!(wip.len(), 13); // "-wip-" + 8 chars of hash
    }
}
