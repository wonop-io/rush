//! Build performance analysis and optimization
//!
//! This module provides comprehensive analysis of build performance,
//! identifying bottlenecks and suggesting optimizations.

// Performance metrics are tracked internally
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Build performance analyzer
pub struct BuildPerformanceAnalyzer {
    /// Collected metrics
    metrics: HashMap<String, ComponentMetrics>,
    /// Build timeline
    timeline: Vec<BuildEvent>,
    /// Resource utilization data
    resource_usage: ResourceUsageData,
    /// Analysis start time
    start_time: Instant,
}

/// Metrics for a single component
#[derive(Debug, Clone)]
pub struct ComponentMetrics {
    /// Component name
    pub name: String,
    /// Build duration
    pub duration: Duration,
    /// Time spent waiting for dependencies
    pub wait_time: Duration,
    /// Cache hit/miss
    pub cache_hit: bool,
    /// Docker build time
    pub docker_time: Duration,
    /// File I/O time
    pub io_time: Duration,
    /// Network time (pulling images, etc.)
    pub network_time: Duration,
    /// CPU utilization (0-1)
    pub cpu_utilization: f64,
    /// Memory usage in MB
    pub memory_usage_mb: usize,
    /// Build size in MB
    pub build_size_mb: usize,
}

impl ComponentMetrics {
    pub fn new(name: String) -> Self {
        Self {
            name,
            duration: Duration::ZERO,
            wait_time: Duration::ZERO,
            cache_hit: false,
            docker_time: Duration::ZERO,
            io_time: Duration::ZERO,
            network_time: Duration::ZERO,
            cpu_utilization: 0.0,
            memory_usage_mb: 0,
            build_size_mb: 0,
        }
    }

    /// Calculate efficiency score (0-1, higher is better)
    pub fn efficiency_score(&self) -> f64 {
        if self.duration == Duration::ZERO {
            return 1.0;
        }

        let active_time = self.duration - self.wait_time;
        let efficiency = active_time.as_secs_f64() / self.duration.as_secs_f64();

        // Factor in resource utilization
        let resource_efficiency = self.cpu_utilization.min(1.0);

        // Combined score
        efficiency * 0.7 + resource_efficiency * 0.3
    }
}

/// Build event for timeline analysis
#[derive(Debug, Clone)]
pub struct BuildEvent {
    /// Event timestamp
    pub timestamp: Instant,
    /// Component name
    pub component: String,
    /// Event type
    pub event_type: BuildEventType,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Types of build events
#[derive(Debug, Clone, PartialEq)]
pub enum BuildEventType {
    /// Build started
    Started,
    /// Waiting for dependencies
    WaitingForDeps,
    /// Dependencies satisfied
    DepsReady,
    /// Cache check
    CacheCheck,
    /// Docker build started
    DockerBuildStart,
    /// Docker build completed
    DockerBuildEnd,
    /// Build completed
    Completed,
    /// Build failed
    Failed(String),
}

/// Resource usage data
#[derive(Debug, Clone)]
pub struct ResourceUsageData {
    /// CPU usage samples over time
    pub cpu_samples: Vec<(Instant, f64)>,
    /// Memory usage samples over time
    pub memory_samples: Vec<(Instant, usize)>,
    /// Disk I/O samples
    pub io_samples: Vec<(Instant, f64)>,
    /// Network usage samples
    pub network_samples: Vec<(Instant, f64)>,
    /// Peak CPU usage
    pub peak_cpu: f64,
    /// Peak memory usage
    pub peak_memory_mb: usize,
}

impl Default for ResourceUsageData {
    fn default() -> Self {
        Self {
            cpu_samples: Vec::new(),
            memory_samples: Vec::new(),
            io_samples: Vec::new(),
            network_samples: Vec::new(),
            peak_cpu: 0.0,
            peak_memory_mb: 0,
        }
    }
}

impl BuildPerformanceAnalyzer {
    /// Create a new analyzer
    pub fn new() -> Self {
        Self {
            metrics: HashMap::new(),
            timeline: Vec::new(),
            resource_usage: ResourceUsageData::default(),
            start_time: Instant::now(),
        }
    }

    /// Record a build event
    pub fn record_event(&mut self, component: String, event_type: BuildEventType) {
        let event = BuildEvent {
            timestamp: Instant::now(),
            component: component.clone(),
            event_type: event_type.clone(),
            metadata: HashMap::new(),
        };

        self.timeline.push(event.clone());

        // Calculate duration if this is a completion event
        let duration = match &event_type {
            BuildEventType::Completed | BuildEventType::Failed(_) => {
                self.find_start_event(&component)
                    .map(|start| Instant::now() - start.timestamp)
            }
            _ => None,
        };

        // Update component metrics based on event
        let metrics = self.metrics
            .entry(component.clone())
            .or_insert_with(|| ComponentMetrics::new(component));

        if let Some(d) = duration {
            metrics.duration = d;
        }
    }

