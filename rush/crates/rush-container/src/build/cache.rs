//! Build caching for container images
//!
//! This module provides caching functionality to avoid rebuilding
//! unchanged components.

use rush_build::ComponentBuildSpec;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json;

/// Cache entry for a built image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Image name with tag
    pub image_name: String,
    /// Component spec hash for validation
    pub spec_hash: String,
    /// Build timestamp
    pub built_at: chrono::DateTime<chrono::Utc>,
    /// Source files hash
    pub source_hash: String,
    /// Component spec
    #[serde(skip)]
    pub spec: Option<ComponentBuildSpec>,
}

impl CacheEntry {
    /// Create a new cache entry
    pub fn new(image_name: String, spec: ComponentBuildSpec) -> Self {
        Self {
            image_name,
            spec_hash: Self::hash_spec(&spec),
            built_at: chrono::Utc::now(),
            source_hash: String::new(), // TODO: Implement source hashing
            spec: Some(spec),
        }
    }

    /// Hash the component spec for cache validation
    fn hash_spec(spec: &ComponentBuildSpec) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        spec.component_name.hash(&mut hasher);
        // Hash build type to detect changes
        format!("{:?}", spec.build_type).hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Check if the entry is still valid
    pub fn is_valid(&self, spec: &ComponentBuildSpec) -> bool {
        Self::hash_spec(spec) == self.spec_hash
    }
}

/// Statistics for the build cache
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total number of cache entries
    pub total_entries: usize,
    /// Number of cache hits
    pub hits: usize,
    /// Number of cache misses
    pub misses: usize,
    /// Total cache size in bytes
    pub total_size: u64,
    /// Cache hit rate
    pub hit_rate: f64,
}

/// Build cache for managing cached images
pub struct BuildCache {
    /// Cache directory
    cache_dir: PathBuf,
    /// In-memory cache entries
    entries: HashMap<String, CacheEntry>,
    /// Cache statistics
    stats: CacheStats,
    /// Cache expiry duration
    expiry: Duration,
    /// Last cleanup time
    last_cleanup: Instant,
}

impl BuildCache {
    /// Create a new build cache
    pub fn new(cache_dir: &Path) -> Self {
        Self {
            cache_dir: cache_dir.to_path_buf(),
            entries: HashMap::new(),
            stats: CacheStats::default(),
            expiry: Duration::from_secs(3600), // 1 hour default
            last_cleanup: Instant::now(),
        }
    }

    /// Load cache from disk
    pub async fn load(&mut self) -> Result<(), std::io::Error> {
        let cache_file = self.cache_dir.join("cache.json");
        
        if !cache_file.exists() {
            debug!("No cache file found, starting with empty cache");
            return Ok(());
        }
        
        let contents = tokio::fs::read_to_string(&cache_file).await?;
        match serde_json::from_str::<HashMap<String, CacheEntry>>(&contents) {
            Ok(entries) => {
                info!("Loaded {} cache entries", entries.len());
                self.entries = entries;
                self.stats.total_entries = self.entries.len();
            }
            Err(e) => {
                warn!("Failed to parse cache file: {}", e);
                // Start with empty cache on parse error
            }
        }
        
        Ok(())
    }

    /// Save cache to disk
    pub async fn save(&self) -> Result<(), std::io::Error> {
        tokio::fs::create_dir_all(&self.cache_dir).await?;
        
        let cache_file = self.cache_dir.join("cache.json");
        let contents = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        
        tokio::fs::write(&cache_file, contents).await?;
        debug!("Saved {} cache entries", self.entries.len());
        
        Ok(())
    }

    /// Get a cached image
    pub async fn get(&self, component: &str) -> Option<String> {
        if let Some(entry) = self.entries.get(component) {
            debug!("Cache hit for {}", component);
            Some(entry.image_name.clone())
        } else {
            debug!("Cache miss for {}", component);
            None
        }
    }

    /// Put an image in the cache
    pub async fn put(&mut self, component: String, entry: CacheEntry) {
        debug!("Caching image for {}: {}", component, entry.image_name);
        self.entries.insert(component, entry);
        self.stats.total_entries = self.entries.len();
        
        // Save cache periodically
        if let Err(e) = self.save().await {
            warn!("Failed to save cache: {}", e);
        }
    }

    /// Check if a cache entry is expired
    pub async fn is_expired(&self, component: &str) -> bool {
        if let Some(entry) = self.entries.get(component) {
            let age = chrono::Utc::now() - entry.built_at;
            let expired = age > chrono::Duration::from_std(self.expiry).unwrap();
            
            if expired {
                debug!("Cache entry for {} is expired", component);
            }
            
            expired
        } else {
            true // Non-existent entries are considered expired
        }
    }

