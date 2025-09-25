//! Advanced dependency resolution with critical path optimization
//!
//! This module provides sophisticated dependency analysis including
//! critical path identification, parallel group optimization, and
//! build time estimation.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use petgraph::algo::{all_simple_paths, is_cyclic_directed, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use rush_build::ComponentBuildSpec;
use rush_core::{Error, Result};

/// Build time estimation for components
#[derive(Debug, Clone)]
pub struct BuildTimeEstimate {
    /// Historical average build time
    pub average: Duration,
    /// Standard deviation
    pub std_dev: Duration,
    /// P95 build time
    pub p95: Duration,
    /// Number of samples
    pub samples: usize,
}

impl Default for BuildTimeEstimate {
    fn default() -> Self {
        Self {
            average: Duration::from_secs(30),
            std_dev: Duration::from_secs(10),
            p95: Duration::from_secs(60),
            samples: 0,
        }
    }
}

/// Advanced dependency resolver with optimization
pub struct DependencyResolver {
    /// Directed graph of dependencies
    graph: DiGraph<String, ()>,
    /// Node index mapping
    node_indices: HashMap<String, NodeIndex>,
    /// Build time estimates
    time_estimates: HashMap<String, BuildTimeEstimate>,
    /// Cache of computed paths
    path_cache: HashMap<(String, String), Vec<Vec<String>>>,
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyResolver {
    /// Create a new dependency resolver
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
            time_estimates: HashMap::new(),
            path_cache: HashMap::new(),
        }
    }

    /// Add a component with its dependencies
    pub fn add_component(&mut self, name: String, dependencies: Vec<String>) {
        // Ensure component node exists
        let component_idx = self.get_or_create_node(name.clone());

        // Add edges for dependencies
        for dep in dependencies {
            let dep_idx = self.get_or_create_node(dep);
            self.graph.add_edge(dep_idx, component_idx, ());
        }
    }

    /// Get or create a node for a component
    fn get_or_create_node(&mut self, name: String) -> NodeIndex {
        if let Some(&idx) = self.node_indices.get(&name) {
            idx
        } else {
            let idx = self.graph.add_node(name.clone());
            self.node_indices.insert(name, idx);
            idx
        }
    }

    /// Build components from specifications
    pub fn from_specs(specs: &[ComponentBuildSpec]) -> Self {
        let mut resolver = Self::new();

        for spec in specs {
            resolver.add_component(spec.component_name.clone(), spec.depends_on.clone());

            // Add default time estimate
            resolver
                .time_estimates
                .insert(spec.component_name.clone(), BuildTimeEstimate::default());
        }

        resolver
    }

    /// Update build time estimate for a component
    pub fn update_time_estimate(&mut self, component: &str, duration: Duration) {
        let estimate = self
            .time_estimates
            .entry(component.to_string())
            .or_default();

        // Simple moving average update
        let n = estimate.samples as f64;
        let new_avg = if n > 0.0 {
            let current = estimate.average.as_secs_f64();
            let new = duration.as_secs_f64();
            Duration::from_secs_f64((current * n + new) / (n + 1.0))
        } else {
            duration
        };

        estimate.average = new_avg;
        estimate.samples += 1;

        // Update P95 (simplified - just use max of average + 2*stddev or actual)
        let p95_estimate =
            Duration::from_secs_f64(new_avg.as_secs_f64() + 2.0 * estimate.std_dev.as_secs_f64());
        estimate.p95 = p95_estimate.max(duration);
    }

    /// Check if the graph has cycles
    pub fn has_cycles(&self) -> bool {
        is_cyclic_directed(&self.graph)
    }

    /// Get optimized build groups with parallelization
    pub fn optimize_build_order(&self) -> Result<Vec<Vec<String>>> {
        if self.has_cycles() {
            return Err(Error::Configuration(
                "Circular dependency detected in build graph".to_string(),
            ));
        }

        // Perform topological sort
        let sorted = toposort(&self.graph, None)
            .map_err(|_| Error::Configuration("Failed to sort dependencies".to_string()))?;

        // Group by levels (components that can be built in parallel)
        let mut levels = Vec::new();
        let mut visited = HashSet::new();
        let mut remaining: HashSet<_> = sorted.iter().cloned().collect();

        while !remaining.is_empty() {
            let mut current_level = Vec::new();

            for &node_idx in &remaining.clone() {
                // Check if all dependencies are satisfied
                let mut can_build = true;
                for neighbor in self.graph.neighbors_directed(node_idx, Direction::Incoming) {
                    if !visited.contains(&neighbor) {
                        can_build = false;
                        break;
                    }
                }

                if can_build {
                    current_level.push(node_idx);
                    remaining.remove(&node_idx);
                }
            }

            if current_level.is_empty() && !remaining.is_empty() {
                return Err(Error::Configuration(
                    "Unable to resolve build order".to_string(),
                ));
            }

            // Convert indices to names
            let level_names: Vec<String> = current_level
                .iter()
                .filter_map(|&idx| self.graph.node_weight(idx))
                .cloned()
                .collect();

            if !level_names.is_empty() {
                visited.extend(current_level);
                levels.push(level_names);
            }
        }

        Ok(levels)
    }

    /// Find the critical path (longest path) in the build graph
    pub fn find_critical_path(&self) -> Result<(Vec<String>, Duration)> {
        let mut longest_path = Vec::new();
        let mut max_duration = Duration::from_secs(0);

        // Find all source nodes (no incoming edges)
        let sources: Vec<_> = self
            .node_indices
            .iter()
            .filter(|(_, &idx)| {
                self.graph
                    .neighbors_directed(idx, Direction::Incoming)
                    .count()
                    == 0
            })
            .map(|(name, _)| name.clone())
            .collect();

        // Find all sink nodes (no outgoing edges)
        let sinks: Vec<_> = self
            .node_indices
            .iter()
            .filter(|(_, &idx)| {
                self.graph
                    .neighbors_directed(idx, Direction::Outgoing)
                    .count()
                    == 0
            })
            .map(|(name, _)| name.clone())
            .collect();

        // Find longest path from each source to each sink
        for source in &sources {
            for sink in &sinks {
                if let Some(path) = self.find_longest_path(source, sink)? {
                    let duration = self.calculate_path_duration(&path);
                    if duration > max_duration {
                        max_duration = duration;
                        longest_path = path;
                    }
                }
            }
        }

        Ok((longest_path, max_duration))
    }

    /// Find the longest path between two nodes
    fn find_longest_path(&self, from: &str, to: &str) -> Result<Option<Vec<String>>> {
        let from_idx = self
            .node_indices
            .get(from)
            .ok_or_else(|| Error::Internal(format!("Component {from} not found")))?;
        let to_idx = self
            .node_indices
            .get(to)
            .ok_or_else(|| Error::Internal(format!("Component {to} not found")))?;

        // Use cache if available
        let cache_key = (from.to_string(), to.to_string());
        if let Some(cached_paths) = self.path_cache.get(&cache_key) {
            return Ok(cached_paths.first().cloned());
        }

        // Find all simple paths
        let paths: Vec<Vec<NodeIndex>> =
            all_simple_paths(&self.graph, *from_idx, *to_idx, 0, None).collect();

        if paths.is_empty() {
            return Ok(None);
        }

        // Find the longest path by duration
        let mut longest_path = Vec::new();
        let mut max_duration = Duration::from_secs(0);

        for path in paths {
            let path_names: Vec<String> = path
                .iter()
                .filter_map(|&idx| self.graph.node_weight(idx))
                .cloned()
                .collect();

            let duration = self.calculate_path_duration(&path_names);
            if duration > max_duration {
                max_duration = duration;
                longest_path = path_names;
            }
        }

        Ok(Some(longest_path))
    }

    /// Calculate total duration for a path
    fn calculate_path_duration(&self, path: &[String]) -> Duration {
        path.iter()
            .filter_map(|name| self.time_estimates.get(name))
            .map(|estimate| estimate.average)
            .sum()
    }

    /// Get build statistics
    pub fn get_build_stats(&self) -> BuildStats {
        let total_components = self.node_indices.len();
        let (critical_path, critical_duration) = self
            .find_critical_path()
            .unwrap_or((Vec::new(), Duration::from_secs(0)));

        let parallelization_factor = if critical_duration.as_secs() > 0 {
            let total_time: Duration = self.time_estimates.values().map(|e| e.average).sum();
            total_time.as_secs_f64() / critical_duration.as_secs_f64()
        } else {
            1.0
        };

        BuildStats {
            total_components,
            critical_path,
            critical_duration,
            estimated_parallel_time: critical_duration,
            parallelization_factor,
            max_parallel_width: self
                .optimize_build_order()
                .ok()
                .and_then(|groups| groups.iter().map(|g| g.len()).max())
                .unwrap_or(1),
        }
    }

    /// Optimize build order for resource constraints
    pub fn optimize_for_resources(&self, max_parallel: usize) -> Result<Vec<Vec<String>>> {
        let groups = self.optimize_build_order()?;

        // Split groups that exceed max_parallel
        let mut optimized = Vec::new();
        for group in groups {
            if group.len() <= max_parallel {
                optimized.push(group);
            } else {
                // Split into smaller groups based on estimated build time
                let mut sorted_group = group.clone();
                sorted_group.sort_by_key(|name| {
                    self.time_estimates
                        .get(name)
                        .map(|e| e.average)
                        .unwrap_or(Duration::from_secs(30))
                });

                for chunk in sorted_group.chunks(max_parallel) {
                    optimized.push(chunk.to_vec());
                }
            }
        }

        Ok(optimized)
    }
}

