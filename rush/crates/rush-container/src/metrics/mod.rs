//! Performance metrics and monitoring for container orchestration
//!
//! This module provides comprehensive metrics collection for tracking
//! container startup performance, health check durations, and system reliability.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use log::{info, debug};

/// Component status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ComponentStatus {
    Pending,
    Starting,
    WaitingForHealth,
    Healthy,
    Failed,
    Degraded,
}

/// Metrics for a single component (internal tracking)
#[derive(Debug, Clone)]
struct ComponentMetricsInternal {
    name: String,
    startup_began: Option<Instant>,
    container_created: Option<Instant>,
    health_check_began: Option<Instant>,
    became_healthy: Option<Instant>,
    health_check_attempts: u32,
    status: ComponentStatus,
    error: Option<String>,
    dependencies: Vec<String>,
    wave_number: Option<usize>,
}

/// Metrics for a single component (serializable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentMetrics {
    pub name: String,
    pub time_to_healthy_ms: Option<u64>,
    pub health_check_attempts: u32,
    pub status: ComponentStatus,
    pub error: Option<String>,
    pub dependencies: Vec<String>,
    pub wave_number: Option<usize>,
}

/// Overall startup metrics (serializable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupMetrics {
    pub total_duration_ms: Option<u64>,
    pub total_waves: usize,
    pub total_components: usize,
    pub successful_components: usize,
    pub failed_components: usize,
    pub avg_health_check_duration_ms: Option<u64>,
    pub longest_startup: Option<(String, u64)>,
    pub success_rate: f64,
}

/// Metrics collector for container orchestration
pub struct MetricsCollector {
    components: Arc<RwLock<HashMap<String, ComponentMetricsInternal>>>,
    startup_began: Arc<RwLock<Instant>>,
    enabled: bool,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(enabled: bool) -> Self {
        Self {
            components: Arc::new(RwLock::new(HashMap::new())),
            startup_began: Arc::new(RwLock::new(Instant::now())),
            enabled,
        }
    }

    /// Record startup beginning
    pub async fn record_startup_begin(&self, total_components: usize, total_waves: usize) {
        if !self.enabled {
            return;
        }

        let mut startup = self.startup_began.write().await;
        *startup = Instant::now();

        info!(
            "Metrics: Starting {} components in {} waves",
            total_components, total_waves
        );
    }

    /// Record component startup
    pub async fn record_component_start(&self, name: &str, wave_number: usize, dependencies: Vec<String>) {
        if !self.enabled {
            return;
        }

        let now = Instant::now();
        let mut components = self.components.write().await;

        components.insert(name.to_string(), ComponentMetricsInternal {
            name: name.to_string(),
            startup_began: Some(now),
            container_created: None,
            health_check_began: None,
            became_healthy: None,
            health_check_attempts: 0,
            status: ComponentStatus::Starting,
            error: None,
            dependencies,
            wave_number: Some(wave_number),
        });

        debug!("Metrics: Component {} starting in wave {}", name, wave_number);
    }

    /// Record container creation
    pub async fn record_container_created(&self, name: &str) {
        if !self.enabled {
            return;
        }

        let mut components = self.components.write().await;
        if let Some(metrics) = components.get_mut(name) {
            metrics.container_created = Some(Instant::now());
            debug!("Metrics: Container created for {}", name);
        }
    }

    /// Record health check beginning
    pub async fn record_health_check_start(&self, name: &str) {
        if !self.enabled {
            return;
        }

        let mut components = self.components.write().await;
        if let Some(metrics) = components.get_mut(name) {
            metrics.health_check_began = Some(Instant::now());
            metrics.status = ComponentStatus::WaitingForHealth;
            debug!("Metrics: Health check started for {}", name);
        }
    }

    /// Record health check attempt
    pub async fn record_health_check_attempt(&self, name: &str) {
        if !self.enabled {
            return;
        }

        let mut components = self.components.write().await;
        if let Some(metrics) = components.get_mut(name) {
            metrics.health_check_attempts += 1;
            debug!("Metrics: Health check attempt {} for {}", metrics.health_check_attempts, name);
        }
    }

    /// Record component becoming healthy
    pub async fn record_component_healthy(&self, name: &str) {
        if !self.enabled {
            return;
        }

        let now = Instant::now();
        let mut components = self.components.write().await;

        if let Some(metrics) = components.get_mut(name) {
            metrics.became_healthy = Some(now);
            metrics.status = ComponentStatus::Healthy;

            if let Some(start) = metrics.startup_began {
                let duration = now - start;
                info!(
                    "Metrics: Component {} became healthy in {:?} after {} health checks",
                    name,
                    duration,
                    metrics.health_check_attempts
                );
            }
        }
    }

    /// Record component failure
    pub async fn record_component_failed(&self, name: &str, error: String) {
        if !self.enabled {
            return;
        }

        let mut components = self.components.write().await;
        if let Some(metrics) = components.get_mut(name) {
            metrics.status = ComponentStatus::Failed;
            metrics.error = Some(error.clone());
            info!("Metrics: Component {} failed: {}", name, error);
        }
    }

