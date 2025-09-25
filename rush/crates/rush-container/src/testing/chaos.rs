//! Chaos testing framework for reliability validation
//!
//! This module provides chaos engineering capabilities to test
//! system resilience under various failure conditions.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use log::{debug, info, warn};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rush_core::{Error, Result};
use tokio::sync::RwLock;

/// Type of chaos to inject
#[derive(Debug, Clone, PartialEq)]
pub enum ChaosType {
    /// Random failures with a given probability
    RandomFailure { probability: f32 },
    /// Inject latency into operations
    LatencyInjection { min: Duration, max: Duration },
    /// Resource exhaustion simulation
    ResourceExhaustion { resource: ResourceType },
    /// Network partition simulation
    NetworkPartition { duration: Duration },
    /// CPU spike simulation
    CpuSpike { intensity: f32 },
    /// Memory pressure simulation
    MemoryPressure { amount_mb: usize },
    /// Disk I/O throttling
    DiskThrottle { iops_limit: u32 },
}

/// Type of resource to exhaust
#[derive(Debug, Clone, PartialEq)]
pub enum ResourceType {
    /// File descriptors
    FileDescriptors,
    /// Memory
    Memory,
    /// CPU
    Cpu,
    /// Disk space
    DiskSpace,
    /// Network connections
    NetworkConnections,
}

/// Chaos injection policy
#[derive(Debug, Clone)]
pub struct ChaosPolicy {
    /// Types of chaos to inject
    pub chaos_types: Vec<ChaosType>,
    /// Target components (empty means all)
    pub target_components: Vec<String>,
    /// Duration of chaos injection
    pub duration: Duration,
    /// Cool-down period between injections
    pub cooldown: Duration,
    /// Whether chaos is enabled
    pub enabled: bool,
}

impl Default for ChaosPolicy {
    fn default() -> Self {
        Self {
            chaos_types: vec![],
            target_components: vec![],
            duration: Duration::from_secs(30),
            cooldown: Duration::from_secs(60),
            enabled: false,
        }
    }
}

/// Chaos monkey for injecting failures
pub struct ChaosMonkey {
    /// Chaos policy
    policy: Arc<RwLock<ChaosPolicy>>,
    /// Is chaos active
    active: Arc<AtomicBool>,
    /// Failure count
    failures_injected: Arc<AtomicU32>,
    /// Last chaos injection time
    last_injection: Arc<RwLock<Option<Instant>>>,
    /// Random number generator
    rng: Arc<RwLock<StdRng>>,
}

impl ChaosMonkey {
    /// Create a new chaos monkey
    pub fn new() -> Self {
        Self {
            policy: Arc::new(RwLock::new(ChaosPolicy::default())),
            active: Arc::new(AtomicBool::new(false)),
            failures_injected: Arc::new(AtomicU32::new(0)),
            last_injection: Arc::new(RwLock::new(None)),
            rng: Arc::new(RwLock::new(StdRng::from_entropy())),
        }
    }

    /// Create with a specific policy
    pub fn with_policy(policy: ChaosPolicy) -> Self {
        Self {
            policy: Arc::new(RwLock::new(policy)),
            active: Arc::new(AtomicBool::new(false)),
            failures_injected: Arc::new(AtomicU32::new(0)),
            last_injection: Arc::new(RwLock::new(None)),
            rng: Arc::new(RwLock::new(StdRng::from_entropy())),
        }
    }

    /// Enable chaos with a specific failure rate
    pub async fn with_failure_rate(self, rate: f32) -> Self {
        {
            let mut policy = self.policy.write().await;
            policy
                .chaos_types
                .push(ChaosType::RandomFailure { probability: rate });
            policy.enabled = true;
        }
        self
    }

    /// Add latency injection
    pub async fn with_latency_injection(self, min: Duration, max: Duration) -> Self {
        {
            let mut policy = self.policy.write().await;
            policy
                .chaos_types
                .push(ChaosType::LatencyInjection { min, max });
            policy.enabled = true;
        }
        self
    }

    /// Start chaos injection
    pub async fn start(&self) {
        self.active.store(true, Ordering::Relaxed);
        info!("Chaos monkey activated");

        let monkey = self.clone();
        tokio::spawn(async move {
            monkey.chaos_loop().await;
        });
    }

    /// Stop chaos injection
    pub async fn stop(&self) {
        self.active.store(false, Ordering::Relaxed);
        info!("Chaos monkey deactivated");
    }

