//! Performance profiling and instrumentation module
//!
//! This module provides comprehensive performance tracking and profiling
//! capabilities for Rush container orchestration.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use tracing::info;
use log::debug;

/// A single timing entry in the performance tracker
#[derive(Debug, Clone)]
pub struct TimingEntry {
    pub operation: String,
    pub component: Option<String>,
    pub duration: Duration,
    pub timestamp: Instant,
    pub metadata: HashMap<String, String>,
}

/// Statistics for a specific operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStats {
    pub count: usize,
    pub total_duration: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
    pub avg_duration: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
}

/// A performance report containing aggregated statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub total_entries: usize,
    pub operation_stats: HashMap<String, OperationStats>,
    pub slowest_operations: Vec<(String, Duration)>,
    pub timeline: Vec<TimelineEntry>,
}

/// An entry in the performance timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub timestamp: Duration,
    pub operation: String,
    pub component: Option<String>,
    pub duration: Duration,
}

/// Performance tracker for collecting timing data
pub struct PerformanceTracker {
    entries: Arc<RwLock<Vec<TimingEntry>>>,
    start_time: Instant,
    enabled: bool,
}

impl PerformanceTracker {
    /// Create a new performance tracker
    pub fn new(enabled: bool) -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            start_time: Instant::now(),
            enabled,
        }
    }

    /// Enable the performance tracker
    pub fn enable(&self) {
        // This is a marker method - the actual enabling happens via environment variable
        // which must be set before the tracker is created
    }

    /// Record a timing entry
    pub async fn record(
        &self,
        operation: &str,
        duration: Duration,
        metadata: HashMap<String, String>,
    ) {
        // Check environment variable dynamically to allow runtime enabling
        if !self.enabled && std::env::var("RUSH_PROFILE").is_err() {
            return;
        }

        // Also emit a tracing event for this timing
        let _span = tracing::span!(
            tracing::Level::INFO,
            "profiling.record",
            operation = %operation,
            duration_ms = duration.as_millis() as u64,
            component = metadata.get("component").map(|s| s.as_str()).unwrap_or("")
        );
        let _enter = _span.enter();

        let entry = TimingEntry {
            operation: operation.to_string(),
            component: metadata.get("component").cloned(),
            duration,
            timestamp: Instant::now(),
            metadata,
        };

        debug!(
            "Performance: {} took {:?} (component: {:?})",
            operation, duration, entry.component
        );

        self.entries.write().await.push(entry);
    }

    /// Record with component name convenience method
    pub async fn record_with_component(
        &self,
        operation: &str,
        component: &str,
        duration: Duration,
    ) {
        // Check environment variable dynamically to allow runtime enabling
        if !self.enabled && std::env::var("RUSH_PROFILE").is_err() {
            return;
        }

        let mut metadata = HashMap::new();
        metadata.insert("component".to_string(), component.to_string());
        self.record(operation, duration, metadata).await;
    }

    /// Start a timing operation
    pub fn start_timing(&self) -> TimingGuard {
        TimingGuard {
            start: Instant::now(),
            tracker: self.clone(),
            operation: None,
            metadata: HashMap::new(),
        }
    }

    /// Generate a performance report
    pub async fn generate_report(&self) -> PerformanceReport {
        let entries = self.entries.read().await;

        // Group by operation
        let mut operations: HashMap<String, Vec<Duration>> = HashMap::new();
        for entry in entries.iter() {
            operations
                .entry(entry.operation.clone())
                .or_default()
                .push(entry.duration);
        }

        // Calculate statistics
        let mut stats = HashMap::new();
        for (op, durations) in operations {
            stats.insert(op, calculate_stats(durations));
        }

        // Find slowest operations
        let mut slowest: Vec<(String, Duration)> = entries
            .iter()
            .map(|e| (format!("{} ({})", e.operation, e.component.as_deref().unwrap_or("N/A")), e.duration))
            .collect();
        slowest.sort_by_key(|(_, d)| std::cmp::Reverse(*d));
        slowest.truncate(10);

        // Generate timeline
        let timeline = generate_timeline(&entries, self.start_time);

        PerformanceReport {
            total_entries: entries.len(),
            operation_stats: stats,
            slowest_operations: slowest,
            timeline,
        }
    }

    /// Clear all entries
    pub async fn clear(&self) {
        self.entries.write().await.clear();
    }

    /// Export entries as JSON
    pub async fn export_json(&self) -> Result<String, serde_json::Error> {
        let report = self.generate_report().await;
        serde_json::to_string_pretty(&report)
    }
}