    /// Find the start event for a component
    fn find_start_event(&self, component: &str) -> Option<&BuildEvent> {
        self.timeline
            .iter()
            .rev()
            .find(|e| e.component == component && e.event_type == BuildEventType::Started)
    }

    /// Record resource usage sample
    pub fn record_resource_usage(&mut self, cpu: f64, memory_mb: usize) {
        let now = Instant::now();

        self.resource_usage.cpu_samples.push((now, cpu));
        self.resource_usage.memory_samples.push((now, memory_mb));

        // Update peaks
        self.resource_usage.peak_cpu = self.resource_usage.peak_cpu.max(cpu);
        self.resource_usage.peak_memory_mb = self.resource_usage.peak_memory_mb.max(memory_mb);

        // Keep only last 1000 samples to avoid memory issues
        if self.resource_usage.cpu_samples.len() > 1000 {
            self.resource_usage.cpu_samples.remove(0);
            self.resource_usage.memory_samples.remove(0);
        }
    }

    /// Update component metrics
    pub fn update_component_metrics(
        &mut self,
        component: String,
        updates: impl FnOnce(&mut ComponentMetrics),
    ) {
        let metrics = self.metrics
            .entry(component.clone())
            .or_insert_with(|| ComponentMetrics::new(component));
        updates(metrics);
    }

    /// Analyze build performance and generate report
    pub fn analyze(&self) -> PerformanceAnalysisReport {
        let total_duration = Instant::now() - self.start_time;

        // Calculate parallel efficiency
        let total_component_time: Duration = self.metrics
            .values()
            .map(|m| m.duration)
            .sum();

        let parallelization_efficiency = if total_duration > Duration::ZERO {
            total_component_time.as_secs_f64() / total_duration.as_secs_f64()
        } else {
            0.0
        };

        // Find bottlenecks
        let bottlenecks = self.identify_bottlenecks();

        // Calculate waste
        let total_wait_time: Duration = self.metrics
            .values()
            .map(|m| m.wait_time)
            .sum();

        let waste_percentage = if total_component_time > Duration::ZERO {
            (total_wait_time.as_secs_f64() / total_component_time.as_secs_f64()) * 100.0
        } else {
            0.0
        };

        // Find slowest components
        let mut components_by_duration: Vec<_> = self.metrics.values().collect();
        components_by_duration.sort_by_key(|m| std::cmp::Reverse(m.duration));

        let slowest_components: Vec<ComponentAnalysis> = components_by_duration
            .into_iter()
            .take(5)
            .map(|m| ComponentAnalysis {
                name: m.name.clone(),
                duration: m.duration,
                efficiency: m.efficiency_score(),
                bottleneck_type: self.classify_bottleneck(m),
            })
            .collect();

        // Generate optimization suggestions
        let suggestions = self.generate_optimization_suggestions();

        PerformanceAnalysisReport {
            total_duration,
            total_component_time,
            parallelization_efficiency,
            waste_percentage,
            peak_cpu: self.resource_usage.peak_cpu,
            peak_memory_mb: self.resource_usage.peak_memory_mb,
            cache_hit_rate: self.calculate_cache_hit_rate(),
            bottlenecks,
            slowest_components,
            suggestions,
        }
    }

    /// Identify bottlenecks in the build process
    fn identify_bottlenecks(&self) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        // Check for sequential bottlenecks
        let sequential_chains = self.find_sequential_chains();
        for chain in sequential_chains {
            if chain.len() > 3 {
                bottlenecks.push(Bottleneck {
                    bottleneck_type: BottleneckType::Sequential,
                    components: chain,
                    impact: Duration::from_secs(10), // Placeholder
                    description: "Long sequential dependency chain".to_string(),
                });
            }
        }

        // Check for resource bottlenecks
        if self.resource_usage.peak_cpu > 0.9 {
            bottlenecks.push(Bottleneck {
                bottleneck_type: BottleneckType::Cpu,
                components: vec![],
                impact: Duration::from_secs(5),
                description: format!("CPU bottleneck: peak usage {:.1}%",
                    self.resource_usage.peak_cpu * 100.0),
            });
        }

