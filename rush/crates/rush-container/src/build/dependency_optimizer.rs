//! Enhanced dependency optimization for build performance
//!
//! This module provides advanced dependency graph analysis and optimization
//! to maximize parallel build throughput and minimize total build time.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use log::info;
use rush_core::Result;
use tokio::sync::RwLock;

use crate::dependency_graph::DependencyGraph;

/// Build statistics for performance tracking
#[derive(Debug, Clone)]
pub struct BuildStatistics {
    /// Average build time per component type
    pub avg_build_times: HashMap<String, Duration>,
    /// Historical build times for components
    pub build_history: HashMap<String, Vec<Duration>>,
    /// Cache hit rates by component
    pub cache_hit_rates: HashMap<String, f64>,
    /// Dependency chain depths
    pub dependency_depths: HashMap<String, usize>,
    /// Critical path components
    pub critical_path: Vec<String>,
}

impl Default for BuildStatistics {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildStatistics {
    pub fn new() -> Self {
        Self {
            avg_build_times: HashMap::new(),
            build_history: HashMap::new(),
            cache_hit_rates: HashMap::new(),
            dependency_depths: HashMap::new(),
            critical_path: Vec::new(),
        }
    }

    /// Update build time for a component
    pub fn record_build_time(&mut self, component: String, duration: Duration) {
        self.build_history
            .entry(component.clone())
            .or_default()
            .push(duration);

        // Update average
        let history = &self.build_history[&component];
        let avg = history.iter().sum::<Duration>() / history.len() as u32;
        self.avg_build_times.insert(component, avg);
    }

    /// Record cache hit/miss
    pub fn record_cache_access(&mut self, component: String, hit: bool) {
        let entry = self.cache_hit_rates.entry(component).or_insert(0.0);
        // Use exponential moving average
        *entry = (*entry * 0.9) + (if hit { 0.1 } else { 0.0 });
    }

    /// Get estimated build time for a component
    pub fn estimate_build_time(&self, component: &str) -> Duration {
        self.avg_build_times
            .get(component)
            .copied()
            .unwrap_or(Duration::from_secs(30)) // Default estimate
    }
}

/// Enhanced dependency optimizer
pub struct DependencyOptimizer {
    graph: Arc<RwLock<DependencyGraph>>,
    stats: Arc<RwLock<BuildStatistics>>,
    max_parallel: usize,
    resource_limits: ResourceLimits,
}

/// Resource limits for build optimization
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum CPU cores to use
    pub max_cpu_cores: usize,
    /// Maximum memory in MB
    pub max_memory_mb: usize,
    /// Maximum concurrent Docker builds
    pub max_docker_builds: usize,
    /// Network bandwidth limit in Mbps
    pub network_bandwidth_mbps: Option<usize>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_cores: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(8),
            max_memory_mb: 8192,
            max_docker_builds: 4,
            network_bandwidth_mbps: None,
        }
    }
}

impl DependencyOptimizer {
    /// Create a new dependency optimizer
    pub fn new(graph: DependencyGraph, max_parallel: usize) -> Self {
        Self {
            graph: Arc::new(RwLock::new(graph)),
            stats: Arc::new(RwLock::new(BuildStatistics::new())),
            max_parallel,
            resource_limits: ResourceLimits::default(),
        }
    }

