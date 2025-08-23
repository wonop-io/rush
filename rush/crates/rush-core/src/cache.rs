//! High-performance caching layer for Rush
//!
//! This module provides various caching strategies to optimize
//! frequently accessed data and expensive computations.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::Result;

/// Cache entry with metadata
#[derive(Debug, Clone)]
pub struct CacheEntry<V> {
    /// The cached value
    pub value: V,
    /// When the entry was created
    pub created_at: Instant,
    /// When the entry was last accessed
    pub last_accessed: Instant,
    /// Number of times accessed
    pub access_count: u64,
    /// Optional TTL for this entry
    pub ttl: Option<Duration>,
}

impl<V> CacheEntry<V> {
    /// Create a new cache entry
    pub fn new(value: V, ttl: Option<Duration>) -> Self {
        let now = Instant::now();
        Self {
            value,
            created_at: now,
            last_accessed: now,
            access_count: 1,
            ttl,
        }
    }
    
    /// Check if the entry has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            self.created_at.elapsed() > ttl
        } else {
            false
        }
    }
    
    /// Update access metadata
    pub fn touch(&mut self) {
        self.last_accessed = Instant::now();
        self.access_count += 1;
    }
}

/// Trait for cache backends
#[async_trait]
pub trait CacheBackend<K, V>: Send + Sync
where
    K: Hash + Eq + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// Get a value from the cache
    async fn get(&self, key: &K) -> Option<V>;
    
    /// Insert a value into the cache
    async fn insert(&self, key: K, value: V, ttl: Option<Duration>);
    
    /// Remove a value from the cache
    async fn remove(&self, key: &K) -> Option<V>;
    
    /// Clear all entries from the cache
    async fn clear(&self);
    
    /// Get the number of entries in the cache
    async fn len(&self) -> usize;
    
    /// Check if the cache is empty
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }
}

/// In-memory cache with TTL support
pub struct MemoryCache<K, V> {
    entries: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    max_size: Option<usize>,
    default_ttl: Option<Duration>,
}

impl<K, V> MemoryCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create a new memory cache
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_size: None,
            default_ttl: None,
        }
    }
    
    /// Create a cache with a maximum size
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_size: Some(max_size),
            default_ttl: None,
        }
    }
    
    /// Set the default TTL for entries
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }
    
    /// Evict expired entries
    async fn evict_expired(&self) {
        let mut entries = self.entries.write().await;
        entries.retain(|_, entry| !entry.is_expired());
    }
    
    /// Evict least recently used entries if over capacity
    async fn evict_lru(&self) {
        if let Some(max_size) = self.max_size {
            let mut entries = self.entries.write().await;
            
            while entries.len() > max_size {
                // Find the least recently used entry
                let lru_key = entries
                    .iter()
                    .min_by_key(|(_, entry)| entry.last_accessed)
                    .map(|(k, _)| k.clone());
                
                if let Some(key) = lru_key {
                    entries.remove(&key);
                } else {
                    break;
                }
            }
        }
    }
}

impl<K, V> Default for MemoryCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<K, V> CacheBackend<K, V> for MemoryCache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    async fn get(&self, key: &K) -> Option<V> {
        self.evict_expired().await;
        
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(key) {
            if !entry.is_expired() {
                entry.touch();
                return Some(entry.value.clone());
            } else {
                entries.remove(key);
            }
        }
        None
    }
    
    async fn insert(&self, key: K, value: V, ttl: Option<Duration>) {
        let ttl = ttl.or(self.default_ttl);
        let entry = CacheEntry::new(value, ttl);
        
        {
            let mut entries = self.entries.write().await;
            entries.insert(key, entry);
        }
        
        self.evict_lru().await;
    }
    
    async fn remove(&self, key: &K) -> Option<V> {
        let mut entries = self.entries.write().await;
        entries.remove(key).map(|e| e.value)
    }
    
    async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }
    
    async fn len(&self) -> usize {
        let entries = self.entries.read().await;
        entries.len()
    }
}

/// LRU (Least Recently Used) cache
pub struct LruCache<K, V> {
    cache: MemoryCache<K, V>,
}

impl<K, V> LruCache<K, V>
where
    K: Hash + Eq + Clone,
    V: Clone,
{
    /// Create a new LRU cache with the specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: MemoryCache::with_max_size(capacity),
        }
    }
}

#[async_trait]
impl<K, V> CacheBackend<K, V> for LruCache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    async fn get(&self, key: &K) -> Option<V> {
        self.cache.get(key).await
    }
    
    async fn insert(&self, key: K, value: V, ttl: Option<Duration>) {
        self.cache.insert(key, value, ttl).await
    }
    
    async fn remove(&self, key: &K) -> Option<V> {
        self.cache.remove(key).await
    }
    
    async fn clear(&self) {
        self.cache.clear().await
    }
    
    async fn len(&self) -> usize {
        self.cache.len().await
    }
}

/// Cache decorator for async functions
pub struct CachedFunction<K, V, F> {
    cache: Arc<dyn CacheBackend<K, V>>,
    function: F,
    ttl: Option<Duration>,
}

impl<K, V, F, Fut> CachedFunction<K, V, F>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    F: Fn(K) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<V>> + Send,
{
    /// Create a new cached function
    pub fn new(cache: Arc<dyn CacheBackend<K, V>>, function: F) -> Self {
        Self {
            cache,
            function,
            ttl: None,
        }
    }
    
    /// Set the TTL for cached values
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }
    
    /// Call the function with caching
    pub async fn call(&self, key: K) -> Result<V> {
        // Check cache first
        if let Some(value) = self.cache.get(&key).await {
            return Ok(value);
        }
        
        // Call the function
        let value = (self.function)(key.clone()).await?;
        
        // Cache the result
        self.cache.insert(key, value.clone(), self.ttl).await;
        
        Ok(value)
    }
}

