//! Docker operation metrics collection
//!
//! This module provides comprehensive metrics for Docker operations
//! including timing, success rates, and resource usage.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use log::{debug, info};

/// Type of Docker operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    BuildImage,
    CreateNetwork,
    DeleteNetwork,
    NetworkExists,
    RunContainer,
    RunContainerWithCommand,
    StopContainer,
    RemoveContainer,
    ContainerStatus,
    ContainerLogs,
    ContainerExists,
    GetContainerByName,
    PullImage,
    FollowContainerLogs,
    SendSignalToContainer,
    ExecInContainer,
}

impl OperationType {
    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::BuildImage => "build_image",
            Self::CreateNetwork => "create_network",
            Self::DeleteNetwork => "delete_network",
            Self::NetworkExists => "network_exists",
            Self::RunContainer => "run_container",
            Self::RunContainerWithCommand => "run_container_with_command",
            Self::StopContainer => "stop_container",
            Self::RemoveContainer => "remove_container",
            Self::ContainerStatus => "container_status",
            Self::ContainerLogs => "container_logs",
            Self::ContainerExists => "container_exists",
            Self::GetContainerByName => "get_container_by_name",
            Self::PullImage => "pull_image",
            Self::FollowContainerLogs => "follow_container_logs",
            Self::SendSignalToContainer => "send_signal_to_container",
            Self::ExecInContainer => "exec_in_container",
        }
    }
}

/// Metrics for a specific operation type
#[derive(Debug, Clone, Default)]
pub struct OperationMetrics {
    /// Total number of operations
    pub total_count: u64,
    /// Number of successful operations
    pub success_count: u64,
    /// Number of failed operations
    pub failure_count: u64,
    /// Total duration of all operations
    pub total_duration: Duration,
    /// Minimum operation duration
    pub min_duration: Option<Duration>,
    /// Maximum operation duration
    pub max_duration: Option<Duration>,
    /// Average operation duration
    pub avg_duration: Duration,
    /// Number of retried operations
    pub retry_count: u64,
    /// Last operation timestamp
    pub last_operation: Option<Instant>,
}

impl OperationMetrics {
    /// Record a successful operation
    pub fn record_success(&mut self, duration: Duration, retried: bool) {
        self.total_count += 1;
        self.success_count += 1;
        self.total_duration += duration;
        
        if retried {
            self.retry_count += 1;
        }
        
        // Update min/max
        match self.min_duration {
            None => self.min_duration = Some(duration),
            Some(min) if duration < min => self.min_duration = Some(duration),
            _ => {}
        }
        
        match self.max_duration {
            None => self.max_duration = Some(duration),
            Some(max) if duration > max => self.max_duration = Some(duration),
            _ => {}
        }
        
        // Update average
        if self.total_count > 0 {
            self.avg_duration = self.total_duration / self.total_count as u32;
        }
        
        self.last_operation = Some(Instant::now());
    }
    
    /// Record a failed operation
    pub fn record_failure(&mut self, duration: Duration) {
        self.total_count += 1;
        self.failure_count += 1;
        self.total_duration += duration;
        
        // Update min/max even for failures
        match self.min_duration {
            None => self.min_duration = Some(duration),
            Some(min) if duration < min => self.min_duration = Some(duration),
            _ => {}
        }
        
        match self.max_duration {
            None => self.max_duration = Some(duration),
            Some(max) if duration > max => self.max_duration = Some(duration),
            _ => {}
        }
        
        // Update average
        if self.total_count > 0 {
            self.avg_duration = self.total_duration / self.total_count as u32;
        }
        
        self.last_operation = Some(Instant::now());
    }
    
    /// Get success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_count == 0 {
            return 0.0;
        }
        (self.success_count as f64 / self.total_count as f64) * 100.0
    }
    
    /// Get retry rate as a percentage
    pub fn retry_rate(&self) -> f64 {
        if self.success_count == 0 {
            return 0.0;
        }
        (self.retry_count as f64 / self.success_count as f64) * 100.0
    }
}

