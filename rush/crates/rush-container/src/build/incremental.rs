//! Incremental build system with content hashing
//!
//! This module provides incremental build capabilities by tracking
//! content changes and only rebuilding what's necessary.

use crate::build::cache::BuildCache;
use rush_build::ComponentBuildSpec;
use rush_core::{Error, Result};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use log::{debug, info};
use serde::{Serialize, Deserialize};
use walkdir::WalkDir;

/// Build state for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildState {
    /// Component name
    pub component: String,
    /// Content hash of all inputs
    pub content_hash: String,
    /// Dependency hashes
    pub dependency_hashes: HashMap<String, String>,
    /// Build timestamp
    pub built_at: SystemTime,
    /// Build duration
    pub build_duration: Duration,
    /// Output image
    pub output_image: String,
    /// Files that were hashed
    pub tracked_files: Vec<PathBuf>,
}

/// Content hasher for efficient file hashing
pub struct ContentHasher {
    /// Cache of file hashes
    file_cache: Arc<RwLock<HashMap<PathBuf, (String, SystemTime)>>>,
    /// Ignore patterns
    ignore_patterns: Vec<String>,
    /// Hash algorithm
    _hasher: Sha256,
}

impl ContentHasher {
    /// Create a new content hasher
    pub fn new(ignore_patterns: Vec<String>) -> Self {
        Self {
            file_cache: Arc::new(RwLock::new(HashMap::new())),
            ignore_patterns,
            _hasher: Sha256::new(),
        }
    }

    /// Compute hash for a component
    pub async fn compute_hash(&self, spec: &ComponentBuildSpec) -> Result<String> {
        let mut hasher = Sha256::new();
        let mut tracked_files = Vec::new();

        // Hash the build specification itself
        hasher.update(spec.component_name.as_bytes());
        hasher.update(format!("{:?}", spec.build_type).as_bytes());

        // Hash dockerfile if it exists
        if let Some(dockerfile) = spec.build_type.dockerfile_path() {
            if Path::new(&dockerfile).exists() {
                let content = fs::read_to_string(&dockerfile)
                    .map_err(|e| Error::FileSystem {
                        path: PathBuf::from(&dockerfile),
                        message: format!("Failed to read dockerfile: {}", e),
                    })?;
                hasher.update(content.as_bytes());
                tracked_files.push(PathBuf::from(dockerfile));
            }
        }

        // Hash source files
        let source_path = spec.build_type.location()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));

        let source_hash = self.hash_directory(&source_path, &mut tracked_files).await?;
        hasher.update(source_hash.as_bytes());

        // Hash environment variables
        if let Some(env) = &spec.env {
            for (key, value) in env {
                hasher.update(key.as_bytes());
                hasher.update(value.as_bytes());
            }
        }

        // Hash build command if present
        if let Some(build_cmd) = &spec.build {
            hasher.update(build_cmd.as_bytes());
        }

        // Hash dependencies
        for dep in &spec.depends_on {
            hasher.update(dep.as_bytes());
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Hash a directory recursively
    async fn hash_directory(&self, path: &Path, tracked_files: &mut Vec<PathBuf>) -> Result<String> {
        let mut dir_hasher = Sha256::new();
        let mut file_hashes = Vec::new();

        // Walk directory and collect file hashes
        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Skip directories and ignored files
            if path.is_dir() || self.should_ignore(path) {
                continue;
            }

            // Get or compute file hash
            let file_hash = self.hash_file(path).await?;
            file_hashes.push((path.to_path_buf(), file_hash));
            tracked_files.push(path.to_path_buf());
        }

        // Sort for consistent hashing
        file_hashes.sort_by(|a, b| a.0.cmp(&b.0));

        // Combine file hashes
        for (path, hash) in file_hashes {
            dir_hasher.update(path.to_string_lossy().as_bytes());
            dir_hasher.update(hash.as_bytes());
        }

        Ok(format!("{:x}", dir_hasher.finalize()))
    }

    /// Hash a single file
    async fn hash_file(&self, path: &Path) -> Result<String> {
        let metadata = fs::metadata(path)
            .map_err(|e| Error::FileSystem {
                path: path.to_path_buf(),
                message: format!("Failed to get metadata: {}", e),
            })?;

        let modified = metadata.modified()
            .unwrap_or(SystemTime::now());

        // Check cache
        {
            let cache = self.file_cache.read().await;
            if let Some((hash, cached_time)) = cache.get(path) {
                if *cached_time == modified {
                    return Ok(hash.clone());
                }
            }
        }

        // Compute hash
        let content = fs::read(path)
            .map_err(|e| Error::FileSystem {
                path: path.to_path_buf(),
                message: format!("Failed to read file: {}", e),
            })?;

        let mut hasher = Sha256::new();
        hasher.update(&content);
        let hash = format!("{:x}", hasher.finalize());

        // Update cache
        {
            let mut cache = self.file_cache.write().await;
            cache.insert(path.to_path_buf(), (hash.clone(), modified));
        }

        Ok(hash)
    }

    /// Check if a path should be ignored
    fn should_ignore(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        for pattern in &self.ignore_patterns {
            if path_str.contains(pattern) {
                return true;
            }
        }

        // Common ignore patterns
        if path_str.contains("/.git/") ||
           path_str.contains("/target/") ||
           path_str.contains("/node_modules/") ||
           path_str.contains("/.rush/") {
            return true;
        }

        false
    }
}

