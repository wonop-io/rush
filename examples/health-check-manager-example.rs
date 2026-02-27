#!/usr/bin/env rust-script
//! Example of using the health check manager with dependency graph
//!
//! This example demonstrates how health checks work with the dependency
//! graph to ensure containers are started in the correct order and are
//! actually ready before dependent services start.
//!
//! ```cargo
//! [dependencies]
//! rush-container = { path = "../rush/crates/rush-container" }
//! rush-build = { path = "../rush/crates/rush-build" }
//! rush-docker = { path = "../rush/crates/rush-docker" }
//! rush-core = { path = "../rush/crates/rush-core" }
//! tokio = { version = "1", features = ["full"] }
//! log = "0.4"
//! env_logger = "0.10"
//! ```

use rush_container::{
    dependency_graph::DependencyGraph,
    health_check_manager::HealthCheckManager,
    docker::DockerCliClient,
};
use rush_build::{ComponentBuildSpec, BuildType, HealthCheckConfig};
use std::sync::Arc;
use log::{info, error};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    // Create Docker client
    let docker_client = Arc::new(DockerCliClient::new("/usr/bin/docker".to_string()));

    // Create health check manager
    let health_manager = HealthCheckManager::new(docker_client.clone());

    // Example: Simulating a typical application stack with health checks
    println!("\n=== Container Orchestration with Health Checks ===\n");

    // Create component specifications with health checks
    let specs = vec![
        // Database - TCP health check
        create_spec(
            "database",
            vec![],
            Some(HealthCheckConfig::tcp(5432)
                .with_initial_delay(2)
                .with_interval(3)
                .with_max_retries(20))
        ),

        // Redis - TCP health check
        create_spec(
            "redis",
            vec![],
            Some(HealthCheckConfig::tcp(6379)
                .with_initial_delay(1)
                .with_interval(2)
                .with_max_retries(15))
        ),

        // Backend - HTTP health check, depends on database and redis
        create_spec(
            "backend",
            vec!["database".to_string(), "redis".to_string()],
            Some(HealthCheckConfig::http("/health")
                .with_initial_delay(3)
                .with_interval(5)
                .with_max_retries(30))
        ),

        // Frontend - HTTP health check, depends on backend
        create_spec(
            "frontend",
            vec!["backend".to_string()],
            Some(HealthCheckConfig::http("/")
                .with_initial_delay(5)
                .with_interval(5)
                .with_max_retries(25))
        ),

        // Ingress - DNS health check, depends on backend and frontend
        create_spec(
            "ingress",
            vec!["backend".to_string(), "frontend".to_string()],
            Some(HealthCheckConfig::dns(vec![
                "backend.docker".to_string(),
                "frontend.docker".to_string(),
            ])
            .with_initial_delay(2)
            .with_interval(1)
            .with_success_threshold(1)
            .with_max_retries(60))  // Give DNS time to propagate
        ),
    ];

    // Build dependency graph
    let mut graph = DependencyGraph::from_specs(specs.clone())?;

    // Get startup waves
    let waves = graph.get_startup_waves()?;

    println!("📋 Startup Plan:");
    for (i, wave) in waves.iter().enumerate() {
        println!("  Wave {}: {:?}", i + 1, wave);
    }
    println!();

    // Simulate container startup with health checks
    let overall_start = Instant::now();

    for (wave_num, wave) in waves.iter().enumerate() {
        println!("🌊 Starting Wave {}/{}", wave_num + 1, waves.len());

        // Start all components in this wave
        let wave_tasks = wave.iter().map(|component_name| {
            let component_spec = specs.iter()
                .find(|s| &s.component_name == component_name)
                .cloned()
                .unwrap();

            let health_manager = health_manager.clone();
            let mut graph = graph.clone();
            let name = component_name.clone();

            tokio::spawn(async move {
                start_component_with_health(
                    &name,
                    component_spec,
                    health_manager,
                    &mut graph
                ).await
            })
        });

        // Wait for all components in wave to be healthy
        let wave_results = futures::future::join_all(wave_tasks).await;

        for result in wave_results {
            match result {
                Ok(Ok((name, duration))) => {
                    println!("  ✅ {} healthy in {:?}", name, duration);
                    graph.mark_healthy(&name)?;
                }
                Ok(Err(e)) => {
                    error!("  ❌ Component failed: {}", e);
                    return Err(e.into());
                }
                Err(e) => {
                    error!("  ❌ Task panic: {}", e);
                    return Err(e.into());
                }
            }
        }

        println!("✅ Wave {} complete\n", wave_num + 1);
    }

    let total_time = overall_start.elapsed();
    println!("🎉 All components started successfully in {:?}!", total_time);

    // Demonstrate health check failure handling
    println!("\n=== Health Check Failure Scenario ===\n");
    simulate_health_check_failure().await;

    Ok(())
}

