//! Recovery and graceful degradation mechanisms
//!
//! This module provides recovery strategies and graceful degradation
//! when components fail during startup or runtime.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use log::{error, info, warn};
use rush_core::error::{Error, Result};
use tokio::sync::RwLock;

/// Recovery strategy for failed components
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecoveryStrategy {
    /// Fail the entire startup
    FailFast,
    /// Continue with degraded functionality
    Graceful,
    /// Retry with fallback configuration
    Fallback,
    /// Skip non-critical components
    SkipNonCritical,
}

/// Component criticality level
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Criticality {
    /// Component must be running for system to function
    Critical,
    /// Component is important but system can function without it
    Important,
    /// Component is optional
    Optional,
}

/// Recovery configuration
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Default recovery strategy
    pub default_strategy: RecoveryStrategy,
    /// Per-component recovery strategies
    pub component_strategies: HashMap<String, RecoveryStrategy>,
    /// Component criticality levels
    pub component_criticality: HashMap<String, Criticality>,
    /// Allow degraded mode
    pub allow_degraded: bool,
    /// Minimum healthy components percentage for degraded mode
    pub degraded_threshold: f64,
    /// Network failure retry attempts
    pub network_retry_attempts: u32,
    /// Network failure retry delay
    pub network_retry_delay: Duration,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            default_strategy: RecoveryStrategy::FailFast,
            component_strategies: HashMap::new(),
            component_criticality: HashMap::new(),
            allow_degraded: true,
            degraded_threshold: 0.7,
            network_retry_attempts: 5,
            network_retry_delay: Duration::from_secs(2),
        }
    }
}

/// Recovery manager for handling component failures
pub struct RecoveryManager {
    config: RecoveryConfig,
    /// Track failed components
    failed_components: Arc<RwLock<HashSet<String>>>,
    /// Track degraded components
    degraded_components: Arc<RwLock<HashSet<String>>>,
    /// Track recovery attempts
    recovery_attempts: Arc<RwLock<HashMap<String, u32>>>,
}

impl RecoveryManager {
    /// Create a new recovery manager
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            config,
            failed_components: Arc::new(RwLock::new(HashSet::new())),
            degraded_components: Arc::new(RwLock::new(HashSet::new())),
            recovery_attempts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Handle component failure
    pub async fn handle_failure(
        &self,
        component_name: &str,
        error: &Error,
        total_components: usize,
    ) -> Result<RecoveryAction> {
        // Track the failure
        {
            let mut failed = self.failed_components.write().await;
            failed.insert(component_name.to_string());
        }

        // Get component criticality
        let criticality = self
            .config
            .component_criticality
            .get(component_name)
            .copied()
            .unwrap_or(Criticality::Important);

        // Get recovery strategy
        let strategy = self
            .config
            .component_strategies
            .get(component_name)
            .copied()
            .unwrap_or(self.config.default_strategy);

        // Check if it's a network-related error
        let is_network_error = self.is_network_error(error);

        // Determine recovery action based on strategy and criticality
        let action = match (strategy, criticality) {
            (RecoveryStrategy::FailFast, Criticality::Critical) => {
                error!("Critical component {component_name} failed, stopping all");
                RecoveryAction::StopAll
            }
            (RecoveryStrategy::Graceful, Criticality::Optional) => {
                warn!("Optional component {component_name} failed, continuing without it");
                RecoveryAction::Skip
            }
            (RecoveryStrategy::SkipNonCritical, Criticality::Optional) => {
                warn!("Skipping non-critical component {component_name}");
                RecoveryAction::Skip
            }
            (RecoveryStrategy::Fallback, _) if is_network_error => {
                // Retry network failures with special handling
                self.handle_network_failure(component_name).await
            }
            (RecoveryStrategy::Graceful, _) => {
                // Check if we can continue in degraded mode
                if self.can_run_degraded(total_components).await {
                    warn!("Running in degraded mode without {component_name}");
                    RecoveryAction::ContinueDegraded
                } else {
                    error!("Too many failures, cannot continue in degraded mode");
                    RecoveryAction::StopAll
                }
            }
            _ => {
                // Default: retry once then fail
                let attempts = self.get_recovery_attempts(component_name).await;
                if attempts < 1 {
                    info!(
                        "Retrying component {} (attempt {})",
                        component_name,
                        attempts + 1
                    );
                    self.increment_recovery_attempts(component_name).await;
                    RecoveryAction::Retry(Duration::from_secs(5))
                } else {
                    error!("Component {component_name} failed after retry");
                    RecoveryAction::StopAll
                }
            }
        };

        Ok(action)
    }

