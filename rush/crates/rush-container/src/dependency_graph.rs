//! Dependency graph for container startup ordering
//!
//! This module provides a dependency graph implementation that ensures containers
//! are started in the correct order based on their dependencies. It includes
//! cycle detection and wave-based startup planning for optimal parallelization.

use std::collections::{HashMap, HashSet, VecDeque};

use log::{debug, error, info, warn};
use rush_build::ComponentBuildSpec;
use rush_core::error::{Error, Result};

/// Represents the state of a node in the dependency graph
#[derive(Debug, Clone, PartialEq)]
pub enum NodeState {
    /// Component has not been started yet
    Pending,
    /// Component is currently starting
    Starting,
    /// Component is started and waiting for health check
    WaitingForHealth,
    /// Component is healthy and ready
    Healthy,
    /// Component failed to start or become healthy
    Failed(String),
}

/// A node in the dependency graph representing a component
#[derive(Debug, Clone)]
pub struct Node {
    /// Component name
    pub name: String,
    /// Component build specification
    pub spec: ComponentBuildSpec,
    /// Current state of the component
    pub state: NodeState,
}

impl Node {
    /// Create a new node from a component spec
    pub fn new(spec: ComponentBuildSpec) -> Self {
        Self {
            name: spec.component_name.clone(),
            spec,
            state: NodeState::Pending,
        }
    }

    /// Check if the node is in a terminal state (Healthy or Failed)
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, NodeState::Healthy | NodeState::Failed(_))
    }

    /// Check if the node is ready (Healthy)
    pub fn is_ready(&self) -> bool {
        matches!(self.state, NodeState::Healthy)
    }
}

