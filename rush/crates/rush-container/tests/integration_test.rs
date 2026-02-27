//! Integration tests for dependency-aware container startup
//!
//! These tests verify that the container orchestration system correctly
//! handles dependencies, health checks, and startup ordering.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rush_build::{BuildType, ComponentBuildSpec, HealthCheckConfig, HealthCheckType};
    use rush_config::Config;
    use rush_container::dependency_graph::DependencyGraph;

    /// Create a test component with optional dependencies and health checks
    fn create_test_component(
        name: &str,
        port: u16,
        depends_on: Vec<String>,
        health_check: Option<HealthCheckConfig>,
    ) -> ComponentBuildSpec {
        let config = Config::test_default();
        let variables = rush_build::Variables::new(std::path::Path::new("."), "test");

        ComponentBuildSpec {
            build_type: BuildType::PureDockerImage {
                image_name_with_tag: format!("{name}:latest"),
                command: None,
                entrypoint: None,
            },
            product_name: "test-product".to_string(),
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
            priority: 0,
            watch: None,
            config,
            variables,
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: HashMap::new(),
            cross_compile: "native".to_string(),
            dotenv_secrets: HashMap::new(),
            domain: format!("{name}.test.local"),
            health_check,
            startup_probe: None,
        }
    }

    #[test]
    fn test_dependency_graph_creation() {
        // Create components with dependencies
        let components = vec![
            create_test_component("database", 5432, vec![], None),
            create_test_component("cache", 6379, vec![], None),
            create_test_component(
                "backend",
                8080,
                vec!["database".to_string(), "cache".to_string()],
                None,
            ),
            create_test_component("frontend", 3000, vec!["backend".to_string()], None),
            create_test_component(
                "ingress",
                80,
                vec!["frontend".to_string(), "backend".to_string()],
                None,
            ),
        ];

        // Build dependency graph
        let graph = DependencyGraph::from_specs(components).unwrap();

        // Get startup waves
        let waves = graph.get_startup_waves().unwrap();

        // Verify wave ordering
        assert_eq!(waves.len(), 4, "Should have 4 startup waves");

        // Wave 0: database and cache (no dependencies)
        assert!(waves[0].contains(&"database".to_string()));
        assert!(waves[0].contains(&"cache".to_string()));

        // Wave 1: backend (depends on database and cache)
        assert!(waves[1].contains(&"backend".to_string()));

        // Wave 2: frontend (depends on backend)
        assert!(waves[2].contains(&"frontend".to_string()));

        // Wave 3: ingress (depends on frontend and backend)
        assert!(waves[3].contains(&"ingress".to_string()));
    }

    #[test]
    fn test_cycle_detection() {
        // Create components with a cycle: A -> B -> C -> A
        let components = vec![
            create_test_component("A", 8001, vec!["C".to_string()], None),
            create_test_component("B", 8002, vec!["A".to_string()], None),
            create_test_component("C", 8003, vec!["B".to_string()], None),
        ];

        // Try to build dependency graph - should fail due to cycle
        let result = DependencyGraph::from_specs(components);

        assert!(result.is_err(), "Should detect dependency cycle");
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Circular dependency") || error_msg.contains("cycle"),
            "Error should mention cycle: {error_msg}"
        );
    }

    #[test]
    fn test_health_check_configuration() {
        // Create a component with HTTP health check
        let http_health = HealthCheckConfig {
            check_type: HealthCheckType::Http {
                path: "/health".to_string(),
                expected_status: 200,
            },
            initial_delay: 5,
            interval: 10,
            timeout: 5,
            success_threshold: 2,
            failure_threshold: 3,
            max_retries: 5,
        };

        let component = create_test_component("api", 8080, vec![], Some(http_health.clone()));

        assert!(component.health_check.is_some());
        let health = component.health_check.unwrap();
        assert_eq!(health.initial_delay, 5);
        assert_eq!(health.interval, 10);

        match health.check_type {
            HealthCheckType::Http {
                path,
                expected_status,
            } => {
                assert_eq!(path, "/health");
                assert_eq!(expected_status, 200);
            }
            _ => panic!("Expected HTTP health check"),
        }
    }

    #[test]
    fn test_startup_wave_parallelization() {
        // Create components that can start in parallel
        let components = vec![
            // Wave 0: All can start in parallel (no dependencies)
            create_test_component("db1", 5432, vec![], None),
            create_test_component("db2", 5433, vec![], None),
            create_test_component("cache1", 6379, vec![], None),
            create_test_component("cache2", 6380, vec![], None),
            // Wave 1: Services depend on wave 0
            create_test_component(
                "api1",
                8080,
                vec!["db1".to_string(), "cache1".to_string()],
                None,
            ),
            create_test_component(
                "api2",
                8081,
                vec!["db2".to_string(), "cache2".to_string()],
                None,
            ),
            // Wave 2: Frontend depends on APIs
            create_test_component(
                "frontend",
                3000,
                vec!["api1".to_string(), "api2".to_string()],
                None,
            ),
        ];

        let graph = DependencyGraph::from_specs(components).unwrap();
        let waves = graph.get_startup_waves().unwrap();

        assert_eq!(waves.len(), 3, "Should have 3 waves");
        assert_eq!(
            waves[0].len(),
            4,
            "Wave 0 should have 4 parallel components"
        );
        assert_eq!(
            waves[1].len(),
            2,
            "Wave 1 should have 2 parallel components"
        );
        assert_eq!(waves[2].len(), 1, "Wave 2 should have 1 component");
    }

    #[test]
    fn test_mixed_health_check_types() {
        // Test different health check types
        let tcp_health = HealthCheckConfig {
            check_type: HealthCheckType::Tcp { port: 5432 },
            initial_delay: 2,
            interval: 5,
            timeout: 3,
            success_threshold: 1,
            failure_threshold: 3,
            max_retries: 5,
        };

        let dns_health = HealthCheckConfig {
            check_type: HealthCheckType::Dns {
                hosts: vec!["database.local".to_string()],
            },
            initial_delay: 1,
            interval: 3,
            timeout: 2,
            success_threshold: 1,
            failure_threshold: 2,
            max_retries: 3,
        };

        let exec_health = HealthCheckConfig {
            check_type: HealthCheckType::Exec {
                command: vec!["pg_isready".to_string()],
            },
            initial_delay: 3,
            interval: 5,
            timeout: 3,
            success_threshold: 2,
            failure_threshold: 3,
            max_retries: 10,
        };

        let components = vec![
            create_test_component("postgres", 5432, vec![], Some(tcp_health)),
            create_test_component("dns-service", 53, vec![], Some(dns_health)),
            create_test_component("custom-service", 9000, vec![], Some(exec_health)),
        ];

        // Verify all components have health checks
        for component in components {
            assert!(component.health_check.is_some());
        }
    }
}
