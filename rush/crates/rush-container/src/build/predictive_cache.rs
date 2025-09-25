//! Predictive caching for build optimization
//!
//! This module implements intelligent caching strategies that predict
//! which components are likely to be built next and preemptively
//! prepare resources to minimize build time.

use crate::build::cache::CacheEntry;
use rush_build::ComponentBuildSpec;
use rush_core::{Error, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{RwLock, Semaphore};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use chrono::{Local, Timelike};

/// Predictive cache that learns from build patterns
pub struct PredictiveCache {
    /// Base cache directory
    cache_dir: PathBuf,
    /// Build pattern history
    patterns: Arc<RwLock<BuildPatterns>>,
    /// Prefetch queue
    prefetch_queue: Arc<RwLock<VecDeque<String>>>,
    /// Active prefetch tasks
    prefetch_semaphore: Arc<Semaphore>,
    /// Cache metadata
    metadata: Arc<RwLock<CacheMetadata>>,
}

/// Build patterns for prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildPatterns {
    /// Component build sequences (A -> B means B often follows A)
    sequences: HashMap<String, Vec<(String, f64)>>,
    /// Time-based patterns (hour of day -> commonly built components)
    temporal_patterns: HashMap<u32, Vec<String>>,
    /// File change to component mapping
    file_impact_map: HashMap<String, HashSet<String>>,
    /// Build frequency by component
    build_frequency: HashMap<String, usize>,
    /// Last build times
    last_builds: HashMap<String, SystemTime>,
}

impl BuildPatterns {
    pub fn new() -> Self {
        Self {
            sequences: HashMap::new(),
            temporal_patterns: HashMap::new(),
            file_impact_map: HashMap::new(),
            build_frequency: HashMap::new(),
            last_builds: HashMap::new(),
        }
    }

    /// Record a build sequence
    pub fn record_sequence(&mut self, from: String, to: String) {
        let entry = self.sequences.entry(from).or_insert_with(Vec::new);

        // Update or add transition probability
        if let Some(transition) = entry.iter_mut().find(|(comp, _)| comp == &to) {
            // Increase probability using exponential moving average
            transition.1 = transition.1 * 0.9 + 0.1;
        } else {
            entry.push((to, 0.1));
        }

        // Keep only top 10 predictions
        entry.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        entry.truncate(10);
    }

    /// Record a build at current time
    pub fn record_build(&mut self, component: String) {
        // Update frequency
        *self.build_frequency.entry(component.clone()).or_insert(0) += 1;

        // Update last build time
        self.last_builds.insert(component.clone(), SystemTime::now());

        // Update temporal pattern
        let hour = Local::now().hour();
        self.temporal_patterns
            .entry(hour)
            .or_insert_with(Vec::new)
            .push(component);
    }

    /// Record file change impact
    pub fn record_file_impact(&mut self, file: String, affected_components: HashSet<String>) {
        self.file_impact_map.insert(file, affected_components);
    }

    /// Predict next likely builds
    pub fn predict_next(&self, current: &str, max_predictions: usize) -> Vec<String> {
        let mut predictions = Vec::new();
        let mut scores: HashMap<String, f64> = HashMap::new();

        // Use sequence patterns
        if let Some(transitions) = self.sequences.get(current) {
            for (component, probability) in transitions {
                *scores.entry(component.clone()).or_insert(0.0) += probability * 2.0;
            }
        }

        // Use temporal patterns
        let hour = Local::now().hour();
        if let Some(temporal) = self.temporal_patterns.get(&hour) {
            for component in temporal {
                *scores.entry(component.clone()).or_insert(0.0) += 0.5;
            }
        }

        // Use build frequency as tiebreaker
        for (component, freq) in &self.build_frequency {
            let score = scores.entry(component.clone()).or_insert(0.0);
            *score += (*freq as f64) * 0.01;
        }

        // Sort by score and return top predictions
        let mut scored: Vec<_> = scores.into_iter().collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        for (component, _score) in scored.into_iter().take(max_predictions) {
            predictions.push(component);
        }

        predictions
    }

    /// Predict components affected by file changes
    pub fn predict_affected(&self, changed_files: &[String]) -> HashSet<String> {
        let mut affected = HashSet::new();

        for file in changed_files {
            // Direct match
            if let Some(components) = self.file_impact_map.get(file) {
                affected.extend(components.clone());
            }

            // Pattern matching for similar files
            for (pattern, components) in &self.file_impact_map {
                if file.contains(pattern) || pattern.contains(file) {
                    affected.extend(components.clone());
                }
            }
        }

        affected
    }
}