/// Build statistics
#[derive(Debug, Clone)]
pub struct BuildStats {
    /// Total number of components
    pub total_components: usize,
    /// Critical path components
    pub critical_path: Vec<String>,
    /// Critical path duration
    pub critical_duration: Duration,
    /// Estimated parallel build time
    pub estimated_parallel_time: Duration,
    /// Parallelization factor (speedup)
    pub parallelization_factor: f64,
    /// Maximum parallel width
    pub max_parallel_width: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_resolver() {
        let mut resolver = DependencyResolver::new();

        resolver.add_component("frontend".to_string(), vec!["api".to_string()]);
        resolver.add_component("api".to_string(), vec!["database".to_string()]);
        resolver.add_component("database".to_string(), vec![]);
        resolver.add_component("cache".to_string(), vec![]);
        resolver.add_component(
            "worker".to_string(),
            vec!["api".to_string(), "cache".to_string()],
        );

        let groups = resolver.optimize_build_order().unwrap();

        // First group should have components with no dependencies
        assert!(groups[0].contains(&"database".to_string()));
        assert!(groups[0].contains(&"cache".to_string()));

        // API depends on database, so should come later
        let api_group = groups
            .iter()
            .position(|g| g.contains(&"api".to_string()))
            .unwrap();
        let db_group = groups
            .iter()
            .position(|g| g.contains(&"database".to_string()))
            .unwrap();
        assert!(api_group > db_group);
    }

    #[test]
    fn test_cycle_detection() {
        let mut resolver = DependencyResolver::new();

        resolver.add_component("A".to_string(), vec!["B".to_string()]);
        resolver.add_component("B".to_string(), vec!["C".to_string()]);
        resolver.add_component("C".to_string(), vec!["A".to_string()]);

        assert!(resolver.has_cycles());
        assert!(resolver.optimize_build_order().is_err());
    }

    #[test]
    fn test_resource_optimization() {
        let mut resolver = DependencyResolver::new();

        // Create a wide graph with many parallel components
        for i in 0..10 {
            resolver.add_component(format!("component_{}", i), vec![]);
        }

        let groups = resolver.optimize_for_resources(3).unwrap();

        // Should split into multiple groups of max 3
        for group in &groups {
            assert!(group.len() <= 3);
        }
    }
}