    /// Record wave start
    pub async fn record_wave_start(&self, wave_number: usize, components: Vec<String>) {
        if !self.enabled {
            return;
        }

        info!(
            "Metrics: Wave {} starting with {} components",
            wave_number,
            components.len()
        );
    }

    /// Record wave completion
    pub async fn record_wave_complete(&self, wave_number: usize) {
        if !self.enabled {
            return;
        }

        info!("Metrics: Wave {} completed", wave_number);
    }

    /// Record overall startup completion
    pub async fn record_startup_complete(&self) {
        if !self.enabled {
            return;
        }

        let startup_began = *self.startup_began.read().await;
        let duration = Instant::now() - startup_began;

        let components = self.components.read().await;
        let successful = components.values().filter(|m| m.status == ComponentStatus::Healthy).count();
        let failed = components.values().filter(|m| m.status == ComponentStatus::Failed).count();

        let success_rate = if !components.is_empty() {
            successful as f64 / components.len() as f64
        } else {
            0.0
        };

        info!(
            "Metrics: Startup completed in {:?} with {:.1}% success rate ({}/{} healthy)",
            duration,
            success_rate * 100.0,
            successful,
            components.len()
        );
    }

    /// Export metrics as JSON
    pub async fn export_json(&self) -> String {
        let components = self.components.read().await;
        let startup_began = *self.startup_began.read().await;
        let now = Instant::now();
        let total_duration = now - startup_began;

        // Convert internal metrics to serializable format
        let mut component_metrics = HashMap::new();
        for (name, internal) in components.iter() {
            let time_to_healthy_ms = internal.startup_began
                .and_then(|start| internal.became_healthy.map(|end| (end - start).as_millis() as u64));

            component_metrics.insert(name.clone(), ComponentMetrics {
                name: internal.name.clone(),
                time_to_healthy_ms,
                health_check_attempts: internal.health_check_attempts,
                status: internal.status.clone(),
                error: internal.error.clone(),
                dependencies: internal.dependencies.clone(),
                wave_number: internal.wave_number,
            });
        }

        let successful = components.values().filter(|m| m.status == ComponentStatus::Healthy).count();
        let failed = components.values().filter(|m| m.status == ComponentStatus::Failed).count();

        let startup_metrics = StartupMetrics {
            total_duration_ms: Some(total_duration.as_millis() as u64),
            total_waves: component_metrics.values()
                .filter_map(|m| m.wave_number)
                .max()
                .unwrap_or(0) + 1,
            total_components: component_metrics.len(),
            successful_components: successful,
            failed_components: failed,
            avg_health_check_duration_ms: None, // Calculated if needed
            longest_startup: component_metrics.values()
                .filter_map(|m| m.time_to_healthy_ms.map(|d| (m.name.clone(), d)))
                .max_by_key(|(_, d)| *d),
            success_rate: if !component_metrics.is_empty() {
                successful as f64 / component_metrics.len() as f64
            } else {
                0.0
            },
        };

        #[derive(Serialize)]
        struct MetricsExport {
            startup: StartupMetrics,
            components: HashMap<String, ComponentMetrics>,
        }

        let export = MetricsExport {
            startup: startup_metrics,
            components: component_metrics,
        };

        serde_json::to_string_pretty(&export).unwrap_or_else(|_| "{}".to_string())
    }

    /// Export metrics as Prometheus format
    pub async fn export_prometheus(&self) -> String {
        let components = self.components.read().await;
        let startup_began = *self.startup_began.read().await;
        let total_duration = (Instant::now() - startup_began).as_secs_f64();

        let mut output = String::new();

        // Overall metrics
        output.push_str(&format!(
            "# HELP rush_startup_duration_seconds Total startup duration\n\
             # TYPE rush_startup_duration_seconds gauge\n\
             rush_startup_duration_seconds {}\n",
            total_duration
        ));

        // Per-component metrics
        for (name, metrics) in components.iter() {
            if let Some(start) = metrics.startup_began {
                if let Some(end) = metrics.became_healthy {
                    let duration = (end - start).as_secs_f64();
                    output.push_str(&format!(
                        "rush_component_startup_duration_seconds{{component=\"{}\"}} {}\n",
                        name, duration
                    ));
                }
            }

            output.push_str(&format!(
                "rush_component_health_check_attempts{{component=\"{}\"}} {}\n",
                name, metrics.health_check_attempts
            ));

            let status_value = match metrics.status {
                ComponentStatus::Healthy => 1,
                ComponentStatus::Failed => -1,
                _ => 0,
            };

            output.push_str(&format!(
                "rush_component_status{{component=\"{}\"}} {}\n",
                name, status_value
            ));
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collection() {
        let collector = MetricsCollector::new(true);

        collector.record_startup_begin(3, 2).await;
        collector.record_component_start("db", 0, vec![]).await;
        collector.record_container_created("db").await;
        collector.record_health_check_start("db").await;
        collector.record_health_check_attempt("db").await;
        collector.record_component_healthy("db").await;
        collector.record_startup_complete().await;

        let json = collector.export_json().await;
        assert!(json.contains("\"name\": \"db\""));
        assert!(json.contains("\"status\": \"Healthy\""));
    }
}