    /// Invalidate cache entries based on file changes
    pub async fn invalidate_changed(&mut self, changed_files: &[PathBuf]) {
        let mut invalidated = Vec::new();
        
        for (component, entry) in &self.entries {
            // Check if any changed file affects this component
            if let Some(spec) = &entry.spec {
                // Get location from build_type if available
                let location = match &spec.build_type {
                    rush_build::BuildType::RustBinary { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::TrunkWasm { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::DixiousWasm { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::Script { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::Zola { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::Book { location, .. } => Some(location.as_str()),
                    _ => None,
                };
                
                if let Some(loc) = location {
                    for file in changed_files {
                        if file.starts_with(loc) {
                            invalidated.push(component.clone());
                            break;
                        }
                    }
                }
            }
        }
        
        for component in invalidated {
            info!("Invalidating cache for {} due to file changes", component);
            self.entries.remove(&component);
        }
        
        self.stats.total_entries = self.entries.len();
    }

    /// Clear all cache entries
    pub async fn clear(&mut self) {
        info!("Clearing all cache entries");
        self.entries.clear();
        self.stats = CacheStats::default();
        
        // Remove cache file
        let cache_file = self.cache_dir.join("cache.json");
        if cache_file.exists() {
            if let Err(e) = tokio::fs::remove_file(&cache_file).await {
                warn!("Failed to remove cache file: {}", e);
            }
        }
    }

    /// Clean up expired entries
    pub async fn cleanup(&mut self) {
        let now = chrono::Utc::now();
        let mut expired = Vec::new();
        
        for (component, entry) in &self.entries {
            let age = now - entry.built_at;
            if age > chrono::Duration::from_std(self.expiry).unwrap() {
                expired.push(component.clone());
            }
        }
        
        if !expired.is_empty() {
            info!("Removing {} expired cache entries", expired.len());
            for component in expired {
                self.entries.remove(&component);
            }
            self.stats.total_entries = self.entries.len();
            
            // Save updated cache
            if let Err(e) = self.save().await {
                warn!("Failed to save cache after cleanup: {}", e);
            }
        }
        
        self.last_cleanup = Instant::now();
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> CacheStats {
        let mut stats = self.stats.clone();
        
        // Calculate hit rate
        let total_requests = stats.hits + stats.misses;
        if total_requests > 0 {
            stats.hit_rate = (stats.hits as f64) / (total_requests as f64);
        }
        
        stats
    }

    /// Update hit statistics
    pub fn record_hit(&mut self) {
        self.stats.hits += 1;
    }

    /// Update miss statistics
    pub fn record_miss(&mut self) {
        self.stats.misses += 1;
    }

    /// Set cache expiry duration
    pub fn set_expiry(&mut self, expiry: Duration) {
        self.expiry = expiry;
    }

    /// Periodic maintenance (cleanup expired entries)
    pub async fn maintenance(&mut self) {
        // Run cleanup every hour
        if self.last_cleanup.elapsed() > Duration::from_secs(3600) {
            self.cleanup().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::sync::Arc;
    
    // Helper function to create a test ComponentBuildSpec
    fn create_test_spec() -> ComponentBuildSpec {
        ComponentBuildSpec {
            build_type: rush_build::BuildType::PureDockerImage { 
                image_name_with_tag: "test:latest".to_string(),
                command: None,
                entrypoint: None,
            },
            product_name: "test-product".to_string(),
            component_name: "test".to_string(),
            color: "blue".to_string(),
            depends_on: Vec::new(),
            build: None,
            mount_point: None,
            subdomain: None,
            artefacts: None,
            artefact_output_dir: "target".to_string(),
            docker_extra_run_args: Vec::new(),
            env: None,
            volumes: None,
            port: None,
            target_port: None,
            k8s: None,
            priority: 0,
            watch: None,
            config: rush_config::Config::test_default(),
            variables: rush_build::Variables::empty(),
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: HashMap::new(),
            dotenv_secrets: HashMap::new(),
            domain: "test.local".to_string(),
            cross_compile: "native".to_string(),
        }
    }

    #[tokio::test]
    async fn test_cache_entry_creation() {
        let spec = create_test_spec();
        
        let entry = CacheEntry::new("test:v1".to_string(), spec.clone());
        assert_eq!(entry.image_name, "test:v1");
        assert!(entry.is_valid(&spec));
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache = BuildCache::new(temp_dir.path());
        
        let spec = create_test_spec();
        
        // Test put and get
        let entry = CacheEntry::new("test:v1".to_string(), spec);
        cache.put("test".to_string(), entry).await;
        
        let cached = cache.get("test").await;
        assert_eq!(cached, Some("test:v1".to_string()));
        
        // Test miss
        let missing = cache.get("missing").await;
        assert_eq!(missing, None);
    }

    #[tokio::test]
    async fn test_cache_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path();
        
        // Create and save cache
        {
            let mut cache = BuildCache::new(cache_dir);
            
            let spec = create_test_spec();
            
            let entry = CacheEntry::new("test:v1".to_string(), spec);
            cache.put("test".to_string(), entry).await;
            cache.save().await.unwrap();
        }
        
        // Load cache in new instance
        {
            let mut cache = BuildCache::new(cache_dir);
            cache.load().await.unwrap();
            
            let cached = cache.get("test").await;
            assert_eq!(cached, Some("test:v1".to_string()));
        }
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let temp_dir = TempDir::new().unwrap();
        let mut cache = BuildCache::new(temp_dir.path());
        
        // Record some hits and misses
        cache.record_hit();
        cache.record_hit();
        cache.record_miss();
        
        let stats = cache.get_stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 2.0 / 3.0);
    }
}