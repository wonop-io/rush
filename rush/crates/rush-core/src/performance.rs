//! Performance monitoring and optimization utilities
//!
//! This module provides tools for measuring and optimizing Rush performance.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Performance metrics for a single operation
#[derive(Debug, Clone)]
pub struct OperationMetrics {
    pub name: String,
    pub duration: Duration,
    pub memory_used: Option<usize>,
    pub timestamp: Instant,
    pub tags: HashMap<String, String>,
}

impl OperationMetrics {
    /// Create new operation metrics
    pub fn new(name: impl Into<String>, duration: Duration) -> Self {
        Self {
            name: name.into(),
            duration,
            memory_used: None,
            timestamp: Instant::now(),
            tags: HashMap::new(),
        }
    }
    
    /// Add a tag to the metrics
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
    
    /// Set memory usage
    pub fn with_memory(mut self, bytes: usize) -> Self {
        self.memory_used = Some(bytes);
        self
    }
}

/// Performance timer for measuring operation duration
pub struct PerfTimer {
    name: String,
    start: Instant,
    tags: HashMap<String, String>,
}

impl PerfTimer {
    /// Start a new performance timer
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: Instant::now(),
            tags: HashMap::new(),
        }
    }
    
    /// Add a tag to the timer
    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
    
    /// Stop the timer and return metrics
    pub fn stop(self) -> OperationMetrics {
        let mut metrics = OperationMetrics::new(self.name, self.start.elapsed());
        metrics.tags = self.tags;
        metrics
    }
    
    /// Get elapsed time without stopping
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Performance monitor for collecting and analyzing metrics
pub struct PerformanceMonitor {
    metrics: Arc<RwLock<Vec<OperationMetrics>>>,
    thresholds: Arc<RwLock<HashMap<String, Duration>>>,
}

impl PerformanceMonitor {
    /// Create a new performance monitor
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
            thresholds: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Record operation metrics
    pub async fn record(&self, metrics: OperationMetrics) {
        // Check if operation exceeded threshold
        let thresholds = self.thresholds.read().await;
        if let Some(threshold) = thresholds.get(&metrics.name) {
            if metrics.duration > *threshold {
                log::warn!(
                    "Operation '{}' took {:?}, exceeding threshold of {:?}",
                    metrics.name,
                    metrics.duration,
                    threshold
                );
            }
        }
        drop(thresholds);
        
        let mut all_metrics = self.metrics.write().await;
        all_metrics.push(metrics);
        
        // Keep only last 1000 metrics to prevent unbounded growth
        if all_metrics.len() > 1000 {
            let drain_count = all_metrics.len() - 1000;
            all_metrics.drain(0..drain_count);
        }
    }
    
    /// Set a performance threshold for an operation
    pub async fn set_threshold(&self, operation: impl Into<String>, threshold: Duration) {
        let mut thresholds = self.thresholds.write().await;
        thresholds.insert(operation.into(), threshold);
    }
    
    /// Get statistics for a specific operation
    pub async fn get_stats(&self, operation: &str) -> OperationStats {
        let metrics = self.metrics.read().await;
        
        let operation_metrics: Vec<_> = metrics
            .iter()
            .filter(|m| m.name == operation)
            .collect();
        
        if operation_metrics.is_empty() {
            return OperationStats::default();
        }
        
        let total_duration: Duration = operation_metrics
            .iter()
            .map(|m| m.duration)
            .sum();
        
        let count = operation_metrics.len();
        let avg_duration = total_duration / count as u32;
        
        let min_duration = operation_metrics
            .iter()
            .map(|m| m.duration)
            .min()
            .unwrap_or_default();
        
        let max_duration = operation_metrics
            .iter()
            .map(|m| m.duration)
            .max()
            .unwrap_or_default();
        
        OperationStats {
            count,
            total_duration,
            avg_duration,
            min_duration,
            max_duration,
        }
    }
    
    /// Get all recorded metrics
    pub async fn get_all_metrics(&self) -> Vec<OperationMetrics> {
        self.metrics.read().await.clone()
    }
    
    /// Clear all metrics
    pub async fn clear(&self) {
        self.metrics.write().await.clear();
    }
    
