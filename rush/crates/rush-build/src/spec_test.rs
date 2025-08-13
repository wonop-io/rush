#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml;
    use std::sync::Arc;
    use rush_config::Config;
    use crate::{BuildType, ComponentBuildSpec, Variables};

    fn create_test_config() -> Arc<Config> {
        Config::test_default()
    }

    fn create_test_variables() -> Arc<Variables> {
        Arc::new(Variables::new())
    }

    #[test]
    fn test_parse_local_service_flat_structure() {
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "postgres"
        service_type: "postgresql"
        version: "15"
        persist_data: true
        env:
          POSTGRES_USER: "testuser"
          POSTGRES_PASSWORD: "testpass"
          POSTGRES_DB: "testdb"
          POSTGRES_PORT: "5432"
        health_check: "pg_isready -U testuser -p 5432"
        init_scripts:
          - "init.sql"
        depends_on:
          - "redis"
        command: "postgres -c max_connections=200"
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        let spec = ComponentBuildSpec::from_yaml(config, variables, &yaml_value);

        // Verify the build type is LocalService
        match &spec.build_type {
            BuildType::LocalService {
                service_type,
                version,
                persist_data,
                env,
                health_check,
                init_scripts,
                depends_on,
                command,
            } => {
                assert_eq!(service_type, "postgresql");
                assert_eq!(version.as_deref(), Some("15"));
                assert_eq!(*persist_data, true);
                
                // Check environment variables
                let env = env.as_ref().unwrap();
                assert_eq!(env.get("POSTGRES_USER"), Some(&"testuser".to_string()));
                assert_eq!(env.get("POSTGRES_PASSWORD"), Some(&"testpass".to_string()));
                assert_eq!(env.get("POSTGRES_DB"), Some(&"testdb".to_string()));
                assert_eq!(env.get("POSTGRES_PORT"), Some(&"5432".to_string()));
                
                assert_eq!(health_check.as_deref(), Some("pg_isready -U testuser -p 5432"));
                
                let init_scripts = init_scripts.as_ref().unwrap();
                assert_eq!(init_scripts.len(), 1);
                assert_eq!(init_scripts[0], "init.sql");
                
                let depends_on = depends_on.as_ref().unwrap();
                assert_eq!(depends_on.len(), 1);
                assert_eq!(depends_on[0], "redis");
                
                assert_eq!(command.as_deref(), Some("postgres -c max_connections=200"));
            }
            _ => panic!("Expected LocalService build type"),
        }
    }

    #[test]
    fn test_parse_local_service_minimal() {
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "redis"
        service_type: "redis"
        persist_data: false
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        let spec = ComponentBuildSpec::from_yaml(config, variables, &yaml_value);

        match &spec.build_type {
            BuildType::LocalService {
                service_type,
                version,
                persist_data,
                env,
                health_check,
                init_scripts,
                depends_on,
                command,
            } => {
                assert_eq!(service_type, "redis");
                assert!(version.is_none());
                assert_eq!(*persist_data, false);
                assert!(env.is_none());
                assert!(health_check.is_none());
                assert!(init_scripts.is_none());
                assert!(depends_on.is_none());
                assert!(command.is_none());
            }
            _ => panic!("Expected LocalService build type"),
        }
    }

    #[test]
    fn test_parse_local_service_with_version() {
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "mongodb"
        service_type: "mongodb"
        version: "6.0"
        persist_data: true
        env:
          MONGO_INITDB_ROOT_USERNAME: "admin"
          MONGO_INITDB_ROOT_PASSWORD: "secret"
          MONGO_PORT: "27017"
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        let spec = ComponentBuildSpec::from_yaml(config, variables, &yaml_value);

        match &spec.build_type {
            BuildType::LocalService {
                service_type,
                version,
                persist_data,
                env,
                ..
            } => {
                assert_eq!(service_type, "mongodb");
                assert_eq!(version.as_deref(), Some("6.0"));
                assert_eq!(*persist_data, true);
                
                let env = env.as_ref().unwrap();
                assert_eq!(env.get("MONGO_PORT"), Some(&"27017".to_string()));
            }
            _ => panic!("Expected LocalService build type"),
        }
    }

    #[test]
    fn test_local_service_no_docker_fields() {
        // This test ensures that Docker-specific fields are not present in LocalService
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "stripe"
        service_type: "stripe-cli"
        persist_data: false
        env:
          STRIPE_API_KEY: "test_key"
        command: "stripe listen --forward-to localhost:8080"
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        let spec = ComponentBuildSpec::from_yaml(config, variables, &yaml_value);

        match &spec.build_type {
            BuildType::LocalService { service_type, .. } => {
                assert_eq!(service_type, "stripe-cli");
                // The BuildType::LocalService variant doesn't have image, ports, volumes, or docker_args fields
                // This is enforced by the type system
            }
            _ => panic!("Expected LocalService build type"),
        }
    }

    #[test]
    #[should_panic(expected = "service_type is required for LocalService")]
    fn test_local_service_missing_service_type() {
        let yaml_str = r#"
        build_type: "LocalService"
        component_name: "test"
        persist_data: true
        "#;

        let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
        let config = create_test_config();
        let variables = create_test_variables();

        // This should panic because service_type is required
        ComponentBuildSpec::from_yaml(config, variables, &yaml_value);
    }

    #[test]
    fn test_all_build_types_flat_structure() {
        // Test that all build types use flat structure consistently
        let test_cases = vec![
            (
                "LocalService",
                r#"
                build_type: "LocalService"
                component_name: "postgres"
                service_type: "postgresql"
                persist_data: true
                "#,
            ),
            (
                "RustBinary",
                r#"
                build_type: "RustBinary"
                component_name: "backend"
                location: "backend/server"
                dockerfile: "backend/Dockerfile"
                "#,
            ),
            (
                "TrunkWasm",
                r#"
                build_type: "TrunkWasm"
                component_name: "frontend"
                location: "frontend/webui"
                dockerfile: "frontend/Dockerfile"
                ssr: false
                "#,
            ),
            (
                "Image",
                r#"
                build_type: "Image"
                component_name: "database"
                image: "postgres:latest"
                "#,
            ),
        ];

        for (expected_type, yaml_str) in test_cases {
            let yaml_value: serde_yaml::Value = serde_yaml::from_str(yaml_str).unwrap();
            
            // Verify build_type is a string at the same level as other fields
            assert!(yaml_value.get("build_type").unwrap().is_string());
            assert_eq!(
                yaml_value.get("build_type").unwrap().as_str().unwrap(),
                expected_type
            );
            
            // Verify it's not a nested structure
            assert!(yaml_value.get("build_type").unwrap().as_mapping().is_none());
        }
    }
}