/// Incremental build manager
pub struct IncrementalBuilder {
    /// Previous build states
    previous_states: Arc<RwLock<HashMap<String, BuildState>>>,
    /// Content hasher
    hasher: Arc<ContentHasher>,
    /// Build cache
    _cache: Arc<RwLock<BuildCache>>,
    /// State file path
    state_file: PathBuf,
}

impl IncrementalBuilder {
    /// Create a new incremental builder
    pub fn new(cache_dir: &Path) -> Self {
        let state_file = cache_dir.join("incremental_state.json");
        let previous_states = Self::load_states(&state_file)
            .unwrap_or_default();

        Self {
            previous_states: Arc::new(RwLock::new(previous_states)),
            hasher: Arc::new(ContentHasher::new(vec![
                ".git".to_string(),
                "target".to_string(),
                "node_modules".to_string(),
                ".rush".to_string(),
            ])),
            _cache: Arc::new(RwLock::new(BuildCache::new(cache_dir, cache_dir))),
            state_file,
        }
    }

    /// Load previous build states
    fn load_states(path: &Path) -> Result<HashMap<String, BuildState>> {
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| Error::FileSystem {
                path: path.to_path_buf(),
                message: format!("Failed to read state file: {}", e),
            })?;

        serde_json::from_str(&content)
            .map_err(|e| Error::Serialization(format!("Failed to parse state file: {}", e)))
    }

    /// Save build states
    async fn save_states(&self) -> Result<()> {
        let states = self.previous_states.read().await;
        let content = serde_json::to_string_pretty(&*states)
            .map_err(|e| Error::Serialization(format!("Failed to serialize states: {}", e)))?;

        fs::write(&self.state_file, content)
            .map_err(|e| Error::FileSystem {
                path: self.state_file.clone(),
                message: format!("Failed to write state file: {}", e),
            })?;

        Ok(())
    }

    /// Check if a component needs rebuilding
    pub async fn needs_rebuild(&self, spec: &ComponentBuildSpec) -> Result<bool> {
        let start = Instant::now();

        // Compute current content hash
        let current_hash = self.hasher.compute_hash(spec).await?;

        // Check previous state
        let states = self.previous_states.read().await;
        let needs_rebuild = match states.get(&spec.component_name) {
            Some(state) => {
                // Check if content changed
                if state.content_hash != current_hash {
                    info!("Component {} needs rebuild: content changed", spec.component_name);
                    true
                } else {
                    // Check if dependencies changed
                    for dep in &spec.depends_on {
                        if let Some(dep_state) = states.get(dep) {
                            if let Some(cached_hash) = state.dependency_hashes.get(dep) {
                                if cached_hash != &dep_state.content_hash {
                                    info!("Component {} needs rebuild: dependency {} changed",
                                        spec.component_name, dep);
                                    return Ok(true);
                                }
                            }
                        } else {
                            // Dependency not built yet
                            info!("Component {} needs rebuild: dependency {} not built",
                                spec.component_name, dep);
                            return Ok(true);
                        }
                    }
                    false
                }
            }
            None => {
                info!("Component {} needs rebuild: no previous build", spec.component_name);
                true
            }
        };

        debug!("Incremental build check for {} took {:?}: rebuild={}",
            spec.component_name, start.elapsed(), needs_rebuild);

        Ok(needs_rebuild)
    }

    /// Record a successful build
    pub async fn record_build(
        &self,
        spec: &ComponentBuildSpec,
        output_image: String,
        duration: Duration,
    ) -> Result<()> {
        let content_hash = self.hasher.compute_hash(spec).await?;

        // Collect dependency hashes
        let mut dependency_hashes = HashMap::new();
        let states = self.previous_states.read().await;
        for dep in &spec.depends_on {
            if let Some(dep_state) = states.get(dep) {
                dependency_hashes.insert(dep.clone(), dep_state.content_hash.clone());
            }
        }
        drop(states);

        // Create new build state
        let build_state = BuildState {
            component: spec.component_name.clone(),
            content_hash,
            dependency_hashes,
            built_at: SystemTime::now(),
            build_duration: duration,
            output_image,
            tracked_files: Vec::new(), // TODO: Populate from hasher
        };

        // Update states
        let mut states = self.previous_states.write().await;
        states.insert(spec.component_name.clone(), build_state);
        drop(states);

        // Save to disk
        self.save_states().await?;

        Ok(())
    }

    /// Get build statistics
    pub async fn get_statistics(&self) -> BuildStatistics {
        let states = self.previous_states.read().await;

        let total_components = states.len();
        let total_build_time: Duration = states
            .values()
            .map(|s| s.build_duration)
            .sum();

        let average_build_time = if total_components > 0 {
            total_build_time / total_components as u32
        } else {
            Duration::from_secs(0)
        };

        // Note: cache.stats() needs to be accessed differently
        // For now, use placeholder values

        BuildStatistics {
            total_components,
            total_build_time,
            average_build_time,
            cache_hits: 0,  // TODO: integrate with actual cache stats
            cache_misses: 0,  // TODO: integrate with actual cache stats
            incremental_builds: states
                .values()
                .filter(|s| s.built_at > SystemTime::now() - Duration::from_secs(3600))
                .count(),
        }
    }

    /// Clean old build states
    pub async fn clean_old_states(&self, max_age: Duration) -> Result<()> {
        let cutoff = SystemTime::now() - max_age;

        let mut states = self.previous_states.write().await;
        let before_count = states.len();

        states.retain(|_, state| state.built_at > cutoff);

        let removed = before_count - states.len();
        if removed > 0 {
            info!("Cleaned {} old build states", removed);
            drop(states);
            self.save_states().await?;
        }

        Ok(())
    }
}

/// Build statistics
#[derive(Debug, Clone)]
pub struct BuildStatistics {
    /// Total components tracked
    pub total_components: usize,
    /// Total build time across all components
    pub total_build_time: Duration,
    /// Average build time per component
    pub average_build_time: Duration,
    /// Cache hits
    pub cache_hits: u64,
    /// Cache misses
    pub cache_misses: u64,
    /// Number of incremental builds in last hour
    pub incremental_builds: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_content_hasher() {
        let hasher = ContentHasher::new(vec![]);

        // Create test file
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        // Hash should be consistent
        let hash1 = hasher.hash_file(&test_file).await.unwrap();
        let hash2 = hasher.hash_file(&test_file).await.unwrap();
        assert_eq!(hash1, hash2);

        // Hash should change with content
        fs::write(&test_file, "different content").unwrap();
        let hash3 = hasher.hash_file(&test_file).await.unwrap();
        assert_ne!(hash1, hash3);
    }

    #[tokio::test]
    async fn test_incremental_builder() {
        let temp_dir = TempDir::new().unwrap();
        let builder = IncrementalBuilder::new(temp_dir.path());

        // Initial statistics
        let stats = builder.get_statistics().await;
        assert_eq!(stats.total_components, 0);
    }
}