        if self.resource_usage.peak_memory_mb > 7000 {
            bottlenecks.push(Bottleneck {
                bottleneck_type: BottleneckType::Memory,
                components: vec![],
                impact: Duration::from_secs(5),
                description: format!("Memory bottleneck: peak usage {} MB",
                    self.resource_usage.peak_memory_mb),
            });
        }

        // Check for I/O bottlenecks
        for metrics in self.metrics.values() {
            if metrics.io_time > metrics.duration / 3 {
                bottlenecks.push(Bottleneck {
                    bottleneck_type: BottleneckType::Io,
                    components: vec![metrics.name.clone()],
                    impact: metrics.io_time,
                    description: format!("I/O bottleneck in {}", metrics.name),
                });
            }
        }

        bottlenecks
    }

    /// Find sequential dependency chains
    fn find_sequential_chains(&self) -> Vec<Vec<String>> {
        let mut chains = Vec::new();
        let mut visited = HashSet::new();

        // Simple DFS to find chains
        for component in self.metrics.keys() {
            if !visited.contains(component) {
                let chain = self.trace_chain(component, &mut visited);
                if chain.len() > 1 {
                    chains.push(chain);
                }
            }
        }

        chains
    }

    /// Trace a dependency chain
    fn trace_chain(&self, start: &str, visited: &mut HashSet<String>) -> Vec<String> {
        let chain = vec![start.to_string()];
        visited.insert(start.to_string());

        // This is simplified - would need actual dependency graph
        // For now, we'll use the timeline to infer dependencies
        chain
    }

    /// Classify the type of bottleneck for a component
    fn classify_bottleneck(&self, metrics: &ComponentMetrics) -> BottleneckType {
        if metrics.wait_time > metrics.duration / 2 {
            BottleneckType::Dependency
        } else if metrics.docker_time > metrics.duration / 2 {
            BottleneckType::Docker
        } else if metrics.network_time > metrics.duration / 3 {
            BottleneckType::Network
        } else if metrics.io_time > metrics.duration / 3 {
            BottleneckType::Io
        } else if metrics.cpu_utilization < 0.5 {
            BottleneckType::UnderUtilized
        } else {
            BottleneckType::None
        }
    }

    /// Calculate overall cache hit rate
    fn calculate_cache_hit_rate(&self) -> f64 {
        if self.metrics.is_empty() {
            return 0.0;
        }

        let hits = self.metrics.values().filter(|m| m.cache_hit).count();
        hits as f64 / self.metrics.len() as f64
    }

    /// Generate optimization suggestions
    fn generate_optimization_suggestions(&self) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Check parallelization
        if self.calculate_cache_hit_rate() < 0.7 {
            suggestions.push(OptimizationSuggestion {
                priority: Priority::High,
                category: "Cache".to_string(),
                suggestion: "Cache hit rate is below 70%. Consider improving cache key computation or increasing cache size.".to_string(),
                estimated_improvement: Duration::from_secs(20),
            });
        }

        // Check for long wait times
        for metrics in self.metrics.values() {
            if metrics.wait_time > Duration::from_secs(30) {
                suggestions.push(OptimizationSuggestion {
                    priority: Priority::Medium,
                    category: "Dependencies".to_string(),
                    suggestion: format!(
                        "Component '{}' waits {:?} for dependencies. Consider restructuring dependencies.",
                        metrics.name, metrics.wait_time
                    ),
                    estimated_improvement: metrics.wait_time / 2,
                });
            }
        }

        // Check resource utilization
        if self.resource_usage.peak_cpu < 0.5 {
            suggestions.push(OptimizationSuggestion {
                priority: Priority::Medium,
                category: "Resources".to_string(),
                suggestion: "CPU utilization is low. Consider increasing parallelism.".to_string(),
                estimated_improvement: Duration::from_secs(10),
            });
        }

        suggestions
    }
}

/// Performance analysis report
#[derive(Debug, Clone)]
pub struct PerformanceAnalysisReport {
    /// Total build duration
    pub total_duration: Duration,
    /// Sum of all component build times
    pub total_component_time: Duration,
    /// Parallelization efficiency (>1 means good parallelization)
    pub parallelization_efficiency: f64,
    /// Percentage of time wasted waiting
    pub waste_percentage: f64,
    /// Peak CPU usage
    pub peak_cpu: f64,
    /// Peak memory usage
    pub peak_memory_mb: usize,
    /// Cache hit rate
    pub cache_hit_rate: f64,
    /// Identified bottlenecks
    pub bottlenecks: Vec<Bottleneck>,
    /// Slowest components
    pub slowest_components: Vec<ComponentAnalysis>,
    /// Optimization suggestions
    pub suggestions: Vec<OptimizationSuggestion>,
}

