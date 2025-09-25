//! Persistent build cache with disk storage and LRU eviction
//!
//! This module provides a persistent cache that stores build metadata
//! on disk and implements LRU eviction for space management.

use rush_build::ComponentBuildSpec;
use rush_core::{Error, Result};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Persistent cache configuration
#[derive(Debug, Clone)]
pub struct PersistentCacheConfig {
    /// Cache directory path
    pub cache_dir: PathBuf,
    /// Maximum cache size in bytes
    pub max_size: u64,
    /// Maximum age for cache entries
    pub max_age: Duration,
    /// Enable compression for cached data
    pub compress: bool,
}

impl Default for PersistentCacheConfig {
    fn default() -> Self {
        Self {
            cache_dir: PathBuf::from(".rush/build-cache"),
            max_size: 10 * 1024 * 1024 * 1024, // 10GB
            max_age: Duration::from_secs(7 * 24 * 60 * 60), // 7 days
            compress: true,
        }
    }
}

/// Metadata for a cached build artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildArtifactMetadata {
    /// Component name
    pub component_name: String,
    /// Image name with tag
    pub image_name: String,
    /// Hash of the build context
    pub context_hash: String,
    /// Hash of the component spec
    pub spec_hash: String,
    /// Build timestamp
    pub build_time: u64,
    /// Size of the cached data
    pub size: u64,
    /// List of source files and their hashes
    pub source_files: HashMap<String, String>,
    /// Docker layer cache hints
    pub layer_cache: Vec<String>,
}

/// LRU cache entry tracking
#[derive(Debug, Clone)]
struct LruEntry {
    key: String,
    _size: u64,
    last_accessed: Instant,
}

/// Persistent build cache with disk storage
pub struct PersistentBuildCache {
    config: PersistentCacheConfig,
    /// In-memory index of cached items
    index: Arc<RwLock<HashMap<String, BuildArtifactMetadata>>>,
    /// LRU tracking
    lru_queue: Arc<RwLock<VecDeque<LruEntry>>>,
    /// Current total cache size
    current_size: Arc<RwLock<u64>>,
}

impl PersistentBuildCache {
    /// Create a new persistent build cache
    pub async fn new(config: PersistentCacheConfig) -> Result<Self> {
        // Ensure cache directory exists
        fs::create_dir_all(&config.cache_dir)
            .map_err(|e| Error::FileSystem {
                path: config.cache_dir.clone(),
                message: format!("Failed to create cache directory: {}", e),
            })?;

        let mut cache = Self {
            config,
            index: Arc::new(RwLock::new(HashMap::new())),
            lru_queue: Arc::new(RwLock::new(VecDeque::new())),
            current_size: Arc::new(RwLock::new(0)),
        };

        // Load existing cache index
        cache.load_index().await?;

        // Clean up expired entries
        cache.cleanup_expired().await?;

        Ok(cache)
    }

    /// Load the cache index from disk
    async fn load_index(&mut self) -> Result<()> {
        let index_path = self.config.cache_dir.join("index.json");

        if !index_path.exists() {
            debug!("No cache index found, starting fresh");
            return Ok(());
        }

        let index_data = fs::read_to_string(&index_path)
            .map_err(|e| Error::FileSystem {
                path: index_path.clone(),
                message: format!("Failed to read cache index: {}", e),
            })?;

        let loaded_index: HashMap<String, BuildArtifactMetadata> =
            serde_json::from_str(&index_data)
                .map_err(|e| Error::Serialization(format!("Failed to parse cache index: {}", e)))?;

        let mut index = self.index.write().await;
        let mut lru_queue = self.lru_queue.write().await;
        let mut total_size = 0u64;

        for (key, metadata) in loaded_index {
            // Verify the cached data still exists
            let data_path = self.get_data_path(&key);
            if data_path.exists() {
                total_size += metadata.size;
                lru_queue.push_back(LruEntry {
                    key: key.clone(),
                    _size: metadata.size,
                    last_accessed: Instant::now(),
                });
                index.insert(key, metadata);
            }
        }

        *self.current_size.write().await = total_size;

        info!("Loaded {} cache entries ({:.2} MB)",
            index.len(),
            total_size as f64 / (1024.0 * 1024.0)
        );

        Ok(())
    }

    /// Save the cache index to disk
    async fn save_index(&self) -> Result<()> {
        let index_path = self.config.cache_dir.join("index.json");
        let index = self.index.read().await;

        let index_data = serde_json::to_string_pretty(&*index)
            .map_err(|e| Error::Serialization(format!("Failed to serialize cache index: {}", e)))?;

        fs::write(&index_path, index_data)
            .map_err(|e| Error::FileSystem {
                path: index_path,
                message: format!("Failed to write cache index: {}", e),
            })?;

        Ok(())
    }