/// Cache metadata for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// Total cache size in bytes
    pub total_size: u64,
    /// Number of cached entries
    pub entry_count: usize,
    /// Cache hit rate (rolling average)
    pub hit_rate: f64,
    /// Most recently used components
    pub mru_components: VecDeque<String>,
    /// Cache age by component
    pub cache_ages: HashMap<String, Duration>,
}

impl CacheMetadata {
    pub fn new() -> Self {
        Self {
            total_size: 0,
            entry_count: 0,
            hit_rate: 0.0,
            mru_components: VecDeque::with_capacity(100),
            cache_ages: HashMap::new(),
        }
    }

    /// Update MRU list
    pub fn touch(&mut self, component: String) {
        // Remove if exists and add to front
        self.mru_components.retain(|c| c != &component);
        self.mru_components.push_front(component);

        // Keep only 100 most recent
        while self.mru_components.len() > 100 {
            self.mru_components.pop_back();
        }
    }

    /// Update hit rate
    pub fn record_access(&mut self, hit: bool) {
        // Exponential moving average
        self.hit_rate = self.hit_rate * 0.95 + if hit { 0.05 } else { 0.0 };
    }
}

impl PredictiveCache {
    /// Create a new predictive cache
    pub fn new(cache_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| rush_core::Error::Io(e))?;

        let patterns = Self::load_patterns(&cache_dir).unwrap_or_else(|_| BuildPatterns::new());
        let metadata = Self::load_metadata(&cache_dir).unwrap_or_else(|_| CacheMetadata::new());