/// Simulate starting a component with health checks
async fn start_component_with_health(
    name: &str,
    spec: ComponentBuildSpec,
    health_manager: HealthCheckManager,
    graph: &mut DependencyGraph,
) -> Result<(String, std::time::Duration), Box<dyn std::error::Error>> {
    let start = Instant::now();

    info!("  🚀 Starting {}", name);
    graph.mark_starting(name)?;

    // Simulate container creation
    let container_id = format!("{}-container-{}", name, uuid::Uuid::new_v4());
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    info!("  📦 {} container created ({})", name, &container_id[..20]);
    graph.mark_waiting_for_health(name)?;

    // Perform health check if configured
    if let Some(health_config) = &spec.health_check {
        // In a real scenario, this would check the actual container
        // For demo, we'll simulate with timing
        simulate_health_check(name, health_config).await?;
    } else {
        info!("  ⚠️  {} has no health check configured", name);
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    let duration = start.elapsed();
    Ok((name.to_string(), duration))
}

/// Simulate a health check (in real use, would check actual container)
async fn simulate_health_check(
    name: &str,
    config: &HealthCheckConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use rush_build::HealthCheckType;

    let check_type = match &config.check_type {
        HealthCheckType::Http { path, .. } => format!("HTTP {}", path),
        HealthCheckType::Tcp { port } => format!("TCP port {}", port),
        HealthCheckType::Dns { hosts } => format!("DNS for {:?}", hosts),
        HealthCheckType::Exec { command } => format!("Exec {:?}", command),
    };

    info!("  🏥 Performing {} health check for {}", check_type, name);

    // Simulate check time based on component
    let check_duration = match name {
        "database" => 800,  // Databases take time to initialize
        "redis" => 300,
        "backend" => 500,
        "frontend" => 400,
        "ingress" => 200,   // Fast once dependencies are ready
        _ => 300,
    };

    tokio::time::sleep(tokio::time::Duration::from_millis(check_duration)).await;

    Ok(())
}

/// Demonstrate health check failure scenario
async fn simulate_health_check_failure() {
    println!("Simulating a component with failing health check...\n");

    let failing_config = HealthCheckConfig::http("/health")
        .with_initial_delay(0)
        .with_interval(1)
        .with_failure_threshold(3)
        .with_max_retries(5);

    println!("Configuration:");
    println!("  - Check type: HTTP /health");
    println!("  - Max retries: 5");
    println!("  - Failure threshold: 3");
    println!("  - Interval: 1s\n");

    // Simulate failures
    for attempt in 1..=5 {
        println!("  Attempt {}/5: ❌ Connection refused", attempt);
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    println!("\n❌ Health check failed after 5 attempts");
    println!("   Component marked as failed");
    println!("   Dependent components will not start");
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

/* Example Output:

=== Container Orchestration with Health Checks ===

📋 Startup Plan:
  Wave 1: ["database", "redis"]
  Wave 2: ["backend"]
  Wave 3: ["frontend"]
  Wave 4: ["ingress"]

🌊 Starting Wave 1/4
  🚀 Starting database
  🚀 Starting redis
  📦 database container created (database-container-12)
  📦 redis container created (redis-container-34)
  🏥 Performing TCP port 5432 health check for database
  🏥 Performing TCP port 6379 health check for redis
  ✅ redis healthy in 500ms
  ✅ database healthy in 1s
✅ Wave 1 complete

🌊 Starting Wave 2/4
  🚀 Starting backend
  📦 backend container created (backend-container-56)
  🏥 Performing HTTP /health health check for backend
  ✅ backend healthy in 700ms
✅ Wave 2 complete

🌊 Starting Wave 3/4
  🚀 Starting frontend
  📦 frontend container created (frontend-container-78)
  🏥 Performing HTTP / health check for frontend
  ✅ frontend healthy in 600ms
✅ Wave 3 complete

🌊 Starting Wave 4/4
  🚀 Starting ingress
  📦 ingress container created (ingress-container-90)
  🏥 Performing DNS health check for ingress
  ✅ ingress healthy in 400ms
✅ Wave 4 complete

🎉 All components started successfully in 3.2s!

=== Health Check Failure Scenario ===

Simulating a component with failing health check...

Configuration:
  - Check type: HTTP /health
  - Max retries: 5
  - Failure threshold: 3
  - Interval: 1s

  Attempt 1/5: ❌ Connection refused
  Attempt 2/5: ❌ Connection refused
  Attempt 3/5: ❌ Connection refused
  Attempt 4/5: ❌ Connection refused
  Attempt 5/5: ❌ Connection refused

❌ Health check failed after 5 attempts
   Component marked as failed
   Dependent components will not start

*/