    /// Get the cache key for a component
    pub fn compute_cache_key(spec: &ComponentBuildSpec) -> String {
        let mut hasher = Sha256::new();

        // Hash component name and build type
        hasher.update(spec.component_name.as_bytes());
        hasher.update(format!("{:?}", spec.build_type).as_bytes());

        // Hash configuration that affects the build
        if let Some(dockerfile) = spec.build_type.dockerfile_path() {
            hasher.update(dockerfile.as_bytes());
        }

        // Hash environment variables
        for (k, v) in &spec.dotenv {
            hasher.update(k.as_bytes());
            hasher.update(v.as_bytes());
        }

        // Hash build command if any
        if let Some(build) = &spec.build {
            hasher.update(build.as_bytes());
        }

        format!("{:x}", hasher.finalize())
    }

    /// Get the path for cached data
    fn get_data_path(&self, key: &str) -> PathBuf {
        self.config.cache_dir.join(format!("{}.cache", key))
    }

    /// Get the path for cached metadata
    fn get_metadata_path(&self, key: &str) -> PathBuf {
        self.config.cache_dir.join(format!("{}.meta", key))
    }

    /// Get cached build artifact
    pub async fn get(&self, spec: &ComponentBuildSpec) -> Option<BuildArtifactMetadata> {
        let key = Self::compute_cache_key(spec);

        // Check if entry exists in index
        let index = self.index.read().await;
        let metadata = index.get(&key)?;

        // Verify the cached data still exists
        let data_path = self.get_data_path(&key);
        if !data_path.exists() {
            warn!("Cache entry {} exists in index but data file is missing", key);
            return None;
        }

        // Update LRU tracking
        let mut lru_queue = self.lru_queue.write().await;
        if let Some(pos) = lru_queue.iter().position(|e| e.key == key) {
            let mut entry = lru_queue.remove(pos).unwrap();
            entry.last_accessed = Instant::now();
            lru_queue.push_back(entry);
        }

        debug!("Cache hit for component {}", spec.component_name);
        Some(metadata.clone())
    }

    /// Store build artifact in cache
    pub async fn put(&self, spec: &ComponentBuildSpec, image_name: String) -> Result<()> {
        let key = Self::compute_cache_key(spec);

        // Create metadata
        let metadata = BuildArtifactMetadata {
            component_name: spec.component_name.clone(),
            image_name: image_name.clone(),
            context_hash: self.compute_context_hash(spec).await?,
            spec_hash: format!("{:?}", spec),
            build_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            size: 0, // Will be updated after writing data
            source_files: self.collect_source_files(spec).await?,
            layer_cache: Vec::new(),
        };

        // Write metadata
        let metadata_path = self.get_metadata_path(&key);
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|e| Error::Serialization(format!("Failed to serialize metadata: {}", e)))?;

        fs::write(&metadata_path, metadata_json)
            .map_err(|e| Error::FileSystem {
                path: metadata_path.clone(),
                message: format!("Failed to write metadata: {}", e),
            })?;

        // Update metadata with actual size
        let size = metadata_path.metadata()
            .map(|m| m.len())
            .unwrap_or(0);

        let mut final_metadata = metadata;
        final_metadata.size = size;

        // Check if we need to evict old entries
        let mut current_size = self.current_size.write().await;
        if *current_size + size > self.config.max_size {
            self.evict_lru(*current_size + size - self.config.max_size).await?;
        }

        // Update index
        let mut index = self.index.write().await;
        index.insert(key.clone(), final_metadata);

        // Update LRU queue
        let mut lru_queue = self.lru_queue.write().await;
        lru_queue.push_back(LruEntry {
            key: key.clone(),
            _size: size,
            last_accessed: Instant::now(),
        });

        *current_size += size;

        // Save index to disk
        drop(index);
        drop(lru_queue);
        drop(current_size);
        self.save_index().await?;

