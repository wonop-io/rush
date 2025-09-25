//! Reliability utilities for Rush
//!
//! This module provides utilities for improving the reliability of Rush,
//! including retry logic, circuit breakers, and timeout wrappers.

use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, warn};
use tokio::sync::RwLock;
use tokio::time::{sleep, timeout};

use crate::{Error, Result};

/// Retry configuration for operations
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Backoff multiplier (e.g., 2.0 for exponential backoff)
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

/// Execute an operation with retry logic and exponential backoff
pub async fn with_retry<F, Fut, T>(operation: F, config: RetryConfig) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut retries = 0;
    let mut backoff = config.initial_backoff;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if retries < config.max_retries && is_retryable(&e) => {
                warn!(
                    "Operation failed (attempt {}/{}): {}",
                    retries + 1,
                    config.max_retries,
                    e
                );
                sleep(backoff).await;

                // Calculate next backoff with jitter
                let next_backoff =
                    Duration::from_secs_f64(backoff.as_secs_f64() * config.backoff_multiplier);
                backoff = next_backoff.min(config.max_backoff);
                retries += 1;
            }
            Err(e) => {
                error!("Operation failed after {retries} retries: {e}");
                return Err(e);
            }
        }
    }
}

/// Check if an error is retryable
fn is_retryable(error: &Error) -> bool {
    match error {
        Error::Docker(msg) => {
            // Retry on transient Docker errors
            msg.contains("timeout")
                || msg.contains("connection refused")
                || msg.contains("temporarily unavailable")
        }
        Error::Network(_) => true,
        Error::External(msg) => {
            // Retry on transient external errors
            msg.contains("timeout") || msg.contains("connection")
        }
        _ => false,
    }
}

/// Circuit breaker for protecting against cascading failures
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    failures: Arc<AtomicU32>,
    last_failure: Arc<RwLock<Option<Instant>>>,
    threshold: u32,
    reset_timeout: Duration,
    half_open_success_count: Arc<AtomicU32>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    pub fn new(threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failures: Arc::new(AtomicU32::new(0)),
            last_failure: Arc::new(RwLock::new(None)),
            threshold,
            reset_timeout,
            half_open_success_count: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Check if the circuit breaker is open
    pub async fn is_open(&self) -> bool {
        let failure_count = self.failures.load(Ordering::SeqCst);

        if failure_count < self.threshold {
            return false;
        }

        // Check if we should transition to half-open
        if let Some(last_failure) = *self.last_failure.read().await {
            if last_failure.elapsed() > self.reset_timeout {
                debug!("Circuit breaker transitioning to half-open");
                return false; // Allow one request through
            }
        }

        true
    }

    /// Execute a function with circuit breaker protection
    pub async fn call<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        if self.is_open().await {
            return Err(Error::ServiceUnavailable(
                "Circuit breaker is open".to_string(),
            ));
        }

        match f().await {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(e) => {
                self.on_failure().await;
                Err(e)
            }
        }
    }

    /// Record a successful operation
    async fn on_success(&self) {
        let failure_count = self.failures.load(Ordering::SeqCst);

        if failure_count >= self.threshold {
            // We're in half-open state
            let success_count = self.half_open_success_count.fetch_add(1, Ordering::SeqCst) + 1;

            if success_count >= 3 {
                // Reset after 3 successful operations
                debug!("Circuit breaker closing after successful operations");
                self.failures.store(0, Ordering::SeqCst);
                self.half_open_success_count.store(0, Ordering::SeqCst);
                *self.last_failure.write().await = None;
            }
        } else {
            // Reset failure count on success
            self.failures.store(0, Ordering::SeqCst);
        }
    }

    /// Record a failed operation
    async fn on_failure(&self) {
        let failures = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure.write().await = Some(Instant::now());
        self.half_open_success_count.store(0, Ordering::SeqCst);

        if failures == self.threshold {
            error!("Circuit breaker opened after {failures} failures");
        }
    }
}

/// Execute an operation with a timeout
pub async fn with_timeout<F, T>(operation: F, duration: Duration, operation_name: &str) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    match timeout(duration, operation).await {
        Ok(result) => result,
        Err(_) => Err(Error::Timeout(format!(
            "{operation_name} exceeded timeout of {duration:?}"
        ))),
    }
}