    /// Generate a performance report
    pub async fn generate_report(&self) -> PerformanceReport {
        let metrics = self.metrics.read().await;
        
        let mut operations = HashMap::new();
        
        for metric in metrics.iter() {
            let stats = operations
                .entry(metric.name.clone())
                .or_insert_with(OperationStats::default);
            
            stats.count += 1;
            stats.total_duration += metric.duration;
        }
        
        // Calculate averages
        for stats in operations.values_mut() {
            if stats.count > 0 {
                stats.avg_duration = stats.total_duration / stats.count as u32;
            }
        }
        
        // Find slow operations
        let slow_operations: Vec<_> = metrics
            .iter()
            .filter(|m| m.duration > Duration::from_secs(1))
            .cloned()
            .collect();
        
        PerformanceReport {
            total_operations: metrics.len(),
            unique_operations: operations.len(),
            operations,
            slow_operations,
        }
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for a specific operation
#[derive(Debug, Clone, Default)]
pub struct OperationStats {
    pub count: usize,
    pub total_duration: Duration,
    pub avg_duration: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
}

/// Performance report
#[derive(Debug, Clone)]
pub struct PerformanceReport {
    pub total_operations: usize,
    pub unique_operations: usize,
    pub operations: HashMap<String, OperationStats>,
    pub slow_operations: Vec<OperationMetrics>,
}

/// Macro for timing a block of code
#[macro_export]
macro_rules! time_operation {
    ($name:expr, $block:block) => {{
        let timer = $crate::performance::PerfTimer::start($name);
        let result = $block;
        let metrics = timer.stop();
        $crate::performance::global_monitor().record(metrics).await;
        result
    }};
}

/// Memory profiler for tracking memory usage
pub struct MemoryProfiler {
    baseline: Option<usize>,
}

impl MemoryProfiler {
    /// Create a new memory profiler
    pub fn new() -> Self {
        Self { baseline: None }
    }
    
    /// Set the baseline memory usage
    pub fn set_baseline(&mut self) {
        self.baseline = Some(Self::current_memory());
    }
    
    /// Get current memory usage in bytes
    pub fn current_memory() -> usize {
        // This is a simplified implementation
        // In production, you'd use platform-specific APIs
        #[cfg(target_os = "linux")]
        {
            Self::linux_memory_usage()
        }
        #[cfg(not(target_os = "linux"))]
        {
            0 // Placeholder for other platforms
        }
    }
    
    #[cfg(target_os = "linux")]
    fn linux_memory_usage() -> usize {
        use std::fs;
        
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<usize>() {
                            return kb * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
        0
    }
    
    /// Get memory delta from baseline
    pub fn delta(&self) -> Option<isize> {
        self.baseline.map(|baseline| {
            Self::current_memory() as isize - baseline as isize
        })
    }
}

impl Default for MemoryProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource limits for operations
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_memory_mb: Option<usize>,
    pub max_duration: Option<Duration>,
    pub max_concurrent_operations: Option<usize>,
}

impl ResourceLimits {
    /// Create new resource limits
    pub fn new() -> Self {
        Self {
            max_memory_mb: None,
            max_duration: None,
            max_concurrent_operations: None,
        }
    }
    
    /// Set maximum memory usage in MB
    pub fn with_max_memory(mut self, mb: usize) -> Self {
        self.max_memory_mb = Some(mb);
        self
    }
    
    /// Set maximum operation duration
    pub fn with_max_duration(mut self, duration: Duration) -> Self {
        self.max_duration = Some(duration);
        self
    }
    
    /// Set maximum concurrent operations
    pub fn with_max_concurrent(mut self, count: usize) -> Self {
        self.max_concurrent_operations = Some(count);
        self
    }
    
    /// Check if memory limit is exceeded
    pub fn check_memory(&self) -> bool {
        if let Some(max_mb) = self.max_memory_mb {
            let current_mb = MemoryProfiler::current_memory() / (1024 * 1024);
            if current_mb > max_mb {
                log::error!(
                    "Memory limit exceeded: {} MB > {} MB",
                    current_mb,
                    max_mb
                );
                return false;
            }
        }
        true
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::new()
    }
}

/// Global performance monitor instance
static PERFORMANCE_MONITOR: once_cell::sync::Lazy<PerformanceMonitor> = 
    once_cell::sync::Lazy::new(PerformanceMonitor::new);

/// Get the global performance monitor
pub fn global_monitor() -> &'static PerformanceMonitor {
    &PERFORMANCE_MONITOR
}

/// Helper function to measure async operation performance
pub async fn measure<F, R>(name: impl Into<String>, f: F) -> (R, OperationMetrics)
where
    F: std::future::Future<Output = R>,
{
    let timer = PerfTimer::start(name);
    let result = f.await;
    let metrics = timer.stop();
    (result, metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_perf_timer() {
        let timer = PerfTimer::start("test_operation")
            .tag("component", "test");
        
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        let metrics = timer.stop();
        assert_eq!(metrics.name, "test_operation");
        assert!(metrics.duration >= Duration::from_millis(10));
        assert_eq!(metrics.tags.get("component"), Some(&"test".to_string()));
    }
    
    #[tokio::test]
    async fn test_performance_monitor() {
        let monitor = PerformanceMonitor::new();
        
        // Record some metrics
        monitor.record(OperationMetrics::new("op1", Duration::from_millis(100))).await;
        monitor.record(OperationMetrics::new("op1", Duration::from_millis(200))).await;
        monitor.record(OperationMetrics::new("op2", Duration::from_millis(50))).await;
        
        // Get stats for op1
        let stats = monitor.get_stats("op1").await;
        assert_eq!(stats.count, 2);
        assert_eq!(stats.avg_duration, Duration::from_millis(150));
        
        // Generate report
        let report = monitor.generate_report().await;
        assert_eq!(report.total_operations, 3);
        assert_eq!(report.unique_operations, 2);
    }
    
    #[tokio::test]
    async fn test_measure_helper() {
        let (result, metrics) = measure("async_op", async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            42
        }).await;
        
        assert_eq!(result, 42);
        assert_eq!(metrics.name, "async_op");
        assert!(metrics.duration >= Duration::from_millis(10));
    }
}