/// Build cache for storing build artifacts
pub struct BuildCache {
    cache: Arc<MemoryCache<String, BuildArtifact>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildArtifact {
    pub component: String,
    pub version: String,
    pub hash: String,
    pub timestamp: u64,
    pub size: u64,
}

impl BuildCache {
    /// Create a new build cache
    pub fn new() -> Self {
        Self {
            cache: Arc::new(
                MemoryCache::new()
                    .with_ttl(Duration::from_secs(3600)) // 1 hour TTL
            ),
        }
    }
    
    /// Get a build artifact from cache
    pub async fn get(&self, component: &str) -> Option<BuildArtifact> {
        self.cache.get(&component.to_string()).await
    }
    
    /// Store a build artifact in cache
    pub async fn store(&self, artifact: BuildArtifact) {
        self.cache.insert(
            artifact.component.clone(),
            artifact,
            None,
        ).await;
    }
    
    /// Check if a component needs rebuilding
    pub async fn needs_rebuild(&self, component: &str, current_hash: &str) -> bool {
        match self.get(component).await {
            Some(artifact) => artifact.hash != current_hash,
            None => true,
        }
    }
    
    /// Clear the build cache
    pub async fn clear(&self) {
        self.cache.clear().await;
    }
}

impl Default for BuildCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration cache for frequently accessed configs
pub struct ConfigCache {
    cache: Arc<MemoryCache<String, serde_json::Value>>,
}

impl ConfigCache {
    /// Create a new config cache
    pub fn new() -> Self {
        Self {
            cache: Arc::new(
                MemoryCache::new()
                    .with_ttl(Duration::from_secs(300)) // 5 minute TTL
            ),
        }
    }
    
    /// Get a config value
    pub async fn get(&self, key: &str) -> Option<serde_json::Value> {
        self.cache.get(&key.to_string()).await
    }
    
    /// Set a config value
    pub async fn set(&self, key: String, value: serde_json::Value) {
        self.cache.insert(key, value, None).await;
    }
    
    /// Invalidate a config entry
    pub async fn invalidate(&self, key: &str) {
        self.cache.remove(&key.to_string()).await;
    }
    
    /// Clear all cached configs
    pub async fn clear(&self) {
        self.cache.clear().await;
    }
}

impl Default for ConfigCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Global cache instances
static BUILD_CACHE: once_cell::sync::Lazy<BuildCache> = 
    once_cell::sync::Lazy::new(BuildCache::new);

static CONFIG_CACHE: once_cell::sync::Lazy<ConfigCache> = 
    once_cell::sync::Lazy::new(ConfigCache::new);

/// Get the global build cache
pub fn build_cache() -> &'static BuildCache {
    &BUILD_CACHE
}

/// Get the global config cache
pub fn config_cache() -> &'static ConfigCache {
    &CONFIG_CACHE
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_memory_cache() {
        let cache = MemoryCache::<String, String>::new();
        
        // Insert and get
        cache.insert("key1".to_string(), "value1".to_string(), None).await;
        assert_eq!(cache.get(&"key1".to_string()).await, Some("value1".to_string()));
        
        // Remove
        assert_eq!(cache.remove(&"key1".to_string()).await, Some("value1".to_string()));
        assert_eq!(cache.get(&"key1".to_string()).await, None);
    }
    
    #[tokio::test]
    async fn test_ttl_expiration() {
        let cache = MemoryCache::<String, String>::new();
        
        // Insert with short TTL
        cache.insert(
            "key1".to_string(),
            "value1".to_string(),
            Some(Duration::from_millis(50)),
        ).await;
        
        // Should exist immediately
        assert_eq!(cache.get(&"key1".to_string()).await, Some("value1".to_string()));
        
        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(60)).await;
        
        // Should be expired
        assert_eq!(cache.get(&"key1".to_string()).await, None);
    }
    
    #[tokio::test]
    async fn test_lru_eviction() {
        let cache = LruCache::<String, String>::new(2);
        
        // Fill cache
        cache.insert("key1".to_string(), "value1".to_string(), None).await;
        cache.insert("key2".to_string(), "value2".to_string(), None).await;
        
        // Access key1 to make it more recently used
        cache.get(&"key1".to_string()).await;
        
        // Insert third item, should evict key2
        cache.insert("key3".to_string(), "value3".to_string(), None).await;
        
        assert_eq!(cache.get(&"key1".to_string()).await, Some("value1".to_string()));
        assert_eq!(cache.get(&"key2".to_string()).await, None); // Evicted
        assert_eq!(cache.get(&"key3".to_string()).await, Some("value3".to_string()));
    }
    
    #[tokio::test]
    async fn test_cached_function() {
        let cache = Arc::new(MemoryCache::<i32, String>::new());
        let call_count = Arc::new(RwLock::new(0));
        let call_count_clone = call_count.clone();
        
        let cached_fn = CachedFunction::new(cache.clone(), move |key: i32| {
            let call_count = call_count_clone.clone();
            async move {
                let mut count = call_count.write().await;
                *count += 1;
                Ok(format!("value_{}", key))
            }
        });
        
        // First call should execute function
        assert_eq!(cached_fn.call(1).await.unwrap(), "value_1");
        assert_eq!(*call_count.read().await, 1);
        
        // Second call should use cache
        assert_eq!(cached_fn.call(1).await.unwrap(), "value_1");
        assert_eq!(*call_count.read().await, 1); // Not incremented
        
        // Different key should execute function
        assert_eq!(cached_fn.call(2).await.unwrap(), "value_2");
        assert_eq!(*call_count.read().await, 2);
    }
}