    /// Handle network-specific failures
    async fn handle_network_failure(&self, component_name: &str) -> RecoveryAction {
        let attempts = self.get_recovery_attempts(component_name).await;

        if attempts < self.config.network_retry_attempts {
            warn!(
                "Network failure for {}, retrying ({}/{})",
                component_name,
                attempts + 1,
                self.config.network_retry_attempts
            );
            self.increment_recovery_attempts(component_name).await;

            // Exponential backoff for network retries
            let delay = self.config.network_retry_delay * 2u32.pow(attempts);
            RecoveryAction::Retry(delay)
        } else {
            error!(
                "Network failure for {} after {} attempts",
                component_name, self.config.network_retry_attempts
            );

            // Check criticality
            let criticality = self
                .config
                .component_criticality
                .get(component_name)
                .copied()
                .unwrap_or(Criticality::Important);

            if criticality == Criticality::Optional {
                RecoveryAction::Skip
            } else {
                RecoveryAction::StopAll
            }
        }
    }

    /// Check if error is network-related
    fn is_network_error(&self, error: &Error) -> bool {
        let error_str = error.to_string().to_lowercase();
        error_str.contains("network")
            || error_str.contains("connection")
            || error_str.contains("dns")
            || error_str.contains("resolve")
            || error_str.contains("timeout")
    }