/// Container resource metrics
#[derive(Debug, Clone)]
pub struct ContainerMetrics {
    /// Container ID
    pub container_id: String,
    /// Component name
    pub component: String,
    /// CPU usage percentage
    pub cpu_percent: f64,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// Memory limit in bytes
    pub memory_limit: u64,
    /// Memory usage percentage
    pub memory_percent: f64,
    /// Network bytes received
    pub network_rx_bytes: u64,
    /// Network bytes transmitted
    pub network_tx_bytes: u64,
    /// Disk read bytes
    pub disk_read_bytes: u64,
    /// Disk write bytes
    pub disk_write_bytes: u64,
    /// Timestamp of metrics
    pub timestamp: Instant,
}

impl Default for ContainerMetrics {
    fn default() -> Self {
        Self {
            container_id: String::new(),
            component: String::new(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            memory_limit: 0,
            memory_percent: 0.0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            disk_read_bytes: 0,
            disk_write_bytes: 0,
            timestamp: Instant::now(),
        }
    }
}

/// Global Docker metrics
#[derive(Debug, Clone)]
pub struct GlobalMetrics {
    /// Start time of metrics collection
    pub start_time: Option<Instant>,
    /// Total operations across all types
    pub total_operations: u64,
    /// Total successful operations
    pub total_successes: u64,
    /// Total failed operations
    pub total_failures: u64,
    /// Total retries
    pub total_retries: u64,
    /// Current active operations
    pub active_operations: usize,
    /// Peak active operations
    pub peak_active_operations: usize,
    /// Total data transferred (bytes)
    pub total_data_transferred: u64,
    /// Number of containers monitored
    pub containers_monitored: usize,
    /// Number of images built
    pub images_built: u64,
    /// Number of networks created
    pub networks_created: u64,
}

impl Default for GlobalMetrics {
    fn default() -> Self {
        Self {
            start_time: None,
            total_operations: 0,
            total_successes: 0,
            total_failures: 0,
            total_retries: 0,
            active_operations: 0,
            peak_active_operations: 0,
            total_data_transferred: 0,
            containers_monitored: 0,
            images_built: 0,
            networks_created: 0,
        }
    }
}

impl GlobalMetrics {
    /// Calculate uptime
    pub fn uptime(&self) -> Duration {
        self.start_time
            .map(|start| start.elapsed())
            .unwrap_or_default()
    }
    
    /// Get overall success rate
    pub fn overall_success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            return 0.0;
        }
        (self.total_successes as f64 / self.total_operations as f64) * 100.0
    }
}

/// Docker metrics collector
pub struct MetricsCollector {
    /// Operation metrics by type
    operation_metrics: Arc<RwLock<HashMap<OperationType, OperationMetrics>>>,
    /// Container metrics by ID
    container_metrics: Arc<RwLock<HashMap<String, ContainerMetrics>>>,
    /// Global metrics
    global_metrics: Arc<RwLock<GlobalMetrics>>,
    /// Whether metrics collection is enabled
    enabled: bool,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(enabled: bool) -> Self {
        let mut global = GlobalMetrics::default();
        if enabled {
            global.start_time = Some(Instant::now());
        }
        
        Self {
            operation_metrics: Arc::new(RwLock::new(HashMap::new())),
            container_metrics: Arc::new(RwLock::new(HashMap::new())),
            global_metrics: Arc::new(RwLock::new(global)),
            enabled,
        }
    }
    
    /// Start recording an operation
    pub async fn start_operation(&self, op_type: OperationType) -> OperationTimer {
        if !self.enabled {
            return OperationTimer::disabled();
        }
        
        // Update active operations
        let mut global = self.global_metrics.write().await;
        global.active_operations += 1;
        if global.active_operations > global.peak_active_operations {
            global.peak_active_operations = global.active_operations;
        }
        
        OperationTimer::new(op_type, self.clone())
    }
    