    /// Set resource limits
    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }

    /// Analyze dependency graph and compute critical path
    pub async fn analyze_critical_path(&self) -> Result<Vec<String>> {
        let graph = self.graph.read().await;
        let stats = self.stats.read().await;

        // Use dynamic programming to find longest path
        let mut path_costs: HashMap<String, Duration> = HashMap::new();
        let mut path_prev: HashMap<String, Option<String>> = HashMap::new();

        // Get topological order
        let sorted = graph.topological_sort()?;

        // Process nodes in reverse topological order
        for name in sorted.iter().rev() {
            let node_cost = stats.estimate_build_time(name);
            let dependents = graph.get_dependents(name);

            let max_dependent_cost = dependents
                .iter()
                .filter_map(|dep| path_costs.get(dep))
                .max()
                .copied()
                .unwrap_or(Duration::ZERO);

            let total_cost = node_cost + max_dependent_cost;
            path_costs.insert(name.clone(), total_cost);

            // Track predecessor for path reconstruction
            let max_dependent = dependents
                .iter()
                .max_by_key(|dep| path_costs.get(*dep).unwrap_or(&Duration::ZERO));

            path_prev.insert(name.clone(), max_dependent.cloned());
        }

        // Find node with maximum cost (critical path start)
        let start = path_costs
            .iter()
            .max_by_key(|(_, cost)| *cost)
            .map(|(name, _)| name.clone())
            .unwrap_or_default();

        // Reconstruct critical path
        let mut critical_path = Vec::new();
        let mut current = Some(start);

        while let Some(node) = current {
            critical_path.push(node.clone());
            current = path_prev.get(&node).and_then(|n| n.clone());
        }

        info!(
            "Critical path: {:?} (estimated time: {:?})",
            critical_path,
            path_costs.get(&critical_path[0]).unwrap_or(&Duration::ZERO)
        );

        Ok(critical_path)
    }

    /// Get optimized build groups based on dependencies and resource constraints
    pub async fn get_optimized_build_groups(&self) -> Result<Vec<BuildGroup>> {
        let graph = self.graph.read().await;
        let stats = self.stats.read().await;

        let mut groups = Vec::new();
        let mut completed = HashSet::new();
        let mut in_progress = HashSet::new();

        // Analyze critical path
        let critical_path = self.analyze_critical_path().await?;
        let critical_set: HashSet<_> = critical_path.into_iter().collect();

        loop {
            // Get all components ready to build
            let ready: Vec<String> = graph
                .nodes()
                .iter()
                .filter_map(|(name, _node)| {
                    if completed.contains(name) || in_progress.contains(name) {
                        return None;
                    }

                    // Check if dependencies are completed
                    let deps = graph.get_dependencies(name);
                    let deps_ready = deps.iter().all(|dep| completed.contains(dep));

                    if deps_ready {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .collect();

            if ready.is_empty() && in_progress.is_empty() {
                break;
            }

            if !ready.is_empty() {
                // Prioritize critical path components
                let mut prioritized = ready.clone();
                prioritized.sort_by_key(|name| {
                    let is_critical = critical_set.contains(name);
                    let estimated_time = stats.estimate_build_time(name);

                    // Critical path components get highest priority (negative score)
                    // Longer builds get higher priority among non-critical
                    if is_critical {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(3600) - estimated_time
                    }
                });

                // Create build group respecting resource limits
                let group = self
                    .create_resource_aware_group(prioritized, &stats)
                    .await?;

                // Mark components as in progress
                for component in &group.components {
                    in_progress.insert(component.clone());
                }

                groups.push(group);
            }

            // Simulate completion of fastest component to continue
            if let Some(fastest) = in_progress
                .iter()
                .min_by_key(|name| stats.estimate_build_time(name))
                .cloned()
            {
                in_progress.remove(&fastest);
                completed.insert(fastest);
            }
        }

        info!(
            "Optimized build plan: {} groups for {} components",
            groups.len(),
            graph.nodes().len()
        );

        Ok(groups)
    }

    /// Create a resource-aware build group
    async fn create_resource_aware_group(
        &self,
        candidates: Vec<String>,
        stats: &BuildStatistics,
    ) -> Result<BuildGroup> {
        let mut group = BuildGroup {
            components: Vec::new(),
            estimated_duration: Duration::ZERO,
            resource_usage: ResourceUsage::default(),
            priority: 0,
        };

        let mut total_cpu = 0;
        let mut total_memory = 0;
        let mut docker_builds = 0;

        for component in candidates {
            // Estimate resource usage
            let resource_usage = self.estimate_resource_usage(&component, stats);

            // Check if adding this component would exceed limits
            if docker_builds >= self.resource_limits.max_docker_builds {
                continue;
            }

            if total_cpu + resource_usage.cpu_cores > self.resource_limits.max_cpu_cores {
                continue;
            }

            if total_memory + resource_usage.memory_mb > self.resource_limits.max_memory_mb {
                continue;
            }

            // Add to group
            group.components.push(component);
            total_cpu += resource_usage.cpu_cores;
            total_memory += resource_usage.memory_mb;
            docker_builds += 1;

            // Update group metrics
            group.resource_usage.cpu_cores = total_cpu;
            group.resource_usage.memory_mb = total_memory;

            // Stop if we've reached parallel limit
            if group.components.len() >= self.max_parallel {
                break;
            }
        }

        // Calculate estimated duration (max of all components)
        group.estimated_duration = group
            .components
            .iter()
            .map(|c| stats.estimate_build_time(c))
            .max()
            .unwrap_or(Duration::ZERO);

        Ok(group)
    }

    /// Estimate resource usage for a component
    fn estimate_resource_usage(&self, component: &str, stats: &BuildStatistics) -> ResourceUsage {
        // Base estimates (can be refined based on component type)
        let base_cpu = 2;
        let base_memory = 1024;

        // Adjust based on historical data if available
        let cache_hit_rate = stats.cache_hit_rates.get(component).copied().unwrap_or(0.0);

        // Cached builds use fewer resources
        let cpu_cores = if cache_hit_rate > 0.8 { 1 } else { base_cpu };

        let memory_mb = if cache_hit_rate > 0.8 {
            base_memory / 2
        } else {
            base_memory
        };

        ResourceUsage {
            cpu_cores,
            memory_mb,
            network_mbps: 10, // Estimate
        }
    }

    /// Update statistics after a build
    pub async fn update_build_stats(&self, component: String, duration: Duration, cache_hit: bool) {
        let mut stats = self.stats.write().await;
        stats.record_build_time(component.clone(), duration);
        stats.record_cache_access(component, cache_hit);
    }

    /// Generate optimization report
    pub async fn generate_optimization_report(&self) -> OptimizationReport {
        let graph = self.graph.read().await;
        let stats = self.stats.read().await;

        // Calculate metrics
        let total_components = graph.nodes().len();
        let waves = graph.get_startup_waves().unwrap_or_default();
        let parallelization_factor = if !waves.is_empty() {
            waves.iter().map(|w| w.len()).max().unwrap_or(1) as f64 / total_components as f64
        } else {
            0.0
        };

        // Find bottlenecks
        let bottlenecks: Vec<String> = graph
            .nodes()
            .iter()
            .filter_map(|(name, _)| {
                let dependents = graph.get_dependents(name);
                if dependents.len() > 3 {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();

        // Calculate estimated vs actual times
        let estimated_total: Duration = stats.avg_build_times.values().sum();

        let estimated_parallel = waves
            .iter()
            .map(|wave| {
                wave.iter()
                    .map(|c| stats.estimate_build_time(c))
                    .max()
                    .unwrap_or(Duration::ZERO)
            })
            .sum();

        OptimizationReport {
            total_components,
            parallelization_factor,
            critical_path: stats.critical_path.clone(),
            bottleneck_components: bottlenecks,
            estimated_sequential_time: estimated_total,
            estimated_parallel_time: estimated_parallel,
            potential_speedup: if estimated_parallel > Duration::ZERO {
                estimated_total.as_secs_f64() / estimated_parallel.as_secs_f64()
            } else {
                1.0
            },
            recommendations: self.generate_recommendations(&graph, &stats),
        }
    }

    /// Generate optimization recommendations
    fn generate_recommendations(
        &self,
        graph: &DependencyGraph,
        stats: &BuildStatistics,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Check for unnecessary dependencies
        for name in graph.nodes().keys() {
            let deps = graph.get_dependencies(name);
            if deps.len() > 5 {
                recommendations.push(format!(
                    "Component '{}' has {} dependencies. Consider refactoring to reduce coupling.",
                    name,
                    deps.len()
                ));
            }
        }

        // Check for low cache hit rates
        for (component, rate) in &stats.cache_hit_rates {
            if *rate < 0.5 {
                recommendations.push(format!(
                    "Component '{}' has low cache hit rate ({:.1}%). Consider improving cache key computation.",
                    component, rate * 100.0
                ));
            }
        }

        // Check for long-running builds
        for (component, duration) in &stats.avg_build_times {
            if *duration > Duration::from_secs(60) {
                recommendations.push(format!(
                    "Component '{}' takes {:.1}s to build. Consider optimizing or splitting.",
                    component,
                    duration.as_secs_f64()
                ));
            }
        }

        recommendations
    }
}

/// A group of components that can be built in parallel
#[derive(Debug, Clone)]
pub struct BuildGroup {
    /// Components in this group
    pub components: Vec<String>,
    /// Estimated duration for the group
    pub estimated_duration: Duration,
    /// Resource usage for the group
    pub resource_usage: ResourceUsage,
    /// Priority (lower is higher priority)
    pub priority: i32,
}

/// Resource usage estimation
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    /// CPU cores required
    pub cpu_cores: usize,
    /// Memory in MB
    pub memory_mb: usize,
    /// Network bandwidth in Mbps
    pub network_mbps: usize,
}

/// Optimization report
#[derive(Debug, Clone)]
pub struct OptimizationReport {
    /// Total number of components
    pub total_components: usize,
    /// Parallelization factor (0-1, higher is better)
    pub parallelization_factor: f64,
    /// Critical path components
    pub critical_path: Vec<String>,
    /// Bottleneck components (many dependents)
    pub bottleneck_components: Vec<String>,
    /// Estimated sequential build time
    pub estimated_sequential_time: Duration,
    /// Estimated parallel build time
    pub estimated_parallel_time: Duration,
    /// Potential speedup factor
    pub potential_speedup: f64,
    /// Optimization recommendations
    pub recommendations: Vec<String>,
}

impl OptimizationReport {
    /// Print the report
    pub fn print(&self) {
        println!("\n=== Build Optimization Report ===");
        println!("Total Components: {}", self.total_components);
        println!(
            "Parallelization Factor: {:.2}%",
            self.parallelization_factor * 100.0
        );
        println!("Potential Speedup: {:.2}x", self.potential_speedup);
        println!("\nTime Estimates:");
        println!("  Sequential: {:?}", self.estimated_sequential_time);
        println!("  Parallel: {:?}", self.estimated_parallel_time);

        if !self.critical_path.is_empty() {
            println!("\nCritical Path:");
            for component in &self.critical_path {
                println!("  - {component}");
            }
        }

        if !self.bottleneck_components.is_empty() {
            println!("\nBottleneck Components:");
            for component in &self.bottleneck_components {
                println!("  - {component}");
            }
        }

        if !self.recommendations.is_empty() {
            println!("\nRecommendations:");
            for (i, rec) in self.recommendations.iter().enumerate() {
                println!("  {}. {}", i + 1, rec);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rush_build::ComponentBuildSpec;

    #[tokio::test]
    async fn test_dependency_optimizer() {
        // Create test specs
        let specs = vec![
            create_test_spec("frontend", vec!["api"]),
            create_test_spec("api", vec!["database", "cache"]),
            create_test_spec("database", vec![]),
            create_test_spec("cache", vec![]),
            create_test_spec("worker", vec!["api"]),
        ];

        let graph = DependencyGraph::from_specs(specs).unwrap();
        let optimizer = DependencyOptimizer::new(graph, 3);

        // Test critical path analysis
        let critical_path = optimizer.analyze_critical_path().await.unwrap();
        assert!(!critical_path.is_empty());

        // Test optimized build groups
        let groups = optimizer.get_optimized_build_groups().await.unwrap();
        assert!(!groups.is_empty());

        // Verify resource limits are respected
        for group in &groups {
            assert!(group.components.len() <= 3);
            assert!(group.resource_usage.cpu_cores <= optimizer.resource_limits.max_cpu_cores);
        }
    }

    #[tokio::test]
    async fn test_build_statistics() {
        let mut stats = BuildStatistics::new();

        // Record build times
        stats.record_build_time("api".to_string(), Duration::from_secs(30));
        stats.record_build_time("api".to_string(), Duration::from_secs(25));
        stats.record_build_time("api".to_string(), Duration::from_secs(35));

        // Check average
        let avg = stats.estimate_build_time("api");
        assert_eq!(avg, Duration::from_secs(30));

        // Record cache accesses
        stats.record_cache_access("api".to_string(), true);
        stats.record_cache_access("api".to_string(), true);
        stats.record_cache_access("api".to_string(), false);

        let hit_rate = stats.cache_hit_rates.get("api").unwrap();
        assert!(*hit_rate > 0.0 && *hit_rate < 1.0);
    }

    fn create_test_spec(name: &str, deps: Vec<&str>) -> ComponentBuildSpec {
        ComponentBuildSpec {
            component_name: name.to_string(),
            depends_on: deps.into_iter().map(String::from).collect(),
            // ... other fields with defaults
            build_type: rush_build::BuildType::RustBinary {
                location: "src".to_string(),
                dockerfile_path: "Dockerfile".to_string(),
                context_dir: None,
                features: None,
                precompile_commands: None,
            },
            product_name: "test".to_string(),
            color: "blue".to_string(),
            build: None,
            mount_point: None,
            subdomain: None,
            artefacts: None,
            artefact_output_dir: "dist".to_string(),
            docker_extra_run_args: vec![],
            env: None,
            volumes: None,
            port: None,
            target_port: None,
            k8s: None,
            priority: 0,
            watch: None,
            config: rush_config::Config::test_default(),
            variables: rush_build::Variables::empty(),
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: Default::default(),
            dotenv_secrets: Default::default(),
            domain: "test.local".to_string(),
            cross_compile: "native".to_string(),
            health_check: None,
            startup_probe: None,
        }
    }
}