/// Dependency graph for managing component startup order
#[derive(Debug)]
pub struct DependencyGraph {
    /// All nodes in the graph, keyed by component name
    nodes: HashMap<String, Node>,
    /// Forward edges: component -> list of components that depend on it
    edges: HashMap<String, Vec<String>>,
    /// Reverse edges: component -> list of components it depends on
    reverse_edges: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
        }
    }

    /// Build a dependency graph from component specifications
    /// with support for LocalService mappings
    pub fn from_specs_with_local_services(
        specs: Vec<ComponentBuildSpec>,
        local_services: &HashMap<String, String>, // Maps service names to their types (e.g., "database" -> "postgresql")
    ) -> Result<Self> {
        let mut graph = Self::new();

        // Log what components we're processing
        info!(
            "Building dependency graph from {} component specs",
            specs.len()
        );
        for spec in &specs {
            debug!(
                "  Component '{}': type={:?}, depends_on={:?}",
                spec.component_name, spec.build_type, spec.depends_on
            );
        }

        // Create nodes for all components
        for spec in specs {
            let node = Node::new(spec.clone());
            let name = node.name.clone();

            // Store dependencies (reverse edges), filtering out local services
            let filtered_deps: Vec<String> = spec
                .depends_on
                .iter()
                .filter(|dep| {
                    // Check if this dependency refers to a local service
                    let is_local_service = local_services.contains_key(*dep)
                        || local_services.values().any(|service_type| {
                            // Map common service type names to dependencies
                            // e.g., "postgresql" service type satisfies "postgres" dependency
                            (service_type == "postgresql" && *dep == "postgres")
                                || (service_type == "mysql" && *dep == "mysql")
                                || (service_type == "redis" && *dep == "redis")
                                || (service_type == "mongodb" && *dep == "mongo")
                        });

                    if is_local_service {
                        debug!(
                            "Filtering out local service dependency '{dep}' for component '{name}'"
                        );
                        false
                    } else {
                        true
                    }
                })
                .cloned()
                .collect();

            if !filtered_deps.is_empty() {
                graph.reverse_edges.insert(name.clone(), filtered_deps);
            }

            // Initialize forward edges
            graph.edges.insert(name.clone(), Vec::new());

            // Add node
            graph.nodes.insert(name, node);
        }

        // Log available nodes
        let available_nodes: Vec<String> = graph.nodes.keys().cloned().collect();
        debug!("Available nodes in graph: {available_nodes:?}");

        // Build forward edges from reverse edges
        for (dependent, dependencies) in &graph.reverse_edges {
            for dep in dependencies {
                // Validate that dependency exists
                if !graph.nodes.contains_key(dep) {
                    // Log detailed error with available components
                    error!("Dependency validation failed:");
                    error!("  Component '{dependent}' depends on '{dep}'");
                    error!("  '{dep}' does not exist in available components: {available_nodes:?}");
                    error!("  Hint: Check if '{dep}' is a LocalService that might have a different name in stack.spec.yaml");

                    return Err(Error::Config(format!(
                        "Component '{dependent}' depends on '{dep}', which does not exist. Available components: {available_nodes:?}"
                    )));
                }

                // Add forward edge
                graph
                    .edges
                    .entry(dep.clone())
                    .or_default()
                    .push(dependent.clone());
            }
        }

        // Validate no cycles exist
        graph.validate_acyclic()?;

        info!(
            "Built dependency graph with {} components",
            graph.nodes.len()
        );
        Ok(graph)
    }

    /// Build a dependency graph from component specifications (legacy method)
    pub fn from_specs(specs: Vec<ComponentBuildSpec>) -> Result<Self> {
        // Call the new method with empty local services map for backward compatibility
        Self::from_specs_with_local_services(specs, &HashMap::new())
    }

    /// Get a node by name
    pub fn get_node(&self, name: &str) -> Option<&Node> {
        self.nodes.get(name)
    }

    /// Get a mutable node by name
    pub fn get_node_mut(&mut self, name: &str) -> Option<&mut Node> {
        self.nodes.get_mut(name)
    }

    /// Get all nodes
    pub fn nodes(&self) -> &HashMap<String, Node> {
        &self.nodes
    }

    /// Get components that are ready to start (all dependencies are healthy)
    pub fn get_ready_components(&self) -> Vec<String> {
        let mut ready = Vec::new();

        for (name, node) in &self.nodes {
            // Skip if not pending
            if node.state != NodeState::Pending {
                continue;
            }

            // Check if all dependencies are healthy
            let deps_ready = self
                .get_dependencies(name)
                .iter()
                .all(|dep| self.nodes.get(dep).map(|n| n.is_ready()).unwrap_or(false));

            if deps_ready {
                ready.push(name.clone());
            }
        }

        debug!("Found {} components ready to start", ready.len());
        ready
    }

    /// Get the dependencies of a component
    pub fn get_dependencies(&self, name: &str) -> Vec<String> {
        self.reverse_edges.get(name).cloned().unwrap_or_default()
    }

    /// Get the dependents of a component (components that depend on it)
    pub fn get_dependents(&self, name: &str) -> Vec<String> {
        self.edges.get(name).cloned().unwrap_or_default()
    }

    /// Get all downstream components that depend on the given components (transitively)
    pub fn get_all_downstream_components(&self, components: &HashSet<String>) -> HashSet<String> {
        let mut downstream = HashSet::new();
        let mut to_visit: VecDeque<String> = components.iter().cloned().collect();
        let mut visited = HashSet::new();

        while let Some(component) = to_visit.pop_front() {
            if !visited.insert(component.clone()) {
                continue;
            }

            // Get direct dependents
            let dependents = self.get_dependents(&component);
            for dependent in dependents {
                if !components.contains(&dependent) {
                    downstream.insert(dependent.clone());
                    to_visit.push_back(dependent);
                }
            }
        }

        downstream
    }

    /// Mark a component as starting
    pub fn mark_starting(&mut self, name: &str) -> Result<()> {
        match self.nodes.get_mut(name) {
            Some(node) => {
                if node.state != NodeState::Pending {
                    warn!(
                        "Component {} is not pending (current state: {:?})",
                        name, node.state
                    );
                }
                node.state = NodeState::Starting;
                debug!("Component {name} marked as starting");
                Ok(())
            }
            None => Err(Error::Internal(format!("Component {name} not found"))),
        }
    }

    /// Mark a component as waiting for health check
    pub fn mark_waiting_for_health(&mut self, name: &str) -> Result<()> {
        match self.nodes.get_mut(name) {
            Some(node) => {
                node.state = NodeState::WaitingForHealth;
                debug!("Component {name} marked as waiting for health");
                Ok(())
            }
            None => Err(Error::Internal(format!("Component {name} not found"))),
        }
    }

    /// Mark a component as healthy
    pub fn mark_healthy(&mut self, name: &str) -> Result<()> {
        match self.nodes.get_mut(name) {
            Some(node) => {
                node.state = NodeState::Healthy;
                info!("Component {name} marked as healthy");
                Ok(())
            }
            None => Err(Error::Internal(format!("Component {name} not found"))),
        }
    }

    /// Mark a component as failed
    pub fn mark_failed(&mut self, name: &str, error: String) -> Result<()> {
        match self.nodes.get_mut(name) {
            Some(node) => {
                node.state = NodeState::Failed(error.clone());
                warn!("Component {name} marked as failed: {error}");
                Ok(())
            }
            None => Err(Error::Internal(format!("Component {name} not found"))),
        }
    }

    /// Perform topological sort to get a valid startup order
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        // Calculate in-degrees (number of dependencies for each node)
        for name in self.nodes.keys() {
            let deps = self.get_dependencies(name);
            in_degree.insert(name.clone(), deps.len());
        }

        // Find nodes with no dependencies (in-degree = 0)
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(name, _)| name.clone())
            .collect();

        debug!("Starting topological sort with {} root nodes", queue.len());

        // Process nodes in topological order
        while let Some(name) = queue.pop_front() {
            result.push(name.clone());

            // Reduce in-degree for all dependents
            for dependent in self.get_dependents(&name) {
                if let Some(degree) = in_degree.get_mut(&dependent) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dependent);
                    }
                }
            }
        }

        // Check if all nodes were processed
        if result.len() != self.nodes.len() {
            let unprocessed: Vec<String> = self
                .nodes
                .keys()
                .filter(|k| !result.contains(k))
                .cloned()
                .collect();

            return Err(Error::Config(format!(
                "Circular dependency detected. Unprocessed components: {unprocessed:?}"
            )));
        }

        info!("Topological sort complete: {result:?}");
        Ok(result)
    }

    /// Get startup waves - groups of components that can start in parallel
    pub fn get_startup_waves(&self) -> Result<Vec<Vec<String>>> {
        let sorted = self.topological_sort()?;
        let mut waves = Vec::new();
        let mut processed = HashSet::new();

        while processed.len() < sorted.len() {
            let mut wave = Vec::new();

            for name in &sorted {
                if processed.contains(name) {
                    continue;
                }

                // Check if all dependencies have been processed
                let deps = self.get_dependencies(name);
                let deps_processed = deps.iter().all(|dep| processed.contains(dep));

                if deps_processed {
                    wave.push(name.clone());
                }
            }

            if wave.is_empty() {
                // This shouldn't happen if topological sort succeeded
                return Err(Error::Internal(
                    "Failed to calculate startup waves - possible logic error".to_string(),
                ));
            }

            // Add wave components to processed set
            for name in &wave {
                processed.insert(name.clone());
            }

            debug!("Wave {}: {:?}", waves.len() + 1, wave);
            waves.push(wave);
        }

        info!("Calculated {} startup waves", waves.len());
        Ok(waves)
    }

    /// Validate that the graph is acyclic (no circular dependencies)
    fn validate_acyclic(&self) -> Result<()> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for name in self.nodes.keys() {
            if !visited.contains(name) && self.has_cycle_dfs(name, &mut visited, &mut rec_stack)? {
                return Err(Error::Config(
                    "Circular dependency detected in component dependencies".to_string(),
                ));
            }
        }

        debug!("Graph validation complete: no cycles detected");
        Ok(())
    }

    /// DFS helper for cycle detection
    fn has_cycle_dfs(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> Result<bool> {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());

        // Visit all dependencies
        for dep in self.get_dependencies(node) {
            if !visited.contains(&dep) {
                if self.has_cycle_dfs(&dep, visited, rec_stack)? {
                    return Ok(true);
                }
            } else if rec_stack.contains(&dep) {
                // Found a back edge - cycle detected
                warn!("Cycle detected: {node} -> {dep}");
                return Ok(true);
            }
        }

        rec_stack.remove(node);
        Ok(false)
    }

    /// Get a visual representation of the graph (for debugging)
    pub fn to_dot(&self) -> String {
        let mut dot = String::from("digraph dependencies {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  node [shape=box];\n\n");

        // Add nodes with state coloring
        for (name, node) in &self.nodes {
            let color = match &node.state {
                NodeState::Pending => "white",
                NodeState::Starting => "yellow",
                NodeState::WaitingForHealth => "orange",
                NodeState::Healthy => "green",
                NodeState::Failed(_) => "red",
            };
            dot.push_str(&format!(
                "  \"{name}\" [style=filled, fillcolor={color}];\n"
            ));
        }

        dot.push('\n');

        // Add edges
        for name in self.nodes.keys() {
            for dep in self.get_dependencies(name) {
                dot.push_str(&format!("  \"{dep}\" -> \"{name}\";\n"));
            }
        }

        dot.push_str("}\n");
        dot
    }

    /// Get statistics about the graph
    pub fn stats(&self) -> GraphStats {
        let total_components = self.nodes.len();
        let components_with_deps = self.reverse_edges.len();
        let total_dependencies: usize = self.reverse_edges.values().map(|deps| deps.len()).sum();

        let max_dependencies = self
            .reverse_edges
            .values()
            .map(|deps| deps.len())
            .max()
            .unwrap_or(0);

        let waves = self.get_startup_waves().unwrap_or_default();
        let wave_count = waves.len();
        let max_wave_size = waves.iter().map(|w| w.len()).max().unwrap_or(0);

        GraphStats {
            total_components,
            components_with_deps,
            total_dependencies,
            max_dependencies,
            wave_count,
            max_wave_size,
        }
    }
}