impl PerformanceAnalysisReport {
    /// Print the report
    pub fn print(&self) {
        println!("\n=== Build Performance Analysis ===");
        println!("Total Duration: {:?}", self.total_duration);
        println!("Component Time Sum: {:?}", self.total_component_time);
        println!("Parallelization Efficiency: {:.2}x", self.parallelization_efficiency);
        println!("Time Wasted Waiting: {:.1}%", self.waste_percentage);
        println!("\nResource Usage:");
        println!("  Peak CPU: {:.1}%", self.peak_cpu * 100.0);
        println!("  Peak Memory: {} MB", self.peak_memory_mb);
        println!("  Cache Hit Rate: {:.1}%", self.cache_hit_rate * 100.0);

        if !self.bottlenecks.is_empty() {
            println!("\nBottlenecks:");
            for bottleneck in &self.bottlenecks {
                println!("  • {} - {} (Impact: {:?})",
                    bottleneck.bottleneck_type,
                    bottleneck.description,
                    bottleneck.impact
                );
            }
        }

        if !self.slowest_components.is_empty() {
            println!("\nSlowest Components:");
            for comp in &self.slowest_components {
                println!("  • {} - {:?} (Efficiency: {:.1}%)",
                    comp.name,
                    comp.duration,
                    comp.efficiency * 100.0
                );
            }
        }

        if !self.suggestions.is_empty() {
            println!("\nOptimization Suggestions:");
            for suggestion in &self.suggestions {
                println!("  [{:?}] {}: {}",
                    suggestion.priority,
                    suggestion.category,
                    suggestion.suggestion
                );
            }
        }
    }
}

/// Bottleneck information
#[derive(Debug, Clone)]
pub struct Bottleneck {
    pub bottleneck_type: BottleneckType,
    pub components: Vec<String>,
    pub impact: Duration,
    pub description: String,
}

/// Types of bottlenecks
#[derive(Debug, Clone, PartialEq)]
pub enum BottleneckType {
    Sequential,
    Dependency,
    Cpu,
    Memory,
    Io,
    Network,
    Docker,
    UnderUtilized,
    None,
}

impl std::fmt::Display for BottleneckType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sequential => write!(f, "Sequential"),
            Self::Dependency => write!(f, "Dependency"),
            Self::Cpu => write!(f, "CPU"),
            Self::Memory => write!(f, "Memory"),
            Self::Io => write!(f, "I/O"),
            Self::Network => write!(f, "Network"),
            Self::Docker => write!(f, "Docker"),
            Self::UnderUtilized => write!(f, "Under-utilized"),
            Self::None => write!(f, "None"),
        }
    }
}

/// Component analysis
#[derive(Debug, Clone)]
pub struct ComponentAnalysis {
    pub name: String,
    pub duration: Duration,
    pub efficiency: f64,
    pub bottleneck_type: BottleneckType,
}

/// Optimization suggestion
#[derive(Debug, Clone)]
pub struct OptimizationSuggestion {
    pub priority: Priority,
    pub category: String,
    pub suggestion: String,
    pub estimated_improvement: Duration,
}

/// Priority levels
#[derive(Debug, Clone, PartialEq, Ord, PartialOrd, Eq)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_analyzer() {
        let mut analyzer = BuildPerformanceAnalyzer::new();

        // Record some events
        analyzer.record_event("frontend".to_string(), BuildEventType::Started);
        std::thread::sleep(Duration::from_millis(100));
        analyzer.record_event("frontend".to_string(), BuildEventType::Completed);

        analyzer.record_event("api".to_string(), BuildEventType::Started);
        std::thread::sleep(Duration::from_millis(150));
        analyzer.record_event("api".to_string(), BuildEventType::Completed);

        // Record resource usage
        analyzer.record_resource_usage(0.75, 2048);
        analyzer.record_resource_usage(0.85, 3072);

        // Generate report
        let report = analyzer.analyze();
        assert!(report.total_duration > Duration::ZERO);
        assert!(report.peak_cpu > 0.0);
        assert!(report.peak_memory_mb > 0);
    }

    #[test]
    fn test_bottleneck_detection() {
        let mut analyzer = BuildPerformanceAnalyzer::new();

        // Create a component with high wait time
        analyzer.update_component_metrics("slow".to_string(), |m| {
            m.duration = Duration::from_secs(60);
            m.wait_time = Duration::from_secs(40);
        });

        let report = analyzer.analyze();
        assert!(report.waste_percentage > 0.0);
    }
}