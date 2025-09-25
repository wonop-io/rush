//! Audit logging for deployment operations
//!
//! This module provides comprehensive audit logging for all deployment operations,
//! tracking who deployed what, when, and the results.

use chrono::{DateTime, Utc};
use rush_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use log::{debug, info};

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEventType {
    DeploymentStarted,
    DeploymentSucceeded,
    DeploymentFailed,
    DeploymentRolledBack,
    ManifestApplied,
    ManifestDeleted,
    ImageBuilt,
    ImagePushed,
    ValidationPassed,
    ValidationFailed,
    HookExecuted,
    ConfigChanged,
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique event ID
    pub id: String,
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Type of event
    pub event_type: AuditEventType,
    /// User who initiated the action
    pub user: String,
    /// Product name
    pub product: String,
    /// Environment
    pub environment: String,
    /// Version or tag being deployed
    pub version: String,
    /// Whether this was a dry run
    pub dry_run: bool,
    /// Success status
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Additional metadata
    pub metadata: serde_json::Value,
    /// Duration in seconds
    pub duration_seconds: Option<f64>,
}

impl AuditEntry {
    pub fn new(
        event_type: AuditEventType,
        product: String,
        environment: String,
        version: String,
    ) -> Self {
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());
        
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type,
            user,
            product,
            environment,
            version,
            dry_run: false,
            success: true,
            error: None,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            duration_seconds: None,
        }
    }
    
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
    
    pub fn with_error(mut self, error: String) -> Self {
        self.success = false;
        self.error = Some(error);
        self
    }
    
    pub fn with_duration(mut self, duration: std::time::Duration) -> Self {
        self.duration_seconds = Some(duration.as_secs_f64());
        self
    }
}

/// Audit logger trait for different storage backends
pub trait AuditLogger: Send + Sync {
    /// Log an audit entry
    fn log(&self, entry: &AuditEntry) -> Result<()>;
    
    /// Query audit logs
    fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>>;
    
    /// Get audit logs for a specific deployment
    fn get_deployment_history(&self, product: &str, environment: &str) -> Result<Vec<AuditEntry>>;
}

/// File-based audit logger
pub struct FileAuditLogger {
    log_dir: PathBuf,
    max_entries_per_file: usize,
}

impl FileAuditLogger {
    pub fn new(log_dir: PathBuf) -> Result<Self> {
        // Ensure log directory exists
        fs::create_dir_all(&log_dir)
            .map_err(|e| Error::Audit(format!("Failed to create audit log directory: {}", e)))?;
        
        Ok(Self {
            log_dir,
            max_entries_per_file: 10000,
        })
    }
    
    fn get_log_file(&self) -> PathBuf {
        let date = Utc::now().format("%Y-%m-%d");
        self.log_dir.join(format!("audit-{}.jsonl", date))
    }
    
    fn rotate_if_needed(&self, file_path: &Path) -> Result<()> {
        if !file_path.exists() {
            return Ok(());
        }
        
        // Count lines in file
        let content = fs::read_to_string(file_path)
            .map_err(|e| Error::Audit(format!("Failed to read audit log: {}", e)))?;
        
        let line_count = content.lines().count();
        
        if line_count >= self.max_entries_per_file {
            // Rotate file
            let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
            let rotated_path = file_path.with_file_name(
                format!("audit-{}-{}.jsonl", 
                    file_path.file_stem().unwrap().to_string_lossy(),
                    timestamp)
            );
            
            fs::rename(file_path, rotated_path)
                .map_err(|e| Error::Audit(format!("Failed to rotate audit log: {}", e)))?;
        }
        
        Ok(())
    }
}

impl AuditLogger for FileAuditLogger {
    fn log(&self, entry: &AuditEntry) -> Result<()> {
        let file_path = self.get_log_file();
        
        // Rotate if needed
        self.rotate_if_needed(&file_path)?;
        
        // Serialize entry to JSON
        let json = serde_json::to_string(entry)
            .map_err(|e| Error::Serialization(format!("Failed to serialize audit entry: {}", e)))?;
        
        // Append to file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(|e| Error::Audit(format!("Failed to open audit log: {}", e)))?;
        
        writeln!(file, "{}", json)
            .map_err(|e| Error::Audit(format!("Failed to write audit log: {}", e)))?;
        
        debug!("Logged audit event: {:?}", entry.event_type);
        
        Ok(())
    }
    
    fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEntry>> {
        let mut results = Vec::new();
        
        // Read all log files in directory
        let entries = fs::read_dir(&self.log_dir)
            .map_err(|e| Error::Audit(format!("Failed to read audit log directory: {}", e)))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| Error::Audit(format!("Failed to read directory entry: {}", e)))?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                let content = fs::read_to_string(&path)
                    .map_err(|e| Error::Audit(format!("Failed to read audit log: {}", e)))?;
                
                for line in content.lines() {
                    if line.is_empty() {
                        continue;
                    }
                    
                    let entry: AuditEntry = serde_json::from_str(line)
                        .map_err(|e| Error::Serialization(format!("Failed to parse audit entry: {}", e)))?;
                    
                    if filter.matches(&entry) {
                        results.push(entry);
                    }
                }
            }
        }
        
        // Sort by timestamp (newest first)
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(results)
    }
    
    fn get_deployment_history(&self, product: &str, environment: &str) -> Result<Vec<AuditEntry>> {
        let filter = AuditFilter::new()
            .with_product(product.to_string())
            .with_environment(environment.to_string())
            .with_event_types(vec![
                AuditEventType::DeploymentStarted,
                AuditEventType::DeploymentSucceeded,
                AuditEventType::DeploymentFailed,
                AuditEventType::DeploymentRolledBack,
            ]);
        
        self.query(&filter)
    }
}