    /// Check if system can run in degraded mode
    async fn can_run_degraded(&self, total_components: usize) -> bool {
        if !self.config.allow_degraded {
            return false;
        }

        let failed_count = self.failed_components.read().await.len();
        let healthy_ratio = (total_components - failed_count) as f64 / total_components as f64;

        // Check if we meet the degraded threshold
        if healthy_ratio >= self.config.degraded_threshold {
            // Also check that no critical components have failed
            let failed = self.failed_components.read().await;
            for component in failed.iter() {
                if let Some(Criticality::Critical) =
                    self.config.component_criticality.get(component)
                {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }

    /// Get recovery attempts for a component
    async fn get_recovery_attempts(&self, component_name: &str) -> u32 {
        self.recovery_attempts
            .read()
            .await
            .get(component_name)
            .copied()
            .unwrap_or(0)
    }

    /// Increment recovery attempts
    async fn increment_recovery_attempts(&self, component_name: &str) {
        let mut attempts = self.recovery_attempts.write().await;
        *attempts.entry(component_name.to_string()).or_insert(0) += 1;
    }

    /// Mark component as degraded
    pub async fn mark_degraded(&self, component_name: &str) {
        let mut degraded = self.degraded_components.write().await;
        degraded.insert(component_name.to_string());
        warn!("Component {component_name} marked as degraded");
    }

    /// Get system status
    pub async fn get_status(&self) -> SystemStatus {
        let failed = self.failed_components.read().await;
        let degraded = self.degraded_components.read().await;

        if failed.is_empty() && degraded.is_empty() {
            SystemStatus::Healthy
        } else if failed.is_empty() {
            SystemStatus::Degraded {
                degraded_components: degraded.clone(),
            }
        } else {
            SystemStatus::Failed {
                failed_components: failed.clone(),
                degraded_components: degraded.clone(),
            }
        }
    }

    /// Reset recovery state
    pub async fn reset(&self) {
        self.failed_components.write().await.clear();
        self.degraded_components.write().await.clear();
        self.recovery_attempts.write().await.clear();
        info!("Recovery state reset");
    }
}

/// Recovery action to take
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryAction {
    /// Retry the component after delay
    Retry(Duration),
    /// Skip the component and continue
    Skip,
    /// Continue in degraded mode
    ContinueDegraded,
    /// Stop all components
    StopAll,
    /// Use fallback configuration
    UseFallback(HashMap<String, String>),
}

/// System status
#[derive(Debug, Clone)]
pub enum SystemStatus {
    /// All components healthy
    Healthy,
    /// Running with degraded functionality
    Degraded {
        degraded_components: HashSet<String>,
    },
    /// System failed
    Failed {
        failed_components: HashSet<String>,
        degraded_components: HashSet<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_criticality_handling() {
        let mut config = RecoveryConfig::default();
        config
            .component_criticality
            .insert("database".to_string(), Criticality::Critical);
        config
            .component_criticality
            .insert("cache".to_string(), Criticality::Important);
        config
            .component_criticality
            .insert("monitoring".to_string(), Criticality::Optional);

        let manager = RecoveryManager::new(config);

        // Critical component failure should stop all
        let action = manager
            .handle_failure(
                "database",
                &Error::Container("Connection failed".to_string()),
                5,
            )
            .await
            .unwrap();
        assert_eq!(action, RecoveryAction::StopAll);

        // Optional component can be skipped
        manager.reset().await;
        let mut config = RecoveryConfig::default();
        config.default_strategy = RecoveryStrategy::SkipNonCritical;
        config
            .component_criticality
            .insert("monitoring".to_string(), Criticality::Optional);

        let manager = RecoveryManager::new(config);
        let action = manager
            .handle_failure(
                "monitoring",
                &Error::Container("Start failed".to_string()),
                5,
            )
            .await
            .unwrap();
        assert_eq!(action, RecoveryAction::Skip);
    }

    #[tokio::test]
    async fn test_degraded_mode() {
        let mut config = RecoveryConfig::default();
        config.default_strategy = RecoveryStrategy::Graceful;
        config.allow_degraded = true;
        config.degraded_threshold = 0.6; // Allow if 60% healthy

        let manager = RecoveryManager::new(config);

        // With 1 failure out of 5, should allow degraded (80% healthy)
        let action = manager
            .handle_failure("component1", &Error::Container("Failed".to_string()), 5)
            .await
            .unwrap();
        assert_eq!(action, RecoveryAction::ContinueDegraded);

        // With 3 failures out of 5, should not allow (40% healthy)
        manager
            .handle_failure("component2", &Error::Container("Failed".to_string()), 5)
            .await
            .unwrap();
        let action = manager
            .handle_failure("component3", &Error::Container("Failed".to_string()), 5)
            .await
            .unwrap();
        assert_eq!(action, RecoveryAction::StopAll);
    }

    #[tokio::test]
    async fn test_network_retry() {
        let mut config = RecoveryConfig::default();
        config.default_strategy = RecoveryStrategy::Fallback;
        config.network_retry_attempts = 3;
        config.network_retry_delay = Duration::from_millis(100);

        let manager = RecoveryManager::new(config);

        // First network failure should retry
        let action = manager
            .handle_failure(
                "api",
                &Error::Container("Network unreachable".to_string()),
                5,
            )
            .await
            .unwrap();
        assert!(matches!(action, RecoveryAction::Retry(_)));

        // After max attempts, should stop
        for _ in 0..2 {
            manager
                .handle_failure(
                    "api",
                    &Error::Container("Network unreachable".to_string()),
                    5,
                )
                .await
                .unwrap();
        }

        let action = manager
            .handle_failure(
                "api",
                &Error::Container("Network unreachable".to_string()),
                5,
            )
            .await
            .unwrap();
        assert_eq!(action, RecoveryAction::StopAll);
    }
}
