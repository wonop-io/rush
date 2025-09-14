#[cfg(test)]
mod tests {
    use rush_local_services::types::LocalServiceType;
    use rush_local_services::config::{LocalServiceConfig, ServiceDefaults};

    #[test]
    fn test_prometheus_defaults() {
        let config = ServiceDefaults::prometheus("test-prometheus".to_string());

        assert_eq!(config.service_type, LocalServiceType::Prometheus);
        assert!(!config.ports.is_empty());
        assert_eq!(config.ports[0].host_port, 9090);
        assert_eq!(config.ports[0].container_port, 9090);
        assert!(config.command.is_some());
        assert!(config.get_health_check().is_some());
        assert_eq!(
            config.get_health_check().unwrap(),
            "curl -f http://localhost:9090/-/healthy"
        );
    }

    #[test]
    fn test_grafana_defaults() {
        let config = ServiceDefaults::grafana("test-grafana".to_string());

        assert_eq!(config.service_type, LocalServiceType::Grafana);
        assert!(!config.ports.is_empty());
        assert_eq!(config.ports[0].host_port, 3000);
        assert_eq!(config.ports[0].container_port, 3000);
        assert!(config.env.contains_key("GF_SECURITY_ADMIN_USER"));
        assert_eq!(config.env.get("GF_SECURITY_ADMIN_USER").unwrap(), "admin");
        assert!(config.env.contains_key("GF_SECURITY_ADMIN_PASSWORD"));
        assert_eq!(config.env.get("GF_SECURITY_ADMIN_PASSWORD").unwrap(), "admin");
        assert_eq!(
            config.get_health_check().unwrap(),
            "curl -f http://localhost:3000/api/health"
        );
    }

    #[test]
    fn test_tempo_defaults() {
        let config = ServiceDefaults::tempo("test-tempo".to_string());

        assert_eq!(config.service_type, LocalServiceType::Tempo);
        assert_eq!(config.ports.len(), 5); // Main port + 4 protocol ports

        // Check main API port
        assert!(config.ports.iter().any(|p| p.host_port == 3200 && p.container_port == 3200));

        // Check protocol ports
        assert!(config.ports.iter().any(|p| p.host_port == 4317 && p.container_port == 4317)); // OTLP gRPC
        assert!(config.ports.iter().any(|p| p.host_port == 4318 && p.container_port == 4318)); // OTLP HTTP
        assert!(config.ports.iter().any(|p| p.host_port == 14268 && p.container_port == 14268)); // Jaeger
        assert!(config.ports.iter().any(|p| p.host_port == 9411 && p.container_port == 9411)); // Zipkin

        assert!(config.command.is_some());
        assert_eq!(
            config.get_health_check().unwrap(),
            "curl -f http://localhost:3200/ready"
        );
    }

    #[test]
    fn test_observability_stack_dependencies() {
        let stack = ServiceDefaults::observability_stack();

        assert_eq!(stack.len(), 3);

        // Find each service in the stack
        let prometheus = stack.iter().find(|s| s.service_type == LocalServiceType::Prometheus);
        let grafana = stack.iter().find(|s| s.service_type == LocalServiceType::Grafana);
        let tempo = stack.iter().find(|s| s.service_type == LocalServiceType::Tempo);

        assert!(prometheus.is_some());
        assert!(grafana.is_some());
        assert!(tempo.is_some());

        // Check Grafana dependencies
        let grafana = grafana.unwrap();
        assert_eq!(grafana.depends_on.len(), 2);
        assert!(grafana.depends_on.contains(&"prometheus".to_string()));
        assert!(grafana.depends_on.contains(&"tempo".to_string()));
    }

    #[test]
    fn test_service_type_parsing() {
        assert_eq!(LocalServiceType::parse("prometheus"), LocalServiceType::Prometheus);
        assert_eq!(LocalServiceType::parse("prom"), LocalServiceType::Prometheus);
        assert_eq!(LocalServiceType::parse("PROMETHEUS"), LocalServiceType::Prometheus);
        assert_eq!(LocalServiceType::parse("grafana"), LocalServiceType::Grafana);
        assert_eq!(LocalServiceType::parse("GRAFANA"), LocalServiceType::Grafana);
        assert_eq!(LocalServiceType::parse("tempo"), LocalServiceType::Tempo);
        assert_eq!(LocalServiceType::parse("TEMPO"), LocalServiceType::Tempo);
    }

    #[test]
    fn test_default_images() {
        assert_eq!(
            LocalServiceType::Prometheus.default_image(),
            "prom/prometheus:latest"
        );
        assert_eq!(
            LocalServiceType::Grafana.default_image(),
            "grafana/grafana:latest"
        );
        assert_eq!(
            LocalServiceType::Tempo.default_image(),
            "grafana/tempo:latest"
        );
    }

    #[test]
    fn test_env_var_suffixes() {
        assert_eq!(LocalServiceType::Prometheus.env_var_suffix(), "PROMETHEUS");
        assert_eq!(LocalServiceType::Grafana.env_var_suffix(), "GRAFANA");
        assert_eq!(LocalServiceType::Tempo.env_var_suffix(), "TEMPO");
    }
}