    /// Main chaos injection loop
    async fn chaos_loop(&self) {
        while self.active.load(Ordering::Relaxed) {
            let policy = self.policy.read().await;

            if !policy.enabled {
                drop(policy);
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }

            // Check cooldown
            let last_injection = self.last_injection.read().await;
            if let Some(last) = *last_injection {
                if last.elapsed() < policy.cooldown {
                    drop(last_injection);
                    drop(policy);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
            drop(last_injection);

            // Inject chaos
            for chaos_type in &policy.chaos_types {
                self.inject_chaos(chaos_type).await;
            }

            // Update last injection time
            let mut last_injection = self.last_injection.write().await;
            *last_injection = Some(Instant::now());

            // Sleep for the chaos duration
            tokio::time::sleep(policy.duration).await;
        }
    }

    /// Inject specific chaos type
    async fn inject_chaos(&self, chaos_type: &ChaosType) {
        match chaos_type {
            ChaosType::RandomFailure { probability } => {
                // This is handled per-operation
                debug!(
                    "Random failure injection enabled with probability {}",
                    probability
                );
            }
            ChaosType::LatencyInjection { min, max } => {
                let duration = self.random_duration(*min, *max).await;
                debug!("Injecting latency: {:?}", duration);
                tokio::time::sleep(duration).await;
            }
            ChaosType::ResourceExhaustion { resource } => {
                self.exhaust_resource(resource).await;
            }
            ChaosType::NetworkPartition { duration } => {
                warn!("Simulating network partition for {:?}", duration);
                // In real implementation, would block network operations
            }
            ChaosType::CpuSpike { intensity } => {
                self.simulate_cpu_spike(*intensity).await;
            }
            ChaosType::MemoryPressure { amount_mb } => {
                self.simulate_memory_pressure(*amount_mb).await;
            }
            ChaosType::DiskThrottle { iops_limit } => {
                debug!("Throttling disk I/O to {} IOPS", iops_limit);
                // In real implementation, would throttle I/O
            }
        }
    }

    /// Should inject failure based on policy
    pub async fn should_inject_failure(&self, component: &str) -> bool {
        let policy = self.policy.read().await;

        if !policy.enabled || !self.active.load(Ordering::Relaxed) {
            return false;
        }

        // Check if component is targeted
        if !policy.target_components.is_empty()
            && !policy.target_components.contains(&component.to_string())
        {
            return false;
        }

        // Check failure probability
        for chaos_type in &policy.chaos_types {
            if let ChaosType::RandomFailure { probability } = chaos_type {
                let mut rng = self.rng.write().await;
                if rng.gen::<f32>() < *probability {
                    self.failures_injected.fetch_add(1, Ordering::Relaxed);
                    warn!(
                        "Chaos monkey injecting failure for component: {}",
                        component
                    );
                    return true;
                }
            }
        }

        false
    }

    /// Inject latency if configured
    pub async fn inject_latency(&self) {
        let policy = self.policy.read().await;

        for chaos_type in &policy.chaos_types {
            if let ChaosType::LatencyInjection { min, max } = chaos_type {
                let duration = self.random_duration(*min, *max).await;
                debug!("Chaos monkey injecting latency: {:?}", duration);
                tokio::time::sleep(duration).await;
            }
        }
    }

    /// Get a random duration between min and max
    async fn random_duration(&self, min: Duration, max: Duration) -> Duration {
        let mut rng = self.rng.write().await;
        let range = max.as_millis() - min.as_millis();
        let random_ms = rng.gen_range(0..=range) + min.as_millis();
        Duration::from_millis(random_ms as u64)
    }

    /// Exhaust a resource
    async fn exhaust_resource(&self, resource: &ResourceType) {
        match resource {
            ResourceType::FileDescriptors => {
                warn!("Simulating file descriptor exhaustion");
                // In real implementation, would open many files
            }
            ResourceType::Memory => {
                warn!("Simulating memory exhaustion");
                // Handled by simulate_memory_pressure
            }
            ResourceType::Cpu => {
                warn!("Simulating CPU exhaustion");
                // Handled by simulate_cpu_spike
            }
            ResourceType::DiskSpace => {
                warn!("Simulating disk space exhaustion");
                // In real implementation, would write large files
            }
            ResourceType::NetworkConnections => {
                warn!("Simulating network connection exhaustion");
                // In real implementation, would open many connections
            }
        }
    }

    /// Simulate CPU spike
    async fn simulate_cpu_spike(&self, intensity: f32) {
        let duration = Duration::from_secs(5);
        let start = Instant::now();

        warn!(
            "Simulating CPU spike at {}% intensity for {:?}",
            intensity * 100.0,
            duration
        );

        while start.elapsed() < duration {
            // Busy loop to consume CPU
            for _ in 0..(1000.0 * intensity) as usize {
                std::hint::black_box(1 + 1);
            }
            tokio::task::yield_now().await;
        }
    }

    /// Simulate memory pressure
    async fn simulate_memory_pressure(&self, amount_mb: usize) {
        warn!("Simulating memory pressure: {} MB", amount_mb);

        // Allocate memory
        let _memory: Vec<u8> = vec![0; amount_mb * 1024 * 1024];

        // Hold for a while
        tokio::time::sleep(Duration::from_secs(10)).await;
    }

    /// Get chaos statistics
    pub async fn get_stats(&self) -> ChaosStats {
        let policy = self.policy.read().await;
        let last_injection = self.last_injection.read().await;

        ChaosStats {
            enabled: policy.enabled,
            active: self.active.load(Ordering::Relaxed),
            failures_injected: self.failures_injected.load(Ordering::Relaxed),
            chaos_types: policy.chaos_types.len(),
            last_injection: last_injection.clone(),
        }
    }
}

impl Clone for ChaosMonkey {
    fn clone(&self) -> Self {
        Self {
            policy: Arc::clone(&self.policy),
            active: Arc::clone(&self.active),
            failures_injected: Arc::clone(&self.failures_injected),
            last_injection: Arc::clone(&self.last_injection),
            rng: Arc::new(RwLock::new(StdRng::from_entropy())),
        }
    }
}

/// Chaos statistics
#[derive(Debug, Clone)]
pub struct ChaosStats {
    /// Whether chaos is enabled
    pub enabled: bool,
    /// Whether chaos is currently active
    pub active: bool,
    /// Number of failures injected
    pub failures_injected: u32,
    /// Number of chaos types configured
    pub chaos_types: usize,
    /// Last injection time
    pub last_injection: Option<Instant>,
}

/// Trait for chaos-aware operations
#[async_trait]
pub trait ChaosAware {
    /// Execute with chaos injection
    async fn execute_with_chaos<T>(
        &self,
        chaos: &ChaosMonkey,
        operation: impl std::future::Future<Output = Result<T>> + Send,
    ) -> Result<T>;
}

/// System under chaos test
pub struct ChaosTestSystem<T> {
    /// The system being tested
    system: T,
    /// Chaos monkey
    chaos: ChaosMonkey,
}

impl<T> ChaosTestSystem<T> {
    /// Create a new chaos test system
    pub fn new(system: T, chaos: ChaosMonkey) -> Self {
        Self { system, chaos }
    }

    /// Run a chaos test scenario
    pub async fn run_scenario<F, R>(&self, scenario: F) -> Result<R>
    where
        F: Fn(&T) -> futures::future::BoxFuture<'_, Result<R>>,
    {
        // Start chaos injection
        self.chaos.start().await;

        // Run the scenario
        let result = scenario(&self.system).await;

        // Stop chaos injection
        self.chaos.stop().await;

        // Report statistics
        let stats = self.chaos.get_stats().await;
        info!(
            "Chaos test completed: {} failures injected",
            stats.failures_injected
        );

        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use super::*;

    #[tokio::test]
    async fn test_chaos_monkey_basic() {
        let chaos = ChaosMonkey::new().with_failure_rate(0.5).await;

        // Activate the chaos monkey
        chaos.active.store(true, Ordering::Relaxed);

        let mut failures = 0;
        let mut successes = 0;

        for _ in 0..100 {
            if chaos.should_inject_failure("test").await {
                failures += 1;
            } else {
                successes += 1;
            }
        }

        println!("Failures: {}, Successes: {}", failures, successes);

        // With 0.5 probability, we expect roughly 50/50
        assert!(failures > 20 && failures < 80);
        assert!(successes > 20 && successes < 80);
    }

    #[tokio::test]
    async fn test_chaos_latency() {
        let chaos = ChaosMonkey::new()
            .with_latency_injection(Duration::from_millis(10), Duration::from_millis(50))
            .await;

        let start = Instant::now();
        chaos.inject_latency().await;
        let elapsed = start.elapsed();

        // Should have injected some latency
        assert!(elapsed >= Duration::from_millis(10));
        assert!(elapsed <= Duration::from_millis(100));
    }
}