/// Health check trait for monitoring component health
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    /// Check the health of the component
    async fn check(&self) -> HealthStatus;

    /// Get the name of the health check
    fn name(&self) -> &str;
}

/// Health status of a component
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    /// Component is healthy
    Healthy,
    /// Component is degraded but operational
    Degraded(String),
    /// Component is unhealthy
    Unhealthy(String),
}

impl HealthStatus {
    /// Check if the status is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    /// Check if the status is operational (healthy or degraded)
    pub fn is_operational(&self) -> bool {
        !matches!(self, HealthStatus::Unhealthy(_))
    }
}

/// Health monitor for periodic health checks
pub struct HealthMonitor {
    checks: Vec<Box<dyn HealthCheck>>,
    interval: Duration,
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(interval: Duration) -> Self {
        Self {
            checks: Vec::new(),
            interval,
        }
    }

    /// Add a health check to the monitor
    pub fn add_check(mut self, check: Box<dyn HealthCheck>) -> Self {
        self.checks.push(check);
        self
    }

    /// Start the health monitor
    pub async fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.interval);

            loop {
                interval.tick().await;

                for check in &self.checks {
                    let status = check.check().await;

                    match status {
                        HealthStatus::Healthy => {
                            debug!("{} health check: healthy", check.name());
                        }
                        HealthStatus::Degraded(msg) => {
                            warn!("{} health check: degraded - {}", check.name(), msg);
                        }
                        HealthStatus::Unhealthy(msg) => {
                            error!("{} health check: unhealthy - {}", check.name(), msg);
                            // TODO: Trigger recovery actions
                        }
                    }
                }
            }
        })
    }
}

/// Fallback strategy for graceful degradation
pub struct FallbackStrategy<T> {
    circuit_breaker: CircuitBreaker,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Default for FallbackStrategy<T>
where
    T: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> FallbackStrategy<T>
where
    T: Send + 'static,
{
    /// Create a new fallback strategy
    pub fn new() -> Self {
        Self {
            circuit_breaker: CircuitBreaker::new(3, Duration::from_secs(60)),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Execute with fallback strategy
    pub async fn execute<F1, F2, Fut1, Fut2>(&self, primary: F1, fallback: F2) -> Result<T>
    where
        F1: FnOnce() -> Fut1,
        F2: FnOnce() -> Fut2,
        Fut1: Future<Output = Result<T>>,
        Fut2: Future<Output = Result<T>>,
    {
        // Try primary with circuit breaker
        match self.circuit_breaker.call(primary).await {
            Ok(response) => Ok(response),
            Err(e) => {
                warn!("Primary service failed, using fallback: {e}");
                fallback().await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_retry_logic() {
        use std::sync::Mutex;

        let attempt = Arc::new(Mutex::new(0));
        let attempt_clone = Arc::clone(&attempt);

        let operation = move || {
            let attempt = Arc::clone(&attempt_clone);
            async move {
                let mut count = attempt.lock().unwrap();
                *count += 1;
                if *count < 3 {
                    Err(Error::Network("Connection refused".to_string()))
                } else {
                    Ok("Success")
                }
            }
        };

        let config = RetryConfig {
            max_retries: 3,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let result = with_retry(operation, config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(100));

        // First failure
        let _ = breaker
            .call(|| async { Err::<(), _>(Error::Docker("Connection failed".to_string())) })
            .await;

        // Second failure - should open the circuit
        let _ = breaker
            .call(|| async { Err::<(), _>(Error::Docker("Connection failed".to_string())) })
            .await;

        // Circuit should be open now
        assert!(breaker.is_open().await);

        // Wait for reset timeout
        sleep(Duration::from_millis(150)).await;

        // Circuit should allow one request through (half-open)
        assert!(!breaker.is_open().await);
    }

    #[tokio::test]
    async fn test_timeout_wrapper() {
        let operation = async {
            sleep(Duration::from_secs(2)).await;
            Ok::<_, Error>("Should timeout")
        };

        let result = with_timeout(operation, Duration::from_millis(100), "test operation").await;

        assert!(result.is_err());
        match result {
            Err(Error::Timeout(msg)) => assert!(msg.contains("test operation")),
            _ => panic!("Expected timeout error"),
        }
    }
}
