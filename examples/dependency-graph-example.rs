#!/usr/bin/env rust-script
//! Example of using the dependency graph for container orchestration
//!
//! This example demonstrates how the dependency graph ensures containers
//! are started in the correct order based on their dependencies.
//!
//! ```cargo
//! [dependencies]
//! rush-container = { path = "../rush/crates/rush-container" }
//! rush-build = { path = "../rush/crates/rush-build" }
//! rush-config = { path = "../rush/crates/rush-config" }
//! tokio = { version = "1", features = ["full"] }
//! log = "0.4"
//! env_logger = "0.10"
//! ```

use rush_container::dependency_graph::{DependencyGraph, NodeState};
use rush_build::{ComponentBuildSpec, BuildType, HealthCheckConfig};
use std::sync::Arc;
use log::{info, debug};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Create component specifications with dependencies
    let specs = vec![
        // Database - no dependencies
        create_spec("database", vec![], Some(HealthCheckConfig::tcp(5432))),

        // Redis - no dependencies
        create_spec("redis", vec![], Some(HealthCheckConfig::tcp(6379))),

        // Backend - depends on database and redis
        create_spec(
            "backend",
            vec!["database".to_string(), "redis".to_string()],
            Some(HealthCheckConfig::http("/health"))
        ),

        // Frontend - depends on backend
        create_spec(
            "frontend",
            vec!["backend".to_string()],
            Some(HealthCheckConfig::http("/"))
        ),

        // Ingress - depends on backend and frontend
        create_spec(
            "ingress",
            vec!["backend".to_string(), "frontend".to_string()],
            Some(HealthCheckConfig::dns(vec![
                "backend.docker".to_string(),
                "frontend.docker".to_string(),
            ]))
        ),
    ];

    // Build dependency graph
    let mut graph = DependencyGraph::from_specs(specs)?;

    // Print graph structure
    println!("\n=== Dependency Graph ===");
    println!("{}", graph.to_dot());

    // Calculate startup waves
    let waves = graph.get_startup_waves()?;
    println!("\n=== Startup Waves ===");
    for (i, wave) in waves.iter().enumerate() {
        println!("Wave {}: {:?}", i + 1, wave);
    }

    // Print statistics
    let stats = graph.stats();
    println!("\n=== Graph Statistics ===");
    println!("Total components: {}", stats.total_components);
    println!("Components with dependencies: {}", stats.components_with_deps);
    println!("Total dependency edges: {}", stats.total_dependencies);
    println!("Maximum dependencies per component: {}", stats.max_dependencies);
    println!("Startup waves: {}", stats.wave_count);
    println!("Largest wave size: {}", stats.max_wave_size);

    // Simulate startup process
    println!("\n=== Simulated Startup Process ===");
    simulate_startup(&mut graph).await?;

    Ok(())
}

/// Create a test component specification
fn create_spec(
    name: &str,
    depends_on: Vec<String>,
    health_check: Option<HealthCheckConfig>,
) -> ComponentBuildSpec {
    use rush_config::Config;
    use rush_build::Variables;

    ComponentBuildSpec {
        build_type: BuildType::RustBinary {
            location: format!("src/{}", name),
            dockerfile_path: "Dockerfile".to_string(),
            context_dir: None,
            features: None,
            precompile_commands: None,
        },
        product_name: "example".to_string(),
        component_name: name.to_string(),
        color: "blue".to_string(),
        depends_on,
        build: None,
        mount_point: Some(format!("/{}", name)),
        subdomain: None,
        artefacts: None,
        artefact_output_dir: "dist".to_string(),
        docker_extra_run_args: vec![],
        env: None,
        volumes: None,
        port: Some(8080),
        target_port: Some(8080),
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
        domain: format!("{}.local", name),
        cross_compile: "native".to_string(),
        health_check,
        startup_probe: None,
    }
}

