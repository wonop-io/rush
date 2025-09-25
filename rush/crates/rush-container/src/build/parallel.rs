//! Parallel build execution with dependency management
//!
//! This module provides parallel build execution while respecting
//! component dependencies and managing resource constraints.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use futures::future::{join_all, BoxFuture};
use log::{debug, info, warn};
use rush_build::ComponentBuildSpec;
use rush_core::Result;
use tokio::sync::Semaphore;

use crate::build::orchestrator::BuildOrchestrator;

/// Dependency graph for build ordering
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Map of component name to its dependencies
    dependencies: HashMap<String, Vec<String>>,
    /// Map of component name to components that depend on it
    dependents: HashMap<String, Vec<String>>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
            dependents: HashMap::new(),
        }
    }

    /// Add a dependency to the graph
    pub fn add_dependency(&mut self, component: String, depends_on: impl Into<Vec<String>>) {
        let deps = depends_on.into();

        // Update dependencies map
        self.dependencies.insert(component.clone(), deps.clone());

        // Update reverse dependency map
        for dep in deps {
            self.dependents
                .entry(dep)
                .or_default()
                .push(component.clone());
        }
    }

    /// Get build order using topological sort
    pub fn get_build_order(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Initialize in-degree counts
        for component in self.dependencies.keys() {
            let degree = self
                .dependencies
                .get(component)
                .map(|deps| deps.len())
                .unwrap_or(0);
            in_degree.insert(component.clone(), degree);

            if degree == 0 {
                queue.push_back(component.clone());
            }
        }

        // Process nodes with 0 in-degree
        while let Some(component) = queue.pop_front() {
            result.push(component.clone());

            // Reduce in-degree of dependent nodes
            if let Some(dependents) = self.dependents.get(&component) {
                for dependent in dependents {
                    if let Some(degree) = in_degree.get_mut(dependent) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent.clone());
                        }
                    }
                }
            }
        }

        // Check for cycles
        if result.len() != self.dependencies.len() {
            return Err(rush_core::Error::Configuration(
                "Circular dependency detected in component graph".to_string(),
            ));
        }

        Ok(result)
    }

    /// Create a dependency graph from component specs
    pub fn from_specs(specs: &[ComponentBuildSpec]) -> Self {
        let mut dependencies = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for spec in specs {
            dependencies.insert(spec.component_name.clone(), spec.depends_on.clone());

            // Build reverse dependency map
            for dep in &spec.depends_on {
                dependents
                    .entry(dep.clone())
                    .or_default()
                    .push(spec.component_name.clone());
            }
        }

        Self {
            dependencies,
            dependents,
        }
    }

    /// Get components with no dependencies (can build immediately)
    pub fn get_root_components(&self) -> Vec<String> {
        self.dependencies
            .iter()
            .filter_map(|(name, deps)| {
                if deps.is_empty() {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get components that can be built after the given set is complete
    pub fn get_next_buildable(&self, completed: &HashSet<String>) -> Vec<String> {
        self.dependencies
            .iter()
            .filter_map(|(name, deps)| {
                // Skip if already completed
                if completed.contains(name) {
                    return None;
                }

                // Check if all dependencies are satisfied
                let all_deps_satisfied = deps.iter().all(|dep| completed.contains(dep));

                if all_deps_satisfied {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if all components are complete
    pub fn is_complete(&self, completed: &HashSet<String>) -> bool {
        self.dependencies
            .keys()
            .all(|name| completed.contains(name))
    }

    /// Perform topological sort to get build groups
    pub fn get_build_groups(&self) -> Vec<Vec<String>> {
        let mut groups = Vec::new();
        let mut completed = HashSet::new();

        while !self.is_complete(&completed) {
            let buildable = self.get_next_buildable(&completed);

            if buildable.is_empty() {
                warn!("Circular dependency detected in build graph");
                break;
            }

            // Add all buildable components to completed set
            for component in &buildable {
                completed.insert(component.clone());
            }

            groups.push(buildable);
        }

        groups
    }
}

/// Parallel build executor
pub struct ParallelBuildExecutor {
    orchestrator: Arc<BuildOrchestrator>,
    max_parallel: usize,
    semaphore: Arc<Semaphore>,
}

impl ParallelBuildExecutor {
    /// Create a new parallel build executor
    pub fn new(orchestrator: Arc<BuildOrchestrator>, max_parallel: usize) -> Self {
        let semaphore = Arc::new(Semaphore::new(max_parallel));
        Self {
            orchestrator,
            max_parallel,
            semaphore,
        }
    }

    /// Build components in parallel while respecting dependencies
    pub async fn build_parallel(
        &self,
        specs: Vec<ComponentBuildSpec>,
        force_rebuild: bool,
    ) -> Result<HashMap<String, String>> {
        info!(
            "Starting parallel build of {} components (max parallel: {})",
            specs.len(),
            self.max_parallel
        );

        let start_time = std::time::Instant::now();
        let mut built_images = HashMap::new();
        let mut completed = HashSet::new();

        // Create dependency graph
        let graph = DependencyGraph::from_specs(&specs);
        let build_groups = graph.get_build_groups();

        info!("Build will proceed in {} stages", build_groups.len());

        // Create a map for quick spec lookup
        let spec_map: HashMap<String, ComponentBuildSpec> = specs
            .into_iter()
            .map(|spec| (spec.component_name.clone(), spec))
            .collect();

        // Build each group in parallel
        for (group_idx, group) in build_groups.iter().enumerate() {
            info!(
                "Building group {} with {} components: {:?}",
                group_idx + 1,
                group.len(),
                group
            );

            let group_start = std::time::Instant::now();

            // Create futures for all components in this group
            let mut build_futures = Vec::new();

            for component_name in group {
                let spec = match spec_map.get(component_name) {
                    Some(s) => s.clone(),
                    None => {
                        warn!("Component {} not found in spec map", component_name);
                        continue;
                    }
                };

                let orchestrator = Arc::clone(&self.orchestrator);
                let semaphore = Arc::clone(&self.semaphore);
                let all_specs: Vec<ComponentBuildSpec> = spec_map.values().cloned().collect();

                // Create build future with semaphore control
                let build_future: BoxFuture<'_, Result<(String, String)>> = Box::pin(async move {
                    // Acquire semaphore permit to limit parallelism
                    let _permit = semaphore.acquire().await.map_err(|e| {
                        rush_core::Error::Internal(format!("Failed to acquire build permit: {e}"))
                    })?;

                    debug!("Building component: {}", spec.component_name);
                    let component_name = spec.component_name.clone();

                    // Check if we should skip build (already exists and not force rebuild)
                    if !force_rebuild {
                        // Compute tag
                        let tag = orchestrator
                            .tag_generator
                            .compute_tag(&spec)
                            .unwrap_or_else(|e| {
                                warn!("Failed to compute tag for {}: {}", component_name, e);
                                "latest".to_string()
                            });

                        let image_name =
                            format!("{}/{}", orchestrator.product_name(), component_name);
                        let full_image = format!("{image_name}:{tag}");

                        // Check if image exists
                        if let Ok(exists) =
                            orchestrator.docker_client().image_exists(&full_image).await
                        {
                            if exists {
                                info!(
                                    "Component {} already built ({}), skipping",
                                    component_name, full_image
                                );
                                return Ok((component_name, full_image));
                            }
                        }
                    }

                    // Build the component
                    match orchestrator.build_single(spec, &all_specs).await {
                        Ok(image) => Ok((component_name, image)),
                        Err(e) => {
                            warn!("Failed to build component {}: {}", component_name, e);
                            Err(e)
                        }
                    }
                });

                build_futures.push(build_future);
            }

            // Execute all builds in this group concurrently
            let results = join_all(build_futures).await;

            // Process results
            for result in results {
                match result {
                    Ok((name, image)) => {
                        built_images.insert(name.clone(), image);
                        completed.insert(name);
                    }
                    Err(e) => {
                        warn!("Build failed in group {}: {}", group_idx + 1, e);
                        // Continue with other builds
                    }
                }
            }

            let group_duration = group_start.elapsed();
            info!("Group {} completed in {:?}", group_idx + 1, group_duration);
        }

        let total_duration = start_time.elapsed();
        info!(
            "Parallel build completed in {:?} ({} components built)",
            total_duration,
            built_images.len()
        );

        // Record performance metrics
        crate::profiling::global_tracker()
            .record("parallel_build_total", total_duration, {
                let mut metadata = HashMap::new();
                metadata.insert("components".to_string(), built_images.len().to_string());
                metadata.insert("max_parallel".to_string(), self.max_parallel.to_string());
                metadata
            })
            .await;

        Ok(built_images)
    }

    /// Build components in optimized groups based on dependencies
    pub async fn build_optimized(
        &self,
        specs: Vec<ComponentBuildSpec>,
        force_rebuild: bool,
    ) -> Result<HashMap<String, String>> {
        // Analyze dependencies and resource requirements
        let graph = DependencyGraph::from_specs(&specs);
        let groups = graph.get_build_groups();

        info!(
            "Optimized build plan: {} groups from {} components",
            groups.len(),
            specs.len()
        );

        // Log the build plan
        for (idx, group) in groups.iter().enumerate() {
            debug!("  Group {}: {:?}", idx + 1, group);
        }

        // Use parallel build with the optimized groups
        self.build_parallel(specs, force_rebuild).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dependency_graph() {
        let mut graph = DependencyGraph::new();

        // Create a simple dependency chain: A -> B -> C
        graph.add_dependency("A".to_string(), vec!["B".to_string()]);
        graph.add_dependency("B".to_string(), vec!["C".to_string()]);
        graph.add_dependency("C".to_string(), vec![]);

        let order = graph.get_build_order().unwrap();
        assert_eq!(order, vec!["C", "B", "A"]);
    }

    #[tokio::test]
    async fn test_circular_dependency_detection() {
        let mut graph = DependencyGraph::new();

        // Create a circular dependency: A -> B -> C -> A
        graph.add_dependency("A".to_string(), vec!["B".to_string()]);
        graph.add_dependency("B".to_string(), vec!["C".to_string()]);
        graph.add_dependency("C".to_string(), vec!["A".to_string()]);

        let result = graph.get_build_order();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Circular dependency"));
    }

    #[tokio::test]
    async fn test_parallel_execution() {
        // Create mock dependency graph without full specs
        let mut graph = DependencyGraph::new();
        graph.add_dependency("component1".to_string(), vec![]);
        graph.add_dependency("component2".to_string(), vec![]);

        let order = graph.get_build_order().unwrap();

        // Both should be buildable in any order since they have no dependencies
        assert_eq!(order.len(), 2);
        assert!(order.contains(&"component1".to_string()));
        assert!(order.contains(&"component2".to_string()));
    }

    #[tokio::test]
    async fn test_complex_dependency_graph() {
        let mut graph = DependencyGraph::new();

        // Build the dependency graph directly
        graph.add_dependency("frontend".to_string(), vec!["api".to_string()]);
        graph.add_dependency(
            "api".to_string(),
            vec!["database".to_string(), "cache".to_string()],
        );
        graph.add_dependency("database".to_string(), vec![]);
        graph.add_dependency("cache".to_string(), vec![]);
        graph.add_dependency("worker".to_string(), vec!["api".to_string()]);

        let order = graph.get_build_order().unwrap();

        // Verify the build order respects dependencies
        let frontend_idx = order.iter().position(|x| x == "frontend").unwrap();
        let api_idx = order.iter().position(|x| x == "api").unwrap();
        let database_idx = order.iter().position(|x| x == "database").unwrap();
        let cache_idx = order.iter().position(|x| x == "cache").unwrap();
        let worker_idx = order.iter().position(|x| x == "worker").unwrap();

        // database and cache should come before api
        assert!(database_idx < api_idx);
        assert!(cache_idx < api_idx);

        // api should come before frontend and worker
        assert!(api_idx < frontend_idx);
        assert!(api_idx < worker_idx);
    }
}