        info!("Cached build for {} ({:.2} KB)", spec.component_name, size as f64 / 1024.0);
        Ok(())
    }

    /// Compute hash of build context
    async fn compute_context_hash(&self, spec: &ComponentBuildSpec) -> Result<String> {
        let mut hasher = Sha256::new();

        // Hash dockerfile content if available
        if let Some(dockerfile) = spec.build_type.dockerfile_path() {
            let dockerfile_path = PathBuf::from(dockerfile);
            if dockerfile_path.exists() {
                let content = fs::read(&dockerfile_path)
                    .map_err(|e| Error::FileSystem {
                        path: dockerfile_path.clone(),
                        message: format!("Failed to read Dockerfile: {}", e),
                    })?;
                hasher.update(&content);
            }
        }

        // Hash artifact templates
        if let Some(artifacts) = &spec.artefacts {
            for (source, _) in artifacts {
                if Path::new(source).exists() {
                    let content = fs::read_to_string(source)
                        .unwrap_or_default();
                    hasher.update(content.as_bytes());
                }
            }
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Collect source file hashes
    async fn collect_source_files(&self, spec: &ComponentBuildSpec) -> Result<HashMap<String, String>> {
        let mut files = HashMap::new();

        // Collect hashes of relevant source files
        // This is a simplified version - in production, you'd want to
        // walk the entire build context and hash all files

        if let Some(location) = spec.build_type.location() {
            let location_path = PathBuf::from(location);
            if location_path.exists() {
                // Hash a few key files as examples
                for entry in ["Cargo.toml", "package.json", "requirements.txt"] {
                    let file_path = location_path.join(entry);
                    if file_path.exists() {
                        let content = fs::read(&file_path).unwrap_or_default();
                        let mut hasher = Sha256::new();
                        hasher.update(&content);
                        files.insert(
                            entry.to_string(),
                            format!("{:x}", hasher.finalize())
                        );
                    }
                }
            }
        }

        Ok(files)
    }

    /// Evict least recently used entries
    async fn evict_lru(&self, bytes_needed: u64) -> Result<()> {
        let mut index = self.index.write().await;
        let mut lru_queue = self.lru_queue.write().await;
        let mut current_size = self.current_size.write().await;
        let mut freed = 0u64;

        while freed < bytes_needed && !lru_queue.is_empty() {
            if let Some(entry) = lru_queue.pop_front() {
                // Remove from index
                if let Some(metadata) = index.remove(&entry.key) {
                    freed += metadata.size;
                    *current_size -= metadata.size;

                    // Delete files
                    let data_path = self.get_data_path(&entry.key);
                    let metadata_path = self.get_metadata_path(&entry.key);

                    let _ = fs::remove_file(data_path);
                    let _ = fs::remove_file(metadata_path);

                    debug!("Evicted cache entry {} to free {:.2} KB",
                        entry.key, metadata.size as f64 / 1024.0);
                }
            }
        }

        info!("Evicted cache entries to free {:.2} MB", freed as f64 / (1024.0 * 1024.0));
        Ok(())
    }

    /// Clean up expired cache entries
    async fn cleanup_expired(&self) -> Result<()> {
        let mut index = self.index.write().await;
        let mut lru_queue = self.lru_queue.write().await;
        let mut current_size = self.current_size.write().await;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let max_age_secs = self.config.max_age.as_secs();
        let mut removed = Vec::new();

        for (key, metadata) in index.iter() {
            if now - metadata.build_time > max_age_secs {
                removed.push(key.clone());
                *current_size -= metadata.size;

                // Delete files
                let data_path = self.get_data_path(key);
                let metadata_path = self.get_metadata_path(key);

                let _ = fs::remove_file(data_path);
                let _ = fs::remove_file(metadata_path);
            }
        }

        // Remove from index and LRU queue
        for key in &removed {
            index.remove(key);
            lru_queue.retain(|e| e.key != *key);
        }

        if !removed.is_empty() {
            info!("Cleaned up {} expired cache entries", removed.len());
        }

        Ok(())
    }

    /// Invalidate cache entry for a component
    pub async fn invalidate(&self, spec: &ComponentBuildSpec) -> Result<()> {
        let key = Self::compute_cache_key(spec);

        let mut index = self.index.write().await;
        if let Some(metadata) = index.remove(&key) {
            let mut current_size = self.current_size.write().await;
            *current_size -= metadata.size;

            // Remove from LRU queue
            let mut lru_queue = self.lru_queue.write().await;
            lru_queue.retain(|e| e.key != key);

            // Delete files
            let data_path = self.get_data_path(&key);
            let metadata_path = self.get_metadata_path(&key);

            let _ = fs::remove_file(data_path);
            let _ = fs::remove_file(metadata_path);

            info!("Invalidated cache for component {}", spec.component_name);
        }

        Ok(())
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let index = self.index.read().await;
        let current_size = self.current_size.read().await;

        CacheStats {
            entries: index.len(),
            total_size: *current_size,
            max_size: self.config.max_size,
            hit_rate: 0.0, // Would need to track hits/misses for this
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cached entries
    pub entries: usize,
    /// Total size of cache in bytes
    pub total_size: u64,
    /// Maximum cache size
    pub max_size: u64,
    /// Cache hit rate (0.0 - 1.0)
    pub hit_rate: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_persistent_cache_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = PersistentCacheConfig {
            cache_dir: temp_dir.path().to_path_buf(),
            max_size: 1024 * 1024, // 1MB
            max_age: Duration::from_secs(3600),
            compress: false,
        };

        let cache = PersistentBuildCache::new(config).await.unwrap();

        // Verify cache stats are initialized correctly
        let stats = cache.stats().await;
        assert_eq!(stats.entries, 0);
        assert_eq!(stats.total_size, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[tokio::test]
    async fn test_cache_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().join("cache").join("nested");

        let config = PersistentCacheConfig {
            cache_dir: cache_dir.clone(),
            max_size: 1024 * 1024,
            max_age: Duration::from_secs(3600),
            compress: false,
        };

        // Cache should create directory if it doesn't exist
        let cache = PersistentBuildCache::new(config).await.unwrap();
        assert!(cache_dir.exists());

        // Verify we can get stats
        let stats = cache.stats().await;
        assert_eq!(stats.entries, 0);
    }
}