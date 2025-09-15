use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use sha2::{Sha256, Digest};
use walkdir::WalkDir;

use rush_core::error::{Error, Result};
use rush_build::{ComponentBuildSpec, BuildType};
use rush_toolchain::ToolchainContext;

/// Centralized service for generating deterministic Docker image tags
pub struct ImageTagGenerator {
    toolchain: Arc<ToolchainContext>,
    base_dir: PathBuf,
}

impl ImageTagGenerator {
    /// Create a new ImageTagGenerator
    pub fn new(toolchain: Arc<ToolchainContext>, base_dir: PathBuf) -> Self {
        Self {
            toolchain,
            base_dir,
        }
    }

    /// Compute deterministic tag for a component
    /// Returns:
    /// - `{git_hash}` (8 chars) for clean state
    /// - `{git_hash}-wip-{content_hash}` (8 chars each) for dirty state
    pub fn compute_tag(&self, spec: &ComponentBuildSpec) -> Result<String> {
        // 1. Determine watch directories
        let watch_dirs = self.get_watch_directories(spec);

        // 2. Compute git hash for watch directories
        let git_hash = self.compute_git_hash_for_directories(&watch_dirs)?;

        // Handle case where no git hash is available
        if git_hash.is_empty() || git_hash == "precommit" {
            log::warn!(
                "No git history found for component '{}', using 'latest' tag",
                spec.component_name
            );
            return Ok("latest".to_string());
        }

        // 3. Check if working directory is dirty
        if self.is_dirty(&watch_dirs)? {
            // 4. Compute SHA256 hash of actual content
            let content_hash = self.compute_content_hash(&watch_dirs)?;
            Ok(format!("{}-wip-{}",
                &git_hash[..8.min(git_hash.len())],
                &content_hash[..8.min(content_hash.len())]
            ))
        } else {
            Ok(git_hash[..8.min(git_hash.len())].to_string())
        }
    }

    /// Get all directories that should be watched for changes
    fn get_watch_directories(&self, spec: &ComponentBuildSpec) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // Main component directory
        let component_dir = self.get_component_directory(spec);
        if component_dir.exists() {
            dirs.push(component_dir);
        }

        // For now, we don't have watch directories in BuildType
        // In the future, we could add watch directories to ComponentBuildSpec
        // or extract them from the build configuration

        // Remove duplicates
        dirs.sort();
        dirs.dedup();