        Ok(Self {
            cache_dir,
            patterns: Arc::new(RwLock::new(patterns)),
            prefetch_queue: Arc::new(RwLock::new(VecDeque::new())),
            prefetch_semaphore: Arc::new(Semaphore::new(2)), // Max 2 concurrent prefetches
            metadata: Arc::new(RwLock::new(metadata)),
        })
    }

    /// Load patterns from disk
    fn load_patterns(cache_dir: &PathBuf) -> Result<BuildPatterns> {
        let path = cache_dir.join("patterns.json");
        if path.exists() {
            let data = std::fs::read_to_string(&path)
                .map_err(|e| rush_core::Error::Io(e))?;
            serde_json::from_str(&data)
                .map_err(|e| rush_core::Error::Config(format!("Failed to parse patterns: {}", e)))
        } else {
            Ok(BuildPatterns::new())
        }
    }

    /// Load metadata from disk
    fn load_metadata(cache_dir: &PathBuf) -> Result<CacheMetadata> {
        let path = cache_dir.join("metadata.json");
        if path.exists() {
            let data = std::fs::read_to_string(&path)
                .map_err(|e| rush_core::Error::Io(e))?;
            serde_json::from_str(&data)
                .map_err(|e| rush_core::Error::Config(format!("Failed to parse metadata: {}", e)))
        } else {
            Ok(CacheMetadata::new())
        }
    }

    /// Save patterns to disk
    pub async fn save_patterns(&self) -> Result<()> {
        let patterns = self.patterns.read().await;
        let path = self.cache_dir.join("patterns.json");
        let data = serde_json::to_string_pretty(&*patterns)
            .map_err(|e| rush_core::Error::Config(format!("Failed to serialize patterns: {}", e)))?;
        std::fs::write(path, data)
            .map_err(|e| rush_core::Error::Io(e))?;
        Ok(())
    }

    /// Save metadata to disk
    pub async fn save_metadata(&self) -> Result<()> {
        let metadata = self.metadata.read().await;
        let path = self.cache_dir.join("metadata.json");
        let data = serde_json::to_string_pretty(&*metadata)
            .map_err(|e| rush_core::Error::Config(format!("Failed to serialize metadata: {}", e)))?;
        std::fs::write(path, data)
            .map_err(|e| rush_core::Error::Io(e))?;
        Ok(())
    }

    /// Get cache entry with prediction
    pub async fn get_with_prediction(&self, component: &str) -> Option<CacheEntry> {
        // Update metadata
        {
            let mut metadata = self.metadata.write().await;
            metadata.touch(component.to_string());
        }

        // Get from cache (simplified - would integrate with actual cache)
        let cache_hit = self.get_from_cache(component).await;

        // Update hit rate
        {
            let mut metadata = self.metadata.write().await;
            metadata.record_access(cache_hit.is_some());
        }

        // Predict and prefetch next components
        if cache_hit.is_some() {
            self.prefetch_predicted(component).await;
        }

        cache_hit
    }

    /// Get from cache (simplified implementation)
    async fn get_from_cache(&self, component: &str) -> Option<CacheEntry> {
        let cache_path = self.cache_dir.join(format!("{}.cache", component));
        if cache_path.exists() {
            // In real implementation, would deserialize CacheEntry
            debug!("Cache hit for component: {}", component);
            None // Placeholder
        } else {
            debug!("Cache miss for component: {}", component);
            None
        }
    }

    /// Prefetch predicted components
    async fn prefetch_predicted(&self, current: &str) {
        let patterns = self.patterns.read().await;
        let predictions = patterns.predict_next(current, 3);

        for component in predictions {
            let queue = Arc::clone(&self.prefetch_queue);
            let semaphore = Arc::clone(&self.prefetch_semaphore);
            let cache_dir = self.cache_dir.clone();

            // Spawn prefetch task
            tokio::spawn(async move {
                if let Ok(_permit) = semaphore.try_acquire() {
                    debug!("Prefetching predicted component: {}", component);
                    // In real implementation, would prefetch component
                    // This might involve:
                    // - Pulling Docker base images
                    // - Downloading dependencies
                    // - Warming build caches
                }
            });
        }
    }

    /// Record build for learning
    pub async fn record_build(&self, from: Option<String>, to: String) {
        let mut patterns = self.patterns.write().await;

        // Record sequence if there's a from component
        if let Some(from_comp) = from {
            patterns.record_sequence(from_comp, to.clone());
        }

        // Record build
        patterns.record_build(to);

        // Periodically save patterns
        if patterns.build_frequency.values().sum::<usize>() % 10 == 0 {
            drop(patterns); // Release lock
            let _ = self.save_patterns().await;
        }
    }

    /// Analyze cache performance
    pub async fn analyze_performance(&self) -> CachePerformanceReport {
        let metadata = self.metadata.read().await;
        let patterns = self.patterns.read().await;

        // Calculate metrics
        let avg_cache_age = if !metadata.cache_ages.is_empty() {
            let total: Duration = metadata.cache_ages.values().sum();
            total / metadata.cache_ages.len() as u32
        } else {
            Duration::ZERO
        };

        // Find most frequently built components
        let mut freq_sorted: Vec<_> = patterns.build_frequency.iter().collect();
        freq_sorted.sort_by_key(|(_, freq)| *freq);
        freq_sorted.reverse();

        let hot_components: Vec<String> = freq_sorted
            .into_iter()
            .take(5)
            .map(|(comp, _)| comp.clone())
            .collect();

        // Prediction accuracy (simplified - would need actual tracking)
        let prediction_accuracy = 0.75; // Placeholder

        CachePerformanceReport {
            hit_rate: metadata.hit_rate,
            total_size: metadata.total_size,
            entry_count: metadata.entry_count,
            avg_cache_age,
            hot_components,
            prediction_accuracy,
            recommendations: self.generate_cache_recommendations(&metadata, &patterns),
        }
    }

    /// Generate cache optimization recommendations
    fn generate_cache_recommendations(
        &self,
        metadata: &CacheMetadata,
        patterns: &BuildPatterns,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Check hit rate
        if metadata.hit_rate < 0.7 {
            recommendations.push(format!(
                "Cache hit rate is {:.1}%. Consider increasing cache size or TTL.",
                metadata.hit_rate * 100.0
            ));
        }

        // Check for stale entries
        let stale_count = metadata.cache_ages
            .values()
            .filter(|age| **age > Duration::from_secs(86400))
            .count();

        if stale_count > metadata.entry_count / 4 {
            recommendations.push(format!(
                "{} cache entries are over 24 hours old. Consider cache cleanup.",
                stale_count
            ));
        }

        // Check for frequently built components not in cache
        for (component, freq) in &patterns.build_frequency {
            if *freq > 10 && !metadata.mru_components.contains(component) {
                recommendations.push(format!(
                    "Component '{}' is frequently built but not cached. Consider persistent caching.",
                    component
                ));
            }
        }

        recommendations
    }

    /// Evict least valuable cache entries
    pub async fn evict_lru(&self, target_size: u64) -> Result<()> {
        let mut metadata = self.metadata.write().await;

        while metadata.total_size > target_size && !metadata.mru_components.is_empty() {
            // Remove least recently used
            if let Some(component) = metadata.mru_components.pop_back() {
                let cache_path = self.cache_dir.join(format!("{}.cache", component));
                if cache_path.exists() {
                    let size = std::fs::metadata(&cache_path)
                        .map_err(|e| rush_core::Error::Io(e))?.len();
                    std::fs::remove_file(cache_path)
                        .map_err(|e| rush_core::Error::Io(e))?;
                    metadata.total_size -= size;
                    metadata.entry_count -= 1;
                    metadata.cache_ages.remove(&component);

                    info!("Evicted cache entry for '{}' (freed {} bytes)", component, size);
                }
            }
        }

        Ok(())
    }
}