    /// Record operation completion
    pub async fn record_operation(
        &self,
        op_type: OperationType,
        duration: Duration,
        success: bool,
        retried: bool,
    ) {
        if !self.enabled {
            return;
        }
        
        // Update operation metrics
        let mut metrics = self.operation_metrics.write().await;
        let op_metrics = metrics.entry(op_type).or_insert_with(OperationMetrics::default);
        
        if success {
            op_metrics.record_success(duration, retried);
        } else {
            op_metrics.record_failure(duration);
        }
        
        drop(metrics);
        
        // Update global metrics
        let mut global = self.global_metrics.write().await;
        global.total_operations += 1;
        if success {
            global.total_successes += 1;
        } else {
            global.total_failures += 1;
        }
        if retried {
            global.total_retries += 1;
        }
        if global.active_operations > 0 {
            global.active_operations -= 1;
        }
        
        // Update specific counters
        match op_type {
            OperationType::BuildImage if success => global.images_built += 1,
            OperationType::CreateNetwork if success => global.networks_created += 1,
            _ => {}
        }
        
        debug!(
            "Operation {} completed in {:?} (success: {}, retried: {})",
            op_type.name(),
            duration,
            success,
            retried
        );
    }
    
    /// Update container metrics
    pub async fn update_container_metrics(&self, metrics: ContainerMetrics) {
        if !self.enabled {
            return;
        }
        
        let mut container_metrics = self.container_metrics.write().await;
        container_metrics.insert(metrics.container_id.clone(), metrics);
        
        // Update global container count
        let mut global = self.global_metrics.write().await;
        global.containers_monitored = container_metrics.len();
    }
    
    /// Record data transfer
    pub async fn record_data_transfer(&self, bytes: u64) {
        if !self.enabled {
            return;
        }
        
        let mut global = self.global_metrics.write().await;
        global.total_data_transferred += bytes;
    }
    
    /// Get operation metrics for a specific type
    pub async fn get_operation_metrics(&self, op_type: OperationType) -> Option<OperationMetrics> {
        let metrics = self.operation_metrics.read().await;
        metrics.get(&op_type).cloned()
    }
    
    /// Get all operation metrics
    pub async fn get_all_operation_metrics(&self) -> HashMap<OperationType, OperationMetrics> {
        let metrics = self.operation_metrics.read().await;
        metrics.clone()
    }
    
    /// Get container metrics
    pub async fn get_container_metrics(&self, container_id: &str) -> Option<ContainerMetrics> {
        let metrics = self.container_metrics.read().await;
        metrics.get(container_id).cloned()
    }
    
    /// Get all container metrics
    pub async fn get_all_container_metrics(&self) -> HashMap<String, ContainerMetrics> {
        let metrics = self.container_metrics.read().await;
        metrics.clone()
    }
    
    /// Get global metrics
    pub async fn get_global_metrics(&self) -> GlobalMetrics {
        let metrics = self.global_metrics.read().await;
        metrics.clone()
    }
    
    /// Generate metrics report
    pub async fn generate_report(&self) -> MetricsReport {
        MetricsReport {
            global: self.get_global_metrics().await,
            operations: self.get_all_operation_metrics().await,
            containers: self.get_all_container_metrics().await,
            generated_at: Instant::now(),
        }
    }
    
    /// Reset all metrics
    pub async fn reset(&self) {
        let mut operation_metrics = self.operation_metrics.write().await;
        operation_metrics.clear();
        
        let mut container_metrics = self.container_metrics.write().await;
        container_metrics.clear();
        
        let mut global = self.global_metrics.write().await;
        *global = GlobalMetrics {
            start_time: Some(Instant::now()),
            ..Default::default()
        };
        
        info!("Metrics reset");
    }
    
    /// Clone the collector
    fn clone(&self) -> Self {
        Self {
            operation_metrics: self.operation_metrics.clone(),
            container_metrics: self.container_metrics.clone(),
            global_metrics: self.global_metrics.clone(),
            enabled: self.enabled,
        }
    }
}

/// Timer for tracking operation duration
pub struct OperationTimer {
    op_type: OperationType,
    start_time: Instant,
    collector: Option<MetricsCollector>,
    completed: bool,
}