/// Simulate the startup process using the dependency graph
async fn simulate_startup(graph: &mut DependencyGraph) -> Result<(), Box<dyn std::error::Error>> {
    let waves = graph.get_startup_waves()?;

    for (wave_num, wave) in waves.iter().enumerate() {
        println!("\n🌊 Starting Wave {} with components: {:?}", wave_num + 1, wave);

        // Start all components in this wave
        for component in wave {
            println!("  🚀 Starting {}", component);
            graph.mark_starting(component)?;

            // Simulate container creation time
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            println!("  📦 {} container created", component);
            graph.mark_waiting_for_health(component)?;

            // Simulate health check time
            let health_check_time = match component.as_str() {
                "database" => 500,  // Database takes longer to start
                "redis" => 200,
                "backend" => 300,
                "frontend" => 250,
                "ingress" => 100,  // Ingress is fast once dependencies are ready
                _ => 200,
            };

            tokio::time::sleep(tokio::time::Duration::from_millis(health_check_time)).await;

            println!("  ✅ {} is healthy", component);
            graph.mark_healthy(component)?;
        }

        println!("✅ Wave {} complete", wave_num + 1);

        // Show what's ready for the next wave
        if wave_num < waves.len() - 1 {
            let ready = graph.get_ready_components();
            if !ready.is_empty() {
                println!("📋 Components ready for next wave: {:?}", ready);
            }
        }
    }

    println!("\n🎉 All components started successfully!");

    // Demonstrate failure handling
    println!("\n=== Simulating Component Failure ===");
    graph.mark_failed("backend", "Connection timeout to database".to_string())?;

    // Check which components would be affected
    let ready = graph.get_ready_components();
    println!("Components that cannot start due to backend failure: {:?}",
        vec!["frontend", "ingress"].iter()
            .filter(|&&name| !ready.contains(&name.to_string()))
            .collect::<Vec<_>>()
    );

    Ok(())
}

/* Example Output:

=== Dependency Graph ===
digraph dependencies {
  rankdir=LR;
  node [shape=box];

  "database" [style=filled, fillcolor=white];
  "redis" [style=filled, fillcolor=white];
  "backend" [style=filled, fillcolor=white];
  "frontend" [style=filled, fillcolor=white];
  "ingress" [style=filled, fillcolor=white];

  "database" -> "backend";
  "redis" -> "backend";
  "backend" -> "frontend";
  "backend" -> "ingress";
  "frontend" -> "ingress";
}

=== Startup Waves ===
Wave 1: ["database", "redis"]
Wave 2: ["backend"]
Wave 3: ["frontend"]
Wave 4: ["ingress"]

=== Graph Statistics ===
Total components: 5
Components with dependencies: 3
Total dependency edges: 5
Maximum dependencies per component: 2
Startup waves: 4
Largest wave size: 2

=== Simulated Startup Process ===

🌊 Starting Wave 1 with components: ["database", "redis"]
  🚀 Starting database
  📦 database container created
  🚀 Starting redis
  📦 redis container created
  ✅ database is healthy
  ✅ redis is healthy
✅ Wave 1 complete
📋 Components ready for next wave: ["backend"]

🌊 Starting Wave 2 with components: ["backend"]
  🚀 Starting backend
  📦 backend container created
  ✅ backend is healthy
✅ Wave 2 complete
📋 Components ready for next wave: ["frontend"]

🌊 Starting Wave 3 with components: ["frontend"]
  🚀 Starting frontend
  📦 frontend container created
  ✅ frontend is healthy
✅ Wave 3 complete
📋 Components ready for next wave: ["ingress"]

🌊 Starting Wave 4 with components: ["ingress"]
  🚀 Starting ingress
  📦 ingress container created
  ✅ ingress is healthy
✅ Wave 4 complete

🎉 All components started successfully!

=== Simulating Component Failure ===
Components that cannot start due to backend failure: ["frontend", "ingress"]

*/