impl Clone for PerformanceTracker {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            start_time: self.start_time,
            enabled: self.enabled,
        }
    }
}

/// Guard for automatic timing measurement
pub struct TimingGuard {
    start: Instant,
    tracker: PerformanceTracker,
    operation: Option<String>,
    metadata: HashMap<String, String>,
}

impl TimingGuard {
    /// Set the operation name
    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add component metadata
    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.metadata.insert("component".to_string(), component.into());
        self
    }

    /// Complete the timing and record it
    pub async fn complete(self) {
        if let Some(operation) = self.operation {
            let duration = self.start.elapsed();
            self.tracker.record(&operation, duration, self.metadata).await;
        }
    }
}

/// Calculate statistics for a set of durations
fn calculate_stats(mut durations: Vec<Duration>) -> OperationStats {
    if durations.is_empty() {
        return OperationStats {
            count: 0,
            total_duration: Duration::ZERO,
            min_duration: Duration::ZERO,
            max_duration: Duration::ZERO,
            avg_duration: Duration::ZERO,
            p50: Duration::ZERO,
            p95: Duration::ZERO,
            p99: Duration::ZERO,
        };
    }

    durations.sort();
    let count = durations.len();
    let total: Duration = durations.iter().sum();
    let avg = total / count as u32;

    let p50_idx = count / 2;
    let p95_idx = (count * 95) / 100;
    let p99_idx = (count * 99) / 100;

    OperationStats {
        count,
        total_duration: total,
        min_duration: durations[0],
        max_duration: durations[count - 1],
        avg_duration: avg,
        p50: durations[p50_idx.min(count - 1)],
        p95: durations[p95_idx.min(count - 1)],
        p99: durations[p99_idx.min(count - 1)],
    }
}

/// Generate a timeline from entries
fn generate_timeline(entries: &[TimingEntry], start_time: Instant) -> Vec<TimelineEntry> {
    entries
        .iter()
        .map(|e| TimelineEntry {
            timestamp: e.timestamp.duration_since(start_time),
            operation: e.operation.clone(),
            component: e.component.clone(),
            duration: e.duration,
        })
        .collect()
}

/// Global performance tracker instance
static GLOBAL_TRACKER: once_cell::sync::Lazy<PerformanceTracker> =
    once_cell::sync::Lazy::new(|| {
        let enabled = std::env::var("RUSH_PROFILE").is_ok();
        if enabled {
            info!("Performance profiling enabled (RUSH_PROFILE is set)");
        }
        PerformanceTracker::new(enabled)
    });

/// Get the global performance tracker
pub fn global_tracker() -> &'static PerformanceTracker {
    &GLOBAL_TRACKER
}

/// Initialize tracing subscriber with profiling support
/// NOTE: This should only be called if no logger has been set up yet
pub fn init_tracing() {
    // Check if a logger is already set up
    if log::max_level() != log::LevelFilter::Off {
        // Logger already initialized, skip tracing setup to avoid conflicts
        debug!("Skipping tracing init, logger already set up");
        return;
    }

    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer);

    // Profiling feature disabled - remove cfg check to eliminate warning
    // #[cfg(feature = "profiling")]
    // let subscriber = {
    //     if std::env::var("RUSH_FLAMEGRAPH").is_ok() {
    //         let (flame_layer, _guard) = tracing_flame::FlameLayer::new(std::io::stdout());
    //         subscriber.with(flame_layer)
    //     } else {
    //         subscriber
    //     }
    // };

    // Try to init, but don't panic if it fails (logger may be set)
    let _ = subscriber.try_init();
}

/// Macro for timing a block of code
#[macro_export]
macro_rules! time_operation {
    ($tracker:expr, $op:expr, $block:expr) => {{
        let start = std::time::Instant::now();
        let result = $block;
        let duration = start.elapsed();
        $tracker.record($op, duration, std::collections::HashMap::new()).await;
        result
    }};
}