impl OperationTimer {
    /// Create a new timer
    fn new(op_type: OperationType, collector: MetricsCollector) -> Self {
        Self {
            op_type,
            start_time: Instant::now(),
            collector: Some(collector),
            completed: false,
        }
    }
    
    /// Create a disabled timer
    fn disabled() -> Self {
        Self {
            op_type: OperationType::NetworkExists,
            start_time: Instant::now(),
            collector: None,
            completed: true,
        }
    }
    
    /// Complete the operation
    pub async fn complete(mut self, success: bool, retried: bool) {
        if let Some(collector) = &self.collector {
            let duration = self.start_time.elapsed();
            collector.record_operation(self.op_type, duration, success, retried).await;
            self.completed = true;
        }
    }
    
    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl Drop for OperationTimer {
    fn drop(&mut self) {
        if !self.completed && self.collector.is_some() {
            // Record as failure if not explicitly completed
            let collector = self.collector.take().unwrap();
            let duration = self.start_time.elapsed();
            let op_type = self.op_type;
            
            tokio::spawn(async move {
                collector.record_operation(op_type, duration, false, false).await;
            });
        }
    }
}

/// Complete metrics report
#[derive(Debug, Clone)]
pub struct MetricsReport {
    /// Global metrics
    pub global: GlobalMetrics,
    /// Operation metrics by type
    pub operations: HashMap<OperationType, OperationMetrics>,
    /// Container metrics by ID
    pub containers: HashMap<String, ContainerMetrics>,
    /// When the report was generated
    pub generated_at: Instant,
}

impl MetricsReport {
    /// Generate a summary string
    pub fn summary(&self) -> String {
        format!(
            "Docker Metrics Summary:\n\
             Uptime: {:?}\n\
             Total Operations: {}\n\
             Success Rate: {:.2}%\n\
             Active Operations: {}\n\
             Peak Active: {}\n\
             Containers Monitored: {}\n\
             Images Built: {}\n\
             Networks Created: {}",
            self.global.uptime(),
            self.global.total_operations,
            self.global.overall_success_rate(),
            self.global.active_operations,
            self.global.peak_active_operations,
            self.global.containers_monitored,
            self.global.images_built,
            self.global.networks_created,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = MetricsCollector::new(true);
        
        // Record some operations
        let timer = collector.start_operation(OperationType::NetworkExists).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        timer.complete(true, false).await;
        
        let timer = collector.start_operation(OperationType::BuildImage).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        timer.complete(true, true).await;
        
        let timer = collector.start_operation(OperationType::RunContainer).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        timer.complete(false, false).await;
        
        // Check metrics
        let global = collector.get_global_metrics().await;
        assert_eq!(global.total_operations, 3);
        assert_eq!(global.total_successes, 2);
        assert_eq!(global.total_failures, 1);
        assert_eq!(global.total_retries, 1);
        assert_eq!(global.images_built, 1);
        
        let network_metrics = collector.get_operation_metrics(OperationType::NetworkExists).await;
        assert!(network_metrics.is_some());
        let network_metrics = network_metrics.unwrap();
        assert_eq!(network_metrics.success_count, 1);
        assert_eq!(network_metrics.failure_count, 0);
        assert_eq!(network_metrics.success_rate(), 100.0);
    }
    
    #[test]
    fn test_operation_metrics() {
        let mut metrics = OperationMetrics::default();
        
        metrics.record_success(Duration::from_millis(100), false);
        metrics.record_success(Duration::from_millis(200), true);
        metrics.record_failure(Duration::from_millis(50));
        
        assert_eq!(metrics.total_count, 3);
        assert_eq!(metrics.success_count, 2);
        assert_eq!(metrics.failure_count, 1);
        assert_eq!(metrics.retry_count, 1);
        assert_eq!(metrics.min_duration, Some(Duration::from_millis(50)));
        assert_eq!(metrics.max_duration, Some(Duration::from_millis(200)));
        assert!((metrics.success_rate() - 66.66666666666667).abs() < 0.00001);
        assert_eq!(metrics.retry_rate(), 50.0);
    }
}