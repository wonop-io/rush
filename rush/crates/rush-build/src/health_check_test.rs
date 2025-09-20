//! Tests for health check configuration parsing

#[cfg(test)]
mod tests {
    use crate::health_check::{HealthCheckConfig, HealthCheckType, parse_health_check};
    use crate::spec::ComponentBuildSpec;
    use serde_yaml;

    #[test]
    fn test_parse_http_health_check_from_yaml() {
        let yaml_str = r#"
        type: http
        path: /health
        expected_status: 200
        initial_delay: 5
        interval: 10
        success_threshold: 2
        failure_threshold: 3
        timeout: 5
        max_retries: 30
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = parse_health_check(&value).expect("Should parse HTTP health check");

        match config.check_type {
            HealthCheckType::Http { path, expected_status } => {
                assert_eq!(path, "/health");
                assert_eq!(expected_status, 200);
            }
            _ => panic!("Expected HTTP health check type"),
        }

        assert_eq!(config.initial_delay, 5);
        assert_eq!(config.interval, 10);
        assert_eq!(config.success_threshold, 2);
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.timeout, 5);
        assert_eq!(config.max_retries, 30);
    }

    #[test]
    fn test_parse_tcp_health_check_from_yaml() {
        let yaml_str = r#"
        type: tcp
        port: 8080
        initial_delay: 3
        interval: 5
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = parse_health_check(&value).expect("Should parse TCP health check");

        match config.check_type {
            HealthCheckType::Tcp { port } => {
                assert_eq!(port, 8080);
            }
            _ => panic!("Expected TCP health check type"),
        }

        assert_eq!(config.initial_delay, 3);
        assert_eq!(config.interval, 5);
        // Check defaults
        assert_eq!(config.success_threshold, 1);
        assert_eq!(config.failure_threshold, 3);
    }

    #[test]
    fn test_parse_dns_health_check_from_yaml() {
        let yaml_str = r#"
        type: dns
        hosts:
          - backend.docker
          - frontend.docker
        initial_delay: 2
        interval: 1
        success_threshold: 1
        failure_threshold: 5
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = parse_health_check(&value).expect("Should parse DNS health check");

        match config.check_type {
            HealthCheckType::Dns { hosts } => {
                assert_eq!(hosts.len(), 2);
                assert_eq!(hosts[0], "backend.docker");
                assert_eq!(hosts[1], "frontend.docker");
            }
            _ => panic!("Expected DNS health check type"),
        }

        assert_eq!(config.initial_delay, 2);
        assert_eq!(config.interval, 1);
        assert_eq!(config.success_threshold, 1);
        assert_eq!(config.failure_threshold, 5);
    }

    #[test]
    fn test_parse_exec_health_check_from_yaml() {
        let yaml_str = r#"
        type: exec
        command:
          - pg_isready
          - -U
          - postgres
        initial_delay: 2
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = parse_health_check(&value).expect("Should parse exec health check");

        match config.check_type {
            HealthCheckType::Exec { command } => {
                assert_eq!(command.len(), 3);
                assert_eq!(command[0], "pg_isready");
                assert_eq!(command[1], "-U");
                assert_eq!(command[2], "postgres");
            }
            _ => panic!("Expected exec health check type"),
        }

        assert_eq!(config.initial_delay, 2);
    }

    #[test]
    fn test_component_spec_with_health_check() {
        let yaml_str = r#"
        backend:
          build_type: "RustBinary"
          location: "backend/server"
          dockerfile: "backend/Dockerfile"
          component_name: "backend"
          port: 8080
          health_check:
            type: tcp
            port: 8080
            initial_delay: 3
            interval: 5
          startup_probe:
            type: http
            path: /ready
            expected_status: 200
            initial_delay: 10
            max_retries: 60
        "#;

        let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let backend = yaml.get("backend").unwrap();

        // This would normally be done by ComponentBuildSpec::parse_from_yaml
        // but we're testing that the fields are parsed correctly
        let health_check = backend.get("health_check")
            .and_then(|v| parse_health_check(v));
        let startup_probe = backend.get("startup_probe")
            .and_then(|v| parse_health_check(v));

        assert!(health_check.is_some());
        assert!(startup_probe.is_some());

        let hc = health_check.unwrap();
        match hc.check_type {
            HealthCheckType::Tcp { port } => assert_eq!(port, 8080),
            _ => panic!("Expected TCP health check"),
        }

        let sp = startup_probe.unwrap();
        match sp.check_type {
            HealthCheckType::Http { path, .. } => assert_eq!(path, "/ready"),
            _ => panic!("Expected HTTP startup probe"),
        }
        assert_eq!(sp.max_retries, 60);
    }

    #[test]
    fn test_missing_health_check_returns_none() {
        let yaml_str = r#"
        component_name: test
        "#;

        let value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = parse_health_check(&value);

        assert!(config.is_none());
    }
}