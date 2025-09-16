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
        // 1. Get watched files (expand patterns on each computation for dynamic detection)
        let (watch_files, watch_dirs) = self.get_watch_files_and_directories(spec);

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
        if self.is_dirty_with_files(&watch_files, &watch_dirs)? {
            // 4. Compute SHA256 hash of actual content
            let content_hash = self.compute_content_hash_from_files(&watch_files)?;
            Ok(format!("{}-wip-{}",
                &git_hash[..8.min(git_hash.len())],
                &content_hash[..8.min(content_hash.len())]
            ))
        } else {
            Ok(git_hash[..8.min(git_hash.len())].to_string())
        }
    }

    /// Get watched files and directories by walking and matching patterns
    /// Returns (files, directories) tuple
    fn get_watch_files_and_directories(&self, spec: &ComponentBuildSpec) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let mut files = Vec::new();
        let mut dirs = Vec::new();

        // Main component directory
        let component_dir = self.get_component_directory(spec);
        if component_dir.exists() {
            dirs.push(component_dir.clone());

            // ALWAYS walk the component directory to get all component files
            log::debug!("Walking component directory for '{}'", spec.component_name);

            for entry in WalkDir::new(&component_dir)
                .follow_links(false)
                .max_depth(10)  // Limit recursion depth
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    let path = entry.path();

                    // Skip common build artifacts
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

            let component_file_count = files.len();
            log::debug!("Found {} component files for '{}'",
                component_file_count, spec.component_name);

            // ADDITIONALLY check watch patterns for extra files outside component dir
            if let Some(watch) = &spec.watch {
                log::debug!("Checking watch patterns for additional files for '{}'",
                    spec.component_name);

                // Walk from base directory for watch patterns that might be outside component
                for entry in WalkDir::new(&self.base_dir)
                    .follow_links(false)
                    .max_depth(10)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.file_type().is_file() {
                        let path = entry.path();

                        // Skip if already included from component directory
                        if path.starts_with(&component_dir) {
                            continue;
                        }

                        // Skip common build artifacts
                        let path_str = path.to_str().unwrap_or("");
                        if path_str.contains("/.git/") ||
                           path_str.contains("/target/") ||
                           path_str.contains("/dist/") ||
                           path_str.contains("/node_modules/") ||
                           path_str.contains("/.rush/") {
                            continue;
                        }

                        // Check if file matches any watch pattern
                        if watch.matches(path) {
                            files.push(path.to_path_buf());
                            // Track parent directory
                            if let Some(parent) = path.parent() {
                                if !dirs.contains(&parent.to_path_buf()) {
                                    dirs.push(parent.to_path_buf());
                                }
                            }
                        }
                    }
                }

                let watch_file_count = files.len() - component_file_count;
                log::debug!("Found {} additional files from watch patterns for '{}'",
                    watch_file_count, spec.component_name);
            }
        }

        // Remove duplicates
        files.sort();
        files.dedup();
        dirs.sort();
        dirs.dedup();

        if files.is_empty() {
            log::warn!("No files found for component '{}' - this will result in empty hash!",
                spec.component_name);
        }

        log::debug!("Total files for '{}': {} files in {} directories",
            spec.component_name, files.len(), dirs.len());

        (files, dirs)
    }

    /// Get all directories that should be watched for changes
    /// This is kept for backwards compatibility but delegates to get_watch_files_and_directories
    fn get_watch_directories(&self, spec: &ComponentBuildSpec) -> Vec<PathBuf> {
        let (_files, dirs) = self.get_watch_files_and_directories(spec);
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

    /// Check if any of the watched files or directories have uncommitted changes
    fn is_dirty_with_files(&self, files: &[PathBuf], dirs: &[PathBuf]) -> Result<bool> {
        let git_path = self.toolchain.git();

        // Check directories for modifications
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

        // Also check specific files if they were expanded from patterns
        if !files.is_empty() {
            // Check if any of the specific files are modified
            for file in files {
                if !file.exists() {
                    continue;
                }

                let file_str = file.to_str().ok_or_else(||
                    Error::Internal(format!("Invalid path: {:?}", file))
                )?;

                let output = Command::new(&git_path)
                    .args(["status", "--porcelain", "--untracked-files=no", file_str])
                    .current_dir(&self.base_dir)
                    .output()
                    .map_err(|e| Error::External(format!("Git status failed: {}", e)))?;

                if !output.stdout.is_empty() {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Check if any of the watch directories have uncommitted changes
    /// This is kept for backwards compatibility
    fn is_dirty(&self, dirs: &[PathBuf]) -> Result<bool> {
        self.is_dirty_with_files(&[], dirs)
    }

    /// Compute SHA256 hash of specific files
    fn compute_content_hash_from_files(&self, files: &[PathBuf]) -> Result<String> {
        let mut hasher = Sha256::new();

        if files.is_empty() {
            log::error!("Computing hash with empty file list - this will produce the empty string hash!");
        }

        // Create a sorted copy for deterministic hashing
        let mut sorted_files = files.to_vec();
        sorted_files.sort();

        let mut hashed_count = 0;

        // Hash file paths and contents
        for file in sorted_files {
            // Skip non-existent files
            if !file.exists() {
                log::trace!("Skipping non-existent file: {:?}", file);
                continue;
            }

            // Hash the relative path
            if let Ok(rel_path) = file.strip_prefix(&self.base_dir) {
                hasher.update(rel_path.to_string_lossy().as_bytes());
                hasher.update(b"\0"); // Separator
            }

            // Hash the file content
            if let Ok(content) = std::fs::read(&file) {
                hasher.update(&content);
                hasher.update(b"\0"); // Separator
                hashed_count += 1;
            }
        }

        let hash = hex::encode(hasher.finalize());

        if &hash[..8.min(hash.len())] == "e3b0c442" {
            log::error!("Generated empty string hash! No files were hashed. File list had {} entries, {} were hashed",
                files.len(), hashed_count);
        } else {
            log::trace!("Hashed {} files to generate hash: {}", hashed_count, &hash[..8.min(hash.len())]);
        }

        Ok(hash)
    }

    /// Compute SHA256 hash of all file contents in watch directories
    /// This is kept for backwards compatibility
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

    fn create_test_spec(temp_dir: &Path, yaml_content: &str) -> rush_build::ComponentBuildSpec {
        // Parse the YAML to extract component details
        let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_content).unwrap();
        let component_name = yaml["component_name"].as_str().unwrap_or("test-component").to_string();
        let location = yaml["location"].as_str().map(|s| s.to_string());

        // Create a simple spec directly
        let build_type = if let Some(loc) = location {
            rush_build::BuildType::RustBinary {
                location: loc,
                dockerfile_path: yaml["dockerfile"].as_str().unwrap_or("Dockerfile").to_string(),
                context_dir: Some(".".to_string()),
                build_script: None,
                features: None,
                precompile_commands: None
            }
        } else {
            rush_build::BuildType::Ingress {
                dockerfile_path: yaml["dockerfile"].as_str().unwrap_or("ingress/Dockerfile").to_string(),
                context_dir: None
            }
        };

        // Create minimal config and variables for testing
        let config = Arc::new(rush_config::Config {
            name: "test-product".to_string(),
            version: "1.0.0".to_string(),
            environment: "local".to_string(),
            docker_registry: None,
            docker_registry_prefix: None,
            docker_config_file: None,
            gcp_project: None,
            gcp_region: None,
            gcp_zone: None,
            k8s_cluster_name: None,
            target_os: None,
            target_arch: None,
            toolchain: None,
            components: vec![],
            ingress: std::collections::HashMap::new(),
            local_services: vec![],
            base_dir: Some(temp_dir.to_path_buf()),
        });

        let variables = Arc::new(rush_build::Variables::empty());

        rush_build::ComponentBuildSpec {
            build_type,
            product_name: "test-product".to_string(),
            component_name,
            color: "blue".to_string(),
            depends_on: vec![],
            build: None,
            mount_point: None,
            subdomain: None,
            artefacts: None,
            artefact_output_dir: "dist".to_string(),
            docker_extra_run_args: vec![],
            env: None,
            volumes: None,
            port: None,
            target_port: None,
            k8s: None,
            priority: 0,
            watch: None,
            config,
            variables,
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: std::collections::HashMap::new(),
            dotenv_secrets: std::collections::HashMap::new(),
            domain: "localhost".to_string(),
            cross_compile: "native".to_string(),
        }
    }

    #[test]
    fn test_get_component_directory() {
        let (generator, temp_dir) = create_test_generator();

        let spec = create_test_spec(temp_dir.path(), r#"
            component_name: test-component
            build_type: RustBinary
            location: backend/server
            dockerfile: backend/Dockerfile
        "#);

        let dir = generator.get_component_directory(&spec);
        assert_eq!(dir, temp_dir.path().join("backend/server"));
    }

    #[test]
    fn test_get_watch_files_and_directories_without_patterns() {
        let (generator, temp_dir) = create_test_generator();

        // Create some test files
        fs::create_dir_all(temp_dir.path().join("backend/server/src")).unwrap();
        fs::write(temp_dir.path().join("backend/server/main.rs"), "content").unwrap();
        fs::write(temp_dir.path().join("backend/server/src/lib.rs"), "content").unwrap();

        let mut spec = create_test_spec(temp_dir.path(), r#"
            component_name: test-component
            build_type: RustBinary
            location: backend/server
            dockerfile: backend/Dockerfile
        "#);
        spec.watch = None; // No watch patterns

        let (files, dirs) = generator.get_watch_files_and_directories(&spec);

        // Should have the component directory
        assert!(dirs.contains(&temp_dir.path().join("backend/server")));

        // Should have found the files
        assert!(files.len() >= 2);
    }

    #[test]
    fn test_get_watch_files_and_directories_with_patterns() {
        let (generator, temp_dir) = create_test_generator();

        // Create backend/server directory structure for the Ingress component
        let backend_dir = temp_dir.path().join("backend/server");
        fs::create_dir_all(backend_dir.join("src")).unwrap();
        fs::write(backend_dir.join("main_app.rs"), "content").unwrap();
        fs::write(backend_dir.join("src/user_api.rs"), "content").unwrap();
        fs::write(backend_dir.join("src/admin_api.rs"), "content").unwrap();
        fs::write(backend_dir.join("src/other.rs"), "content").unwrap();

        let mut spec = create_test_spec(temp_dir.path(), r#"
            component_name: backend
            build_type: RustBinary
            location: backend/server
            dockerfile: backend/Dockerfile
        "#);

        // Add watch patterns
        let patterns = vec!["**/*_app*".to_string(), "**/*_api*".to_string()];
        spec.watch = Some(Arc::new(rush_utils::PathMatcher::new(&backend_dir, patterns)));

        let (files, dirs) = generator.get_watch_files_and_directories(&spec);

        // Should find only files matching the patterns
        assert_eq!(files.len(), 3);
        assert!(files.contains(&backend_dir.join("main_app.rs")));
        assert!(files.contains(&backend_dir.join("src/user_api.rs")));
        assert!(files.contains(&backend_dir.join("src/admin_api.rs")));
        assert!(!files.contains(&backend_dir.join("src/other.rs")));
    }

    #[test]
    fn test_content_hash_from_files_deterministic() {
        let (generator, temp_dir) = create_test_generator();

        // Create test files
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        fs::write(&file1, "content1").unwrap();
        fs::write(&file2, "content2").unwrap();

        let files = vec![file1.clone(), file2.clone()];

        // Compute hash twice
        let hash1 = generator.compute_content_hash_from_files(&files).unwrap();
        let hash2 = generator.compute_content_hash_from_files(&files).unwrap();

        // Should be identical
        assert_eq!(hash1, hash2);

        // Change content
        fs::write(&file1, "different content").unwrap();
        let hash3 = generator.compute_content_hash_from_files(&files).unwrap();

        // Should be different
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_dynamic_file_detection_with_watch_patterns() {
        let (generator, temp_dir) = create_test_generator();

        // Create backend/server directory for RustBinary component
        let backend_dir = temp_dir.path().join("backend/server");
        fs::create_dir_all(&backend_dir).unwrap();

        // Create initial file
        fs::write(backend_dir.join("initial_api.rs"), "content").unwrap();

        let mut spec = create_test_spec(temp_dir.path(), r#"
            component_name: backend
            build_type: RustBinary
            location: backend/server
            dockerfile: backend/Dockerfile
        "#);

        // Add watch patterns
        let patterns = vec!["**/*_api.rs".to_string()];
        spec.watch = Some(Arc::new(rush_utils::PathMatcher::new(&backend_dir, patterns)));

        // First check - should find initial file
        let (files1, _dirs1) = generator.get_watch_files_and_directories(&spec);
        assert_eq!(files1.len(), 1);
        assert!(files1.contains(&backend_dir.join("initial_api.rs")));

        // Add new file matching pattern
        fs::write(backend_dir.join("new_api.rs"), "content").unwrap();

        // Second check - should find both files (dynamic detection)
        let (files2, _dirs2) = generator.get_watch_files_and_directories(&spec);
        assert_eq!(files2.len(), 2);
        assert!(files2.contains(&backend_dir.join("initial_api.rs")));
        assert!(files2.contains(&backend_dir.join("new_api.rs")));
    }

    #[test]
    fn test_tag_changes_with_new_files() {
        let (generator, temp_dir) = create_test_generator();

        // Create backend/server directory for RustBinary component
        let backend_dir = temp_dir.path().join("backend/server");
        fs::create_dir_all(&backend_dir).unwrap();

        // Initialize git repo for testing
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .ok();

        // Create initial file and commit
        fs::write(backend_dir.join("main_app.rs"), "initial").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(temp_dir.path())
            .output()
            .ok();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp_dir.path())
            .output()
            .ok();

        let mut spec = create_test_spec(temp_dir.path(), r#"
            component_name: backend
            build_type: RustBinary
            location: backend/server
            dockerfile: backend/Dockerfile
        "#);

        // Add watch patterns
        let patterns = vec!["**/*_app*".to_string()];
        spec.watch = Some(Arc::new(rush_utils::PathMatcher::new(&backend_dir, patterns)));

        // Compute initial tag
        let tag1 = generator.compute_tag(&spec);

        // Add new file matching pattern (creates dirty state)
        fs::write(backend_dir.join("new_app.rs"), "new content").unwrap();

        // Compute tag again - should be different due to new file
        let tag2 = generator.compute_tag(&spec);

        // Tags should be different when new files are added
        if tag1.is_ok() && tag2.is_ok() {
            assert_ne!(tag1.unwrap(), tag2.unwrap());
        }
    }
}