/// Statistics about the dependency graph
#[derive(Debug, Clone)]
pub struct GraphStats {
    pub total_components: usize,
    pub components_with_deps: usize,
    pub total_dependencies: usize,
    pub max_dependencies: usize,
    pub wave_count: usize,
    pub max_wave_size: usize,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rush_build::{BuildType, Variables};
    use rush_config::Config;

    use super::*;

    fn create_test_spec(name: &str, depends_on: Vec<String>) -> ComponentBuildSpec {
        ComponentBuildSpec {
            build_type: BuildType::RustBinary {
                location: "src".to_string(),
                dockerfile_path: "Dockerfile".to_string(),
                context_dir: None,
                features: None,
                precompile_commands: None,
            },
            product_name: "test".to_string(),
            component_name: name.to_string(),
            color: "blue".to_string(),
            depends_on,
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
            config: Config::test_default(),
            variables: Variables::empty(),
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

    #[test]
    fn test_simple_dependency_chain() {
        // Create chain: A -> B -> C
        let specs = vec![
            create_test_spec("A", vec![]),
            create_test_spec("B", vec!["A".to_string()]),
            create_test_spec("C", vec!["B".to_string()]),
        ];

        let graph = DependencyGraph::from_specs(specs).unwrap();
        let sorted = graph.topological_sort().unwrap();

        assert_eq!(sorted, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_parallel_dependencies() {
        // Create diamond: A -> B,C -> D
        let specs = vec![
            create_test_spec("A", vec![]),
            create_test_spec("B", vec!["A".to_string()]),
            create_test_spec("C", vec!["A".to_string()]),
            create_test_spec("D", vec!["B".to_string(), "C".to_string()]),
        ];

        let graph = DependencyGraph::from_specs(specs).unwrap();
        let waves = graph.get_startup_waves().unwrap();

        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0], vec!["A"]);
        assert!(waves[1].contains(&"B".to_string()));
        assert!(waves[1].contains(&"C".to_string()));
        assert_eq!(waves[2], vec!["D"]);
    }

    #[test]
    fn test_cycle_detection() {
        // Create cycle: A -> B -> C -> A
        let specs = vec![
            create_test_spec("A", vec!["C".to_string()]),
            create_test_spec("B", vec!["A".to_string()]),
            create_test_spec("C", vec!["B".to_string()]),
        ];

        let result = DependencyGraph::from_specs(specs);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("Circular dependency"));
    }

    #[test]
    fn test_missing_dependency() {
        // Create spec with non-existent dependency
        let specs = vec![create_test_spec("A", vec!["NonExistent".to_string()])];

        let result = DependencyGraph::from_specs(specs);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn test_independent_components() {
        // Create components with no dependencies
        let specs = vec![
            create_test_spec("A", vec![]),
            create_test_spec("B", vec![]),
            create_test_spec("C", vec![]),
        ];

        let graph = DependencyGraph::from_specs(specs).unwrap();
        let waves = graph.get_startup_waves().unwrap();

        // All should start in the first wave
        assert_eq!(waves.len(), 1);
        assert_eq!(waves[0].len(), 3);
    }

    #[test]
    fn test_get_ready_components() {
        let specs = vec![
            create_test_spec("A", vec![]),
            create_test_spec("B", vec!["A".to_string()]),
            create_test_spec("C", vec!["B".to_string()]),
        ];

        let mut graph = DependencyGraph::from_specs(specs).unwrap();

        // Initially, only A should be ready
        let ready = graph.get_ready_components();
        assert_eq!(ready, vec!["A"]);

        // Mark A as healthy
        graph.mark_healthy("A").unwrap();

        // Now B should be ready
        let ready = graph.get_ready_components();
        assert_eq!(ready, vec!["B"]);

        // Mark B as healthy
        graph.mark_healthy("B").unwrap();

        // Now C should be ready
        let ready = graph.get_ready_components();
        assert_eq!(ready, vec!["C"]);
    }

    #[test]
    fn test_complex_graph() {
        // Create a more complex graph
        // database, redis (no deps)
        // backend (depends on database, redis)
        // frontend (depends on backend)
        // ingress (depends on backend, frontend)
        let specs = vec![
            create_test_spec("database", vec![]),
            create_test_spec("redis", vec![]),
            create_test_spec("backend", vec!["database".to_string(), "redis".to_string()]),
            create_test_spec("frontend", vec!["backend".to_string()]),
            create_test_spec(
                "ingress",
                vec!["backend".to_string(), "frontend".to_string()],
            ),
        ];

        let graph = DependencyGraph::from_specs(specs).unwrap();
        let waves = graph.get_startup_waves().unwrap();

        assert_eq!(waves.len(), 4);

        // First wave: database and redis
        assert_eq!(waves[0].len(), 2);
        assert!(waves[0].contains(&"database".to_string()));
        assert!(waves[0].contains(&"redis".to_string()));

        // Second wave: backend
        assert_eq!(waves[1], vec!["backend"]);

        // Third wave: frontend
        assert_eq!(waves[2], vec!["frontend"]);

        // Fourth wave: ingress
        assert_eq!(waves[3], vec!["ingress"]);
    }

    #[test]
    fn test_node_state_transitions() {
        let specs = vec![create_test_spec("A", vec![])];

        let mut graph = DependencyGraph::from_specs(specs).unwrap();

        // Initial state should be Pending
        assert_eq!(graph.get_node("A").unwrap().state, NodeState::Pending);

        // Mark as starting
        graph.mark_starting("A").unwrap();
        assert_eq!(graph.get_node("A").unwrap().state, NodeState::Starting);

        // Mark as waiting for health
        graph.mark_waiting_for_health("A").unwrap();
        assert_eq!(
            graph.get_node("A").unwrap().state,
            NodeState::WaitingForHealth
        );

        // Mark as healthy
        graph.mark_healthy("A").unwrap();
        assert_eq!(graph.get_node("A").unwrap().state, NodeState::Healthy);
        assert!(graph.get_node("A").unwrap().is_ready());
        assert!(graph.get_node("A").unwrap().is_terminal());
    }

    #[test]
    fn test_graph_stats() {
        let specs = vec![
            create_test_spec("A", vec![]),
            create_test_spec("B", vec!["A".to_string()]),
            create_test_spec("C", vec!["A".to_string()]),
            create_test_spec("D", vec!["B".to_string(), "C".to_string()]),
        ];

        let graph = DependencyGraph::from_specs(specs).unwrap();
        let stats = graph.stats();

        assert_eq!(stats.total_components, 4);
        assert_eq!(stats.components_with_deps, 3);
        assert_eq!(stats.total_dependencies, 4); // B->A, C->A, D->B, D->C
        assert_eq!(stats.max_dependencies, 2); // D has 2 dependencies
        assert_eq!(stats.wave_count, 3);
        assert_eq!(stats.max_wave_size, 2); // Wave 2 has B and C
    }
}