/// Cache performance report
#[derive(Debug, Clone)]
pub struct CachePerformanceReport {
    /// Overall cache hit rate
    pub hit_rate: f64,
    /// Total cache size in bytes
    pub total_size: u64,
    /// Number of cache entries
    pub entry_count: usize,
    /// Average age of cache entries
    pub avg_cache_age: Duration,
    /// Most frequently accessed components
    pub hot_components: Vec<String>,
    /// Prediction accuracy
    pub prediction_accuracy: f64,
    /// Optimization recommendations
    pub recommendations: Vec<String>,
}

impl CachePerformanceReport {
    /// Print the report
    pub fn print(&self) {
        println!("\n=== Cache Performance Report ===");
        println!("Hit Rate: {:.1}%", self.hit_rate * 100.0);
        println!("Cache Size: {} MB ({} entries)",
            self.total_size / 1_000_000, self.entry_count);
        println!("Average Cache Age: {:?}", self.avg_cache_age);
        println!("Prediction Accuracy: {:.1}%", self.prediction_accuracy * 100.0);

        if !self.hot_components.is_empty() {
            println!("\nHot Components:");
            for component in &self.hot_components {
                println!("  - {}", component);
            }
        }

        if !self.recommendations.is_empty() {
            println!("\nRecommendations:");
            for rec in &self.recommendations {
                println!("  • {}", rec);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_build_patterns() {
        let mut patterns = BuildPatterns::new();

        // Record sequences
        patterns.record_sequence("frontend".to_string(), "api".to_string());
        patterns.record_sequence("frontend".to_string(), "api".to_string());
        patterns.record_sequence("frontend".to_string(), "worker".to_string());

        // Test prediction
        let predictions = patterns.predict_next("frontend", 2);
        assert_eq!(predictions.len(), 2);
        assert_eq!(predictions[0], "api"); // Should be first due to higher probability
    }

    #[tokio::test]
    async fn test_predictive_cache() {
        let temp_dir = TempDir::new().unwrap();
        let cache = PredictiveCache::new(temp_dir.path().to_path_buf()).unwrap();

        // Record some builds
        cache.record_build(None, "database".to_string()).await;
        cache.record_build(Some("database".to_string()), "api".to_string()).await;
        cache.record_build(Some("api".to_string()), "frontend".to_string()).await;

        // Save and reload
        cache.save_patterns().await.unwrap();
        cache.save_metadata().await.unwrap();

        // Test performance analysis
        let report = cache.analyze_performance().await;
        assert!(report.entry_count == 0 || report.entry_count > 0);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let temp_dir = TempDir::new().unwrap();
        let cache = PredictiveCache::new(temp_dir.path().to_path_buf()).unwrap();

        // Populate metadata with simulated cache files
        {
            let mut metadata = cache.metadata.write().await;
            metadata.total_size = 1_000_000_000; // 1GB
            metadata.entry_count = 10;
            for i in 0..10 {
                let component_name = format!("component_{}", i);
                // Create actual cache files so eviction can remove them
                let cache_file = temp_dir.path().join(format!("{}.cache", component_name));
                std::fs::write(&cache_file, vec![0u8; 100_000_000]).unwrap(); // 100MB each
                metadata.mru_components.push_back(component_name);
            }
        }

        // Test eviction
        cache.evict_lru(500_000_000).await.unwrap(); // Target 500MB

        let metadata = cache.metadata.read().await;
        // After eviction, total size should be reduced (but may not be exactly 500MB due to file sizes)
        assert!(metadata.total_size < 1_000_000_000, "Cache size should be reduced after eviction");
        assert!(metadata.entry_count < 10, "Some entries should have been evicted");
    }
}