        dirs
    }

    /// Get the main directory for a component
    fn get_component_directory(&self, spec: &ComponentBuildSpec) -> PathBuf {
        match &spec.build_type {
            BuildType::TrunkWasm { location, .. } |
            BuildType::DixiousWasm { location, .. } |
            BuildType::RustBinary { location, .. } |
            BuildType::Script { location, .. } |
            BuildType::Zola { location, .. } |
            BuildType::Book { location, .. } => {
                self.base_dir.join(location)
            }
            BuildType::Ingress { .. } => {
                // Ingress typically uses the product directory
                self.base_dir.clone()
            }
            _ => {
                // Default to component name as subdirectory
                self.base_dir.join(&spec.component_name)
            }
        }
    }

    /// Compute the most recent git commit hash that touched any of the watch directories
    fn compute_git_hash_for_directories(&self, dirs: &[PathBuf]) -> Result<String> {
        if dirs.is_empty() {
            // No directories to check, use HEAD
            return self.get_head_hash();
        }

        let git_path = self.toolchain.git();
        let mut latest_hash = String::new();
        let mut latest_time = 0i64;

        for dir in dirs {
            // Skip non-existent directories
            if !dir.exists() {
                continue;
            }

            let dir_str = dir.to_str().ok_or_else(||
                Error::Internal(format!("Invalid path: {:?}", dir))
            )?;

            let output = Command::new(&git_path)
                .args(["log", "-n", "1", "--format=%H %ct", "--", dir_str])
                .current_dir(&self.base_dir)
                .output()
                .map_err(|e| Error::External(format!("Git command failed: {}", e)))?;

            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = stdout.trim().split(' ').collect();

                if parts.len() == 2 {
                    if let Ok(timestamp) = parts[1].parse::<i64>() {
                        if timestamp > latest_time {
                            latest_time = timestamp;
                            latest_hash = parts[0].to_string();
                        }
                    }
                }
            }
        }

        // If no hash found for directories, fall back to HEAD
        if latest_hash.is_empty() {
            self.get_head_hash()
        } else {
            Ok(latest_hash)
        }
    }

    /// Get the HEAD commit hash
    fn get_head_hash(&self) -> Result<String> {
        let git_path = self.toolchain.git();

        let output = Command::new(&git_path)
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.base_dir)
            .output()
            .map_err(|e| Error::External(format!("Git command failed: {}", e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            // No git repository or no commits yet
            Ok("precommit".to_string())
        }
    }

    /// Check if any of the watch directories have uncommitted changes
    fn is_dirty(&self, dirs: &[PathBuf]) -> Result<bool> {
        let git_path = self.toolchain.git();

        for dir in dirs {
            if !dir.exists() {
                continue;
            }

            let dir_str = dir.to_str().ok_or_else(||
                Error::Internal(format!("Invalid path: {:?}", dir))
            )?;

            let output = Command::new(&git_path)
                .args(["status", "--porcelain", "--untracked-files=no", dir_str])
                .current_dir(&self.base_dir)
                .output()
                .map_err(|e| Error::External(format!("Git status failed: {}", e)))?;

            if !output.stdout.is_empty() {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Compute SHA256 hash of all file contents in watch directories
    fn compute_content_hash(&self, dirs: &[PathBuf]) -> Result<String> {
        let mut hasher = Sha256::new();
        let mut files = Vec::new();

        // Collect all files in watch directories
        for dir in dirs {
            if !dir.exists() {
                continue;
            }

            for entry in WalkDir::new(dir)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    let path = entry.path();

                    // Skip .git, target, dist, and node_modules directories
                    let path_str = path.to_str().unwrap_or("");
                    if path_str.contains("/.git/") ||
                       path_str.contains("/target/") ||
                       path_str.contains("/dist/") ||
                       path_str.contains("/node_modules/") ||
                       path_str.contains("/.rush/") {
                        continue;
                    }

                    files.push(path.to_path_buf());
                }
            }
        }

        // Sort files for deterministic hashing
        files.sort();

        // Hash file paths and contents
        for file in files {
            // Hash the relative path
            if let Ok(rel_path) = file.strip_prefix(&self.base_dir) {
                hasher.update(rel_path.to_string_lossy().as_bytes());
                hasher.update(b"\0"); // Separator
            }

            // Hash the file content
            if let Ok(content) = std::fs::read(&file) {
                hasher.update(&content);
                hasher.update(b"\0"); // Separator
            }
        }

        Ok(hex::encode(hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn create_test_generator() -> (ImageTagGenerator, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let toolchain = Arc::new(ToolchainContext::default());
        let generator = ImageTagGenerator::new(toolchain, temp_dir.path().to_path_buf());
        (generator, temp_dir)
    }

    #[test]
    fn test_get_component_directory() {
        let (generator, temp_dir) = create_test_generator();

        let mut spec = ComponentBuildSpec::default();
        spec.component_name = "test-component".to_string();

        // Test RustBinary with location
        spec.build_type = BuildType::RustBinary {
            location: "backend/server".to_string(),
            context_dir: None,
            script: None,
            watch: None,
        };

        let dir = generator.get_component_directory(&spec);
        assert_eq!(dir, temp_dir.path().join("backend/server"));
    }

    #[test]
    fn test_get_watch_directories() {
        let (generator, temp_dir) = create_test_generator();

        // Create some directories
        fs::create_dir_all(temp_dir.path().join("backend/server")).unwrap();
        fs::create_dir_all(temp_dir.path().join("shared/types")).unwrap();

        let mut spec = ComponentBuildSpec::default();
        spec.component_name = "test-component".to_string();
        spec.build_type = BuildType::RustBinary {
            location: "backend/server".to_string(),
            context_dir: None,
            script: None,
            watch: Some(vec!["shared/types".to_string()]),
        };

        let dirs = generator.get_watch_directories(&spec);
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&temp_dir.path().join("backend/server")));
        assert!(dirs.contains(&temp_dir.path().join("shared/types")));
    }

    #[test]
    fn test_content_hash_deterministic() {
        let (generator, temp_dir) = create_test_generator();

        // Create a test file
        let test_dir = temp_dir.path().join("test");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(test_dir.join("file.txt"), "test content").unwrap();

        let dirs = vec![test_dir.clone()];

        // Compute hash twice
        let hash1 = generator.compute_content_hash(&dirs).unwrap();
        let hash2 = generator.compute_content_hash(&dirs).unwrap();

        // Should be identical
        assert_eq!(hash1, hash2);

        // Change content
        fs::write(test_dir.join("file.txt"), "different content").unwrap();
        let hash3 = generator.compute_content_hash(&dirs).unwrap();

        // Should be different
        assert_ne!(hash1, hash3);
    }
}