/// Filter for querying audit logs
#[derive(Debug, Clone)]
pub struct AuditFilter {
    pub product: Option<String>,
    pub environment: Option<String>,
    pub user: Option<String>,
    pub event_types: Option<Vec<AuditEventType>>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub success_only: bool,
    pub limit: Option<usize>,
}

impl AuditFilter {
    pub fn new() -> Self {
        Self {
            product: None,
            environment: None,
            user: None,
            event_types: None,
            start_time: None,
            end_time: None,
            success_only: false,
            limit: None,
        }
    }
    
    pub fn with_product(mut self, product: String) -> Self {
        self.product = Some(product);
        self
    }
    
    pub fn with_environment(mut self, environment: String) -> Self {
        self.environment = Some(environment);
        self
    }
    
    pub fn with_user(mut self, user: String) -> Self {
        self.user = Some(user);
        self
    }
    
    pub fn with_event_types(mut self, types: Vec<AuditEventType>) -> Self {
        self.event_types = Some(types);
        self
    }
    
    pub fn with_time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }
    
    pub fn success_only(mut self) -> Self {
        self.success_only = true;
        self
    }
    
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
    
    fn matches(&self, entry: &AuditEntry) -> bool {
        // Check product filter
        if let Some(ref product) = self.product {
            if entry.product != *product {
                return false;
            }
        }
        
        // Check environment filter
        if let Some(ref env) = self.environment {
            if entry.environment != *env {
                return false;
            }
        }
        
        // Check user filter
        if let Some(ref user) = self.user {
            if entry.user != *user {
                return false;
            }
        }
        
        // Check event type filter
        if let Some(ref types) = self.event_types {
            if !types.iter().any(|t| std::mem::discriminant(t) == std::mem::discriminant(&entry.event_type)) {
                return false;
            }
        }
        
        // Check time range
        if let Some(start) = self.start_time {
            if entry.timestamp < start {
                return false;
            }
        }
        
        if let Some(end) = self.end_time {
            if entry.timestamp > end {
                return false;
            }
        }
        
        // Check success filter
        if self.success_only && !entry.success {
            return false;
        }
        
        true
    }
}

/// Audit manager that coordinates audit logging
pub struct AuditManager {
    logger: Box<dyn AuditLogger>,
    start_time: std::time::Instant,
}

impl AuditManager {
    pub fn new(logger: Box<dyn AuditLogger>) -> Self {
        Self {
            logger,
            start_time: std::time::Instant::now(),
        }
    }
    
    /// Create with default file logger
    pub fn with_file_logger(log_dir: PathBuf) -> Result<Self> {
        let logger = FileAuditLogger::new(log_dir)?;
        Ok(Self::new(Box::new(logger)))
    }
    
    /// Log a deployment started event
    pub fn log_deployment_started(&self, product: &str, environment: &str, version: &str) -> Result<()> {
        let entry = AuditEntry::new(
            AuditEventType::DeploymentStarted,
            product.to_string(),
            environment.to_string(),
            version.to_string(),
        );
        
        self.logger.log(&entry)
    }
    
    /// Log a deployment succeeded event
    pub fn log_deployment_succeeded(&self, product: &str, environment: &str, version: &str) -> Result<()> {
        let duration = self.start_time.elapsed();
        let entry = AuditEntry::new(
            AuditEventType::DeploymentSucceeded,
            product.to_string(),
            environment.to_string(),
            version.to_string(),
        ).with_duration(duration);
        
        self.logger.log(&entry)
    }
    
    /// Log a deployment failed event
    pub fn log_deployment_failed(&self, product: &str, environment: &str, version: &str, error: String) -> Result<()> {
        let duration = self.start_time.elapsed();
        let entry = AuditEntry::new(
            AuditEventType::DeploymentFailed,
            product.to_string(),
            environment.to_string(),
            version.to_string(),
        ).with_error(error)
        .with_duration(duration);
        
        self.logger.log(&entry)
    }
    
    /// Log a custom event
    pub fn log_event(&self, entry: AuditEntry) -> Result<()> {
        self.logger.log(&entry)
    }
    
    /// Get deployment history
    pub fn get_deployment_history(&self, product: &str, environment: &str) -> Result<Vec<AuditEntry>> {
        self.logger.get_deployment_history(product, environment)
    }
    
    /// Query audit logs
    pub fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEntry>> {
        self.logger.query(&filter)
    }
}

/// Integration with monitoring systems
pub struct MonitoringIntegration {
    /// Prometheus push gateway URL
    prometheus_url: Option<String>,
    /// DataDog API key
    datadog_api_key: Option<String>,
}

impl MonitoringIntegration {
    pub fn new() -> Self {
        Self {
            prometheus_url: std::env::var("PROMETHEUS_PUSH_GATEWAY").ok(),
            datadog_api_key: std::env::var("DATADOG_API_KEY").ok(),
        }
    }
    
    /// Send metrics to Prometheus
    pub async fn send_prometheus_metrics(&self, _entry: &AuditEntry) -> Result<()> {
        if let Some(ref url) = self.prometheus_url {
            // TODO: Implement Prometheus metrics push
            info!("Would send metrics to Prometheus: {}", url);
        }
        Ok(())
    }
    
    /// Send events to DataDog
    pub async fn send_datadog_event(&self, _entry: &AuditEntry) -> Result<()> {
        if let Some(ref api_key) = self.datadog_api_key {
            // TODO: Implement DataDog event sending
            info!("Would send event to DataDog with key: {}...", &api_key[..8]);
        }
        Ok(())
    }
}