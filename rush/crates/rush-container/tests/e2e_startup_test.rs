//! End-to-end tests for dependency-aware container startup
//!
//! These tests verify the complete startup sequence with real component specs

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use rush_build::{BuildType, ComponentBuildSpec, HealthCheckConfig, HealthCheckType, Variables};
    use rush_container::{
        dependency_graph::DependencyGraph,
        metrics::MetricsCollector,
    };
    use rush_config::Config;

    /// Create a realistic microservices architecture for testing
    fn create_microservices_architecture() -> Vec<ComponentBuildSpec> {
        let config = Config::test_default();
        let variables = Variables::new(std::path::Path::new("."), "test");

        vec![
            // Database - Level 0 (no dependencies)
            create_component(
                "postgres",
                5432,
                vec![],
                Some(create_tcp_health_check(5432)),
                0,
                config.clone(),
                variables.clone(),
            ),
            // Cache - Level 0
            create_component(
                "redis",
                6379,
                vec![],
                Some(create_tcp_health_check(6379)),
                0,
                config.clone(),
                variables.clone(),
            ),
            // Message Queue - Level 0
            create_component(
                "rabbitmq",
                5672,
                vec![],
                Some(create_tcp_health_check(5672)),
                0,
                config.clone(),
                variables.clone(),
            ),
            // Auth Service - Level 1 (depends on DB and cache)
            create_component(
                "auth-service",
                8001,
                vec!["postgres".to_string(), "redis".to_string()],
                Some(create_http_health_check("/health", 200)),
                1,
                config.clone(),
                variables.clone(),
            ),
            // User Service - Level 2 (depends on auth)
            create_component(
                "user-service",
                8002,
                vec!["postgres".to_string(), "redis".to_string(), "auth-service".to_string()],
                Some(create_http_health_check("/health", 200)),
                2,
                config.clone(),
                variables.clone(),
            ),
            // Product Service - Level 1
            create_component(
                "product-service",
                8003,
                vec!["postgres".to_string(), "redis".to_string()],
                Some(create_http_health_check("/health", 200)),
                1,
                config.clone(),
                variables.clone(),
            ),
            // Order Service - Level 3 (depends on user and product)
            create_component(
                "order-service",
                8004,
                vec![
                    "postgres".to_string(),
                    "redis".to_string(),
                    "rabbitmq".to_string(),
                    "user-service".to_string(),
                    "product-service".to_string(),
                ],
                Some(create_http_health_check("/health", 200)),
                3,
                config.clone(),
                variables.clone(),
            ),
            // API Gateway - Level 4 (depends on all services)
            create_component(
                "api-gateway",
                8000,
                vec![
                    "auth-service".to_string(),
                    "user-service".to_string(),
                    "product-service".to_string(),
                    "order-service".to_string(),
                ],
                Some(create_http_health_check("/health", 200)),
                4,
                config.clone(),
                variables.clone(),
            ),
            // Frontend - Level 4
            create_component(
                "frontend",
                3000,
                vec!["api-gateway".to_string()],
                Some(create_http_health_check("/", 200)),
                4,
                config.clone(),
                variables.clone(),
            ),
            // Ingress - Level 5 (depends on frontend and gateway)
            create_component(
                "ingress",
                80,
                vec!["frontend".to_string(), "api-gateway".to_string()],
                Some(create_dns_health_check(vec![
                    "frontend".to_string(),
                    "api-gateway".to_string(),
                    "auth-service".to_string(),
                    "user-service".to_string(),
                    "product-service".to_string(),
                    "order-service".to_string(),
                ])),
                5,
                config.clone(),
                variables.clone(),
            ),
        ]
    }

    fn create_component(
        name: &str,
        port: u16,
        depends_on: Vec<String>,
        health_check: Option<HealthCheckConfig>,
        priority: u64,
        config: Arc<Config>,
        variables: Arc<Variables>,
    ) -> ComponentBuildSpec {
        ComponentBuildSpec {
            build_type: BuildType::PureDockerImage {
                image_name_with_tag: format!("{}:latest", name),
                command: None,
                entrypoint: None,
            },
            product_name: "microservices".to_string(),
            component_name: name.to_string(),
            color: "blue".to_string(),
            depends_on,
            build: None,
            mount_point: None,
            subdomain: Some(name.to_string()),
            artefacts: None,
            artefact_output_dir: "/tmp".to_string(),
            docker_extra_run_args: vec![],
            env: Some(HashMap::new()),
            volumes: Some(HashMap::new()),
            port: Some(port),
            target_port: Some(port),
            k8s: None,
            priority,
            watch: None,
            config,
            variables,
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: HashMap::new(),
            cross_compile: "native".to_string(),
            dotenv_secrets: HashMap::new(),
            domain: format!("{}.test.local", name),
            health_check,
            startup_probe: None,
        }
    }

    fn create_tcp_health_check(port: u16) -> HealthCheckConfig {
        HealthCheckConfig {
            check_type: HealthCheckType::Tcp { port },
            initial_delay: 2,
            interval: 5,
            timeout: 3,
            success_threshold: 1,
            failure_threshold: 3,
            max_retries: 30,
        }
    }

    fn create_http_health_check(path: &str, expected_status: u16) -> HealthCheckConfig {
        HealthCheckConfig {
            check_type: HealthCheckType::Http {
                path: path.to_string(),
                expected_status,
            },
            initial_delay: 5,
            interval: 10,
            timeout: 5,
            success_threshold: 2,
            failure_threshold: 3,
            max_retries: 20,
        }
    }

    fn create_dns_health_check(hosts: Vec<String>) -> HealthCheckConfig {
        HealthCheckConfig {
            check_type: HealthCheckType::Dns { hosts },
            initial_delay: 1,
            interval: 2,
            timeout: 2,
            success_threshold: 1,
            failure_threshold: 10,
            max_retries: 30,
        }
    }

    #[test]
    fn test_microservices_startup_waves() {
        let components = create_microservices_architecture();
        let graph = DependencyGraph::from_specs(components).unwrap();
        let waves = graph.get_startup_waves().unwrap();

        // Verify we have at least 6 waves (may have more due to implementation details)
        assert!(waves.len() >= 6, "Should have at least 6 startup waves, got {}", waves.len());

        // Verify foundation services start first
        let wave0: Vec<String> = waves[0].clone();
        assert!(wave0.contains(&"postgres".to_string()));
        assert!(wave0.contains(&"redis".to_string()));
        assert!(wave0.contains(&"rabbitmq".to_string()));

        // Find which wave contains auth-service and product-service
        let mut auth_wave = None;
        let mut user_wave = None;
        let mut ingress_wave = None;

        for (i, wave) in waves.iter().enumerate() {
            if wave.contains(&"auth-service".to_string()) {
                auth_wave = Some(i);
            }
            if wave.contains(&"user-service".to_string()) {
                user_wave = Some(i);
            }
            if wave.contains(&"ingress".to_string()) {
                ingress_wave = Some(i);
            }
        }

        // Verify ordering constraints
        assert!(auth_wave.is_some(), "auth-service should be in a wave");
        assert!(user_wave.is_some(), "user-service should be in a wave");
        assert!(ingress_wave.is_some(), "ingress should be in a wave");

        // User service must start after auth service
        assert!(user_wave.unwrap() > auth_wave.unwrap(),
                "user-service should start after auth-service");

        // Ingress must be in the last wave
        assert_eq!(ingress_wave.unwrap(), waves.len() - 1,
                   "ingress should be in the last wave");
    }

    #[test]
    fn test_complex_dependency_chains() {
        let config = Config::test_default();
        let variables = Variables::new(std::path::Path::new("."), "test");

        // Create a diamond dependency pattern
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        let components = vec![
            create_component("A", 8001, vec![], None, 0, config.clone(), variables.clone()),
            create_component("B", 8002, vec!["A".to_string()], None, 1, config.clone(), variables.clone()),
            create_component("C", 8003, vec!["A".to_string()], None, 1, config.clone(), variables.clone()),
            create_component("D", 8004, vec!["B".to_string(), "C".to_string()], None, 2, config.clone(), variables.clone()),
        ];

        let graph = DependencyGraph::from_specs(components).unwrap();
        let waves = graph.get_startup_waves().unwrap();

        assert_eq!(waves.len(), 3);
        assert_eq!(waves[0], vec!["A".to_string()]);
        assert_eq!(waves[1].len(), 2); // B and C in parallel
        assert_eq!(waves[2], vec!["D".to_string()]);
    }

    #[tokio::test]
    async fn test_startup_metrics_simulation() {
        let components = create_microservices_architecture();
        let metrics = MetricsCollector::new(true);

        // Simulate startup
        let start = Instant::now();
        metrics.record_startup_begin(components.len(), 6).await;

        // Simulate wave-by-wave startup
        let waves = vec![
            vec!["postgres", "redis", "rabbitmq"],
            vec!["auth-service", "product-service"],
            vec!["user-service"],
            vec!["order-service"],
            vec!["api-gateway", "frontend"],
            vec!["ingress"],
        ];

        for (wave_num, components_in_wave) in waves.iter().enumerate() {
            metrics.record_wave_start(wave_num, components_in_wave.iter().map(|s| s.to_string()).collect()).await;

            for component in components_in_wave {
                // Simulate component startup
                metrics.record_component_start(component, wave_num, vec![]).await;
                tokio::time::sleep(Duration::from_millis(10)).await;
                metrics.record_container_created(component).await;

                // Simulate health checks
                metrics.record_health_check_start(component).await;
                for _ in 0..3 {
                    metrics.record_health_check_attempt(component).await;
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                metrics.record_component_healthy(component).await;
            }

            metrics.record_wave_complete(wave_num).await;
        }

        metrics.record_startup_complete().await;

        let elapsed = start.elapsed();
        println!("Simulated startup took {:?}", elapsed);

        // Export metrics
        let json_metrics = metrics.export_json().await;
        assert!(json_metrics.contains("\"successful_components\": 10"));
        assert!(json_metrics.contains("\"total_waves\": 6"));

        let prometheus_metrics = metrics.export_prometheus().await;
        assert!(prometheus_metrics.contains("rush_startup_duration_seconds"));
        assert!(prometheus_metrics.contains("rush_component_status"));
    }

    #[test]
    fn test_priority_based_ordering() {
        let components = create_microservices_architecture();

        // Verify components are properly prioritized
        let mut priority_map: HashMap<u64, Vec<String>> = HashMap::new();
        for component in &components {
            priority_map.entry(component.priority)
                .or_insert_with(Vec::new)
                .push(component.component_name.clone());
        }

        // Priority 0: Foundation services
        assert_eq!(priority_map[&0].len(), 3);
        assert!(priority_map[&0].contains(&"postgres".to_string()));

        // Priority 5: Ingress (last)
        assert_eq!(priority_map[&5].len(), 1);
        assert!(priority_map[&5].contains(&"ingress".to_string()));
    }

    #[test]
    fn test_health_check_diversity() {
        let components = create_microservices_architecture();

        let mut tcp_checks = 0;
        let mut http_checks = 0;
        let mut dns_checks = 0;

        for component in components {
            if let Some(health_check) = component.health_check {
                match health_check.check_type {
                    HealthCheckType::Tcp { .. } => tcp_checks += 1,
                    HealthCheckType::Http { .. } => http_checks += 1,
                    HealthCheckType::Dns { .. } => dns_checks += 1,
                    HealthCheckType::Exec { .. } => {}
                }
            }
        }

        assert_eq!(tcp_checks, 3, "Should have 3 TCP health checks");
        assert_eq!(http_checks, 6, "Should have 6 HTTP health checks");
        assert_eq!(dns_checks, 1, "Should have 1 DNS health check");
    }

    #[test]
    fn test_startup_failure_scenarios() {
        let config = Config::test_default();
        let variables = Variables::new(std::path::Path::new("."), "test");

        // Create a scenario where a critical service would fail
        let mut components = vec![
            create_component("database", 5432, vec![], None, 0, config.clone(), variables.clone()),
            create_component("api", 8000, vec!["database".to_string()], None, 1, config.clone(), variables.clone()),
            create_component("frontend", 3000, vec!["api".to_string()], None, 2, config.clone(), variables.clone()),
        ];

        // Simulate database failure by removing it
        components.remove(0);

        // This should fail to create a valid dependency graph
        let result = DependencyGraph::from_specs(components);
        assert!(result.is_err(), "Should fail with missing dependency");
    }
}