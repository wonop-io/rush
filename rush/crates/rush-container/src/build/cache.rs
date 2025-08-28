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
        Self::new_with_base_dir(image_name, spec, &std::env::current_dir().unwrap_or_default())
    }
    
    /// Create a new cache entry with explicit base directory
    pub fn new_with_base_dir(image_name: String, spec: ComponentBuildSpec, base_dir: &Path) -> Self {
        Self {
            image_name,
            spec_hash: Self::hash_spec(&spec),
            built_at: chrono::Utc::now(),
            source_hash: Self::compute_source_hash(&spec, base_dir),
            spec: Some(spec),
        }
    }
    
    /// Compute hash of source files and artifacts
    fn compute_source_hash(spec: &ComponentBuildSpec, base_dir: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Hash artifact template content if available
        if let Some(artifacts) = &spec.artefacts {
            for (source_path, _) in artifacts {
                // Try to read and hash the artifact template file
                if let Ok(content) = std::fs::read_to_string(source_path) {
                    content.hash(&mut hasher);
                }
            }
        }
        
        // For ingress, also hash the rendered nginx.conf if it exists
        if matches!(spec.build_type, rush_build::BuildType::Ingress { .. }) {
            let nginx_path = base_dir.join("target/rushd/nginx.conf");
            if let Ok(content) = std::fs::read_to_string(&nginx_path) {
                content.hash(&mut hasher);
            }
        }
        
        format!("{:x}", hasher.finish())
    }

    /// Hash the component spec for cache validation
    fn hash_spec(spec: &ComponentBuildSpec) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        spec.component_name.hash(&mut hasher);
        
        // Hash build type to detect changes
        format!("{:?}", spec.build_type).hash(&mut hasher);
        
        // Hash artifacts to detect template changes
        if let Some(artifacts) = &spec.artefacts {
            let mut sorted_artifacts: Vec<_> = artifacts.iter().collect();
            sorted_artifacts.sort_by_key(|(k, _)| k.as_str());
            for (source, target) in sorted_artifacts {
                source.hash(&mut hasher);
                target.hash(&mut hasher);
            }
        }
        
        // Hash port configuration (important for ingress)
        if let Some(port) = spec.port {
            port.hash(&mut hasher);
        }
        if let Some(target_port) = spec.target_port {
            target_port.hash(&mut hasher);
        }
        
        // Hash mount point (affects routing)
        if let Some(mount_point) = &spec.mount_point {
            mount_point.hash(&mut hasher);
        }
        
        format!("{:x}", hasher.finish())
    }

    /// Check if the entry is still valid
    pub fn is_valid(&self, spec: &ComponentBuildSpec, base_dir: &Path) -> bool {
        // Check both spec hash and source content hash
        if Self::hash_spec(spec) != self.spec_hash {
            debug!("Cache entry invalid: spec hash mismatch");
            return false;
        }
        
        // Also check if source files have changed
        let current_source_hash = Self::compute_source_hash(spec, base_dir);
        if current_source_hash != self.source_hash {
            debug!("Cache entry invalid: source hash mismatch (was: {}, now: {})", 
                self.source_hash, current_source_hash);
            return false;
        }
        
        true
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
    /// Base directory for the product (used for path resolution)
    base_dir: PathBuf,
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
    pub fn new(cache_dir: &Path, base_dir: &Path) -> Self {
        Self {
            cache_dir: cache_dir.to_path_buf(),
            base_dir: base_dir.to_path_buf(),
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

    /// Get a cached image if valid
    pub async fn get(&self, component: &str, spec: &ComponentBuildSpec) -> Option<String> {
        if let Some(entry) = self.entries.get(component) {
            // Validate the cache entry against current spec
            if entry.is_valid(spec, &self.base_dir) {
                info!("[CACHE HIT] Component '{}': Using cached image '{}' built at {}", 
                    component, entry.image_name, entry.built_at);
                Some(entry.image_name.clone())
            } else {
                info!("[CACHE MISS] Component '{}': Cache entry invalid (spec or source changed)", component);
                None
            }
        } else {
            info!("[CACHE MISS] Component '{}': No cached image found", component);
            None
        }
    }
    
    /// Get a cached image without validation (for backwards compatibility)
    pub async fn get_unchecked(&self, component: &str) -> Option<String> {
        if let Some(entry) = self.entries.get(component) {
            Some(entry.image_name.clone())
        } else {
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
    
    /// Put an image in the cache with proper base_dir context
    pub async fn put_with_spec(&mut self, component: String, image_name: String, spec: ComponentBuildSpec) {
        let entry = CacheEntry::new_with_base_dir(image_name, spec, &self.base_dir);
        self.put(component, entry).await;
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
        
        info!("[CACHE] Checking {} changed files for cache invalidation", changed_files.len());
        for file in changed_files {
            debug!("[CACHE]   Changed file: {}", file.display());
        }
        
        for (component, entry) in &self.entries {
            // Check if any changed file affects this component
            if let Some(spec) = &entry.spec {
                let mut should_invalidate = false;
                
                // Get location from build_type if available
                let location = match &spec.build_type {
                    rush_build::BuildType::RustBinary { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::TrunkWasm { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::DixiousWasm { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::Script { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::Zola { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::Book { location, .. } => Some(location.as_str()),
                    rush_build::BuildType::Ingress { .. } => Some("./ingress"), // Ingress typically uses ./ingress directory
                    _ => None,
                };
                
                // Check location-based changes
                if let Some(loc) = location {
                    // Convert relative location to absolute path for comparison
                    let abs_location = if Path::new(loc).is_absolute() {
                        PathBuf::from(loc)
                    } else {
                        self.base_dir.join(loc)
                    };
                    
                    debug!("[CACHE] Checking component '{}' with location: {}", 
                        component, abs_location.display());
                    
                    for file in changed_files {
                        if file.starts_with(&abs_location) {
                            info!("[CACHE INVALIDATE] Component '{}': File '{}' changed in component directory '{}'", 
                                component, file.display(), abs_location.display());
                            should_invalidate = true;
                            break;
                        }
                    }
                }
                
                // Check artifact template changes
                if !should_invalidate && spec.artefacts.is_some() {
                    if let Some(artifacts) = &spec.artefacts {
                        for (source_path, _) in artifacts {
                            let abs_artifact_path = if Path::new(source_path).is_absolute() {
                                PathBuf::from(source_path)
                            } else {
                                self.base_dir.join(source_path)
                            };
                            
                            for file in changed_files {
                                if file == &abs_artifact_path || file.ends_with(source_path) {
                                    info!("[CACHE INVALIDATE] Component '{}': Artifact template '{}' changed", 
                                        component, source_path);
                                    should_invalidate = true;
                                    break;
                                }
                            }
                            if should_invalidate {
                                break;
                            }
                        }
                    }
                }
                
                // For ingress, also check if nginx.conf in target/rushd changed (rendered artifact)
                if !should_invalidate && matches!(spec.build_type, rush_build::BuildType::Ingress { .. }) {
                    let rendered_nginx = self.base_dir.join("target/rushd/nginx.conf");
                    for file in changed_files {
                        if file == &rendered_nginx {
                            info!("[CACHE INVALIDATE] Component '{}': Rendered nginx.conf changed", component);
                            should_invalidate = true;
                            break;
                        }
                    }
                }
                
                if should_invalidate {
                    invalidated.push(component.clone());
                } else {
                    debug!("[CACHE] Component '{}' not affected by file changes", component);
                }
            } else {
                debug!("[CACHE] Component '{}' has no build spec", component);
            }
        }
        
        if invalidated.is_empty() {
            info!("[CACHE] No components invalidated by file changes");
        } else {
            info!("[CACHE] Invalidating {} components: {:?}", invalidated.len(), invalidated);
        }
        
        for component in invalidated {
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
        let base_dir = temp_dir.path().join("product");
        std::fs::create_dir(&base_dir).unwrap();
        let mut cache = BuildCache::new(temp_dir.path(), &base_dir);
        
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
        let base_dir = temp_dir.path().join("product");
        std::fs::create_dir(&base_dir).unwrap();
        
        // Create and save cache
        {
            let mut cache = BuildCache::new(cache_dir, &base_dir);
            
            let spec = create_test_spec();
            
            let entry = CacheEntry::new("test:v1".to_string(), spec);
            cache.put("test".to_string(), entry).await;
            cache.save().await.unwrap();
        }
        
        // Load cache in new instance
        {
            let mut cache = BuildCache::new(cache_dir, &base_dir);
            cache.load().await.unwrap();
            
            let cached = cache.get("test").await;
            assert_eq!(cached, Some("test:v1".to_string()));
        }
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("product");
        std::fs::create_dir(&base_dir).unwrap();
        let mut cache = BuildCache::new(temp_dir.path(), &base_dir);
        
        // Record some hits and misses
        cache.record_hit();
        cache.record_hit();
        cache.record_miss();
        
        let stats = cache.get_stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 2.0 / 3.0);
    }

    #[tokio::test]
    async fn test_cache_invalidation_with_file_changes() {
        let temp_dir = TempDir::new().unwrap();
        let base_dir = temp_dir.path().join("product");
        std::fs::create_dir(&base_dir).unwrap();
        
        // Create component directory structure
        let frontend_dir = base_dir.join("frontend/webui");
        std::fs::create_dir_all(&frontend_dir).unwrap();
        
        let mut cache = BuildCache::new(temp_dir.path(), &base_dir);
        
        // Create a spec with location
        let spec = ComponentBuildSpec {
            build_type: rush_build::BuildType::TrunkWasm { 
                location: "frontend/webui".to_string(),
                dockerfile_path: "frontend/Dockerfile".to_string(),
                context_dir: Some("frontend".to_string()),
                ssr: false,
                features: None,
                precompile_commands: None,
            },
            component_name: "frontend".to_string(),
            ..create_test_spec()
        };
        
        // Add entry to cache
        let entry = CacheEntry::new("frontend:v1".to_string(), spec);
        cache.put("frontend".to_string(), entry).await;
        
        // Verify entry exists
        assert_eq!(cache.get("frontend").await, Some("frontend:v1".to_string()));
        
        // Test 1: File change in component directory should invalidate
        let changed_file = base_dir.join("frontend/webui/src/main.rs");
        cache.invalidate_changed(&[changed_file]).await;
        assert_eq!(cache.get("frontend").await, None, "Cache should be invalidated for file in component directory");
        
        // Re-add entry for next test
        let spec2 = ComponentBuildSpec {
            build_type: rush_build::BuildType::TrunkWasm { 
                location: "frontend/webui".to_string(),
                dockerfile_path: "frontend/Dockerfile".to_string(),
                context_dir: Some("frontend".to_string()),
                ssr: false,
                features: None,
                precompile_commands: None,
            },
            component_name: "frontend".to_string(),
            ..create_test_spec()
        };
        let entry2 = CacheEntry::new("frontend:v2".to_string(), spec2);
        cache.put("frontend".to_string(), entry2).await;
        
        // Test 2: File change outside component directory should NOT invalidate
        let unrelated_file = base_dir.join("backend/src/main.rs");
        cache.invalidate_changed(&[unrelated_file]).await;
        assert_eq!(cache.get("frontend").await, Some("frontend:v2".to_string()), 
            "Cache should NOT be invalidated for file outside component directory");
    }
}