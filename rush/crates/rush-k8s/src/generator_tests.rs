#[cfg(test)]
mod tests {
    use super::*;
    use rush_build::{ComponentBuildSpec, BuildType};
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Arc;
    use tempfile::TempDir;
    use rush_config::Config;
    use rush_build::Variables;

    fn create_test_component(name: &str, port: Option<u16>) -> ComponentBuildSpec {
        let config = Arc::new(Config::default());
        let variables = Arc::new(Variables::new());
        
        ComponentBuildSpec {
            build_type: BuildType::RustBinary {
                location: "src".to_string(),
                dockerfile_path: "Dockerfile".to_string(),
                context_dir: None,
                features: None,
                release: true,
            },
            product_name: "test-product".to_string(),
            component_name: name.to_string(),
            color: "blue".to_string(),
            depends_on: vec![],
            build: None,
            mount_point: None,
            subdomain: None,
            artefacts: None,
            artefact_output_dir: "/tmp".to_string(),
            docker_extra_run_args: vec![],
            env: None,
            volumes: None,
            port,
            target_port: None,
            k8s: None,
            priority: 0,
            watch: None,
            config,
            variables,
            service_spec: None,
        }
    }

    #[tokio::test]
    async fn test_generate_deployment() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "test".to_string(),
        );

        let component = create_test_component("test-app", Some(8080));
        let manifests = generator.generate_manifests(&[component], None).await.unwrap();

        // Should generate deployment and service
        assert_eq!(manifests.len(), 2);
        
        // Check deployment
        let deployment = manifests.iter()
            .find(|m| matches!(m.kind, ManifestKind::Deployment))
            .expect("Should have a deployment");
        assert_eq!(deployment.name, "test-app");
        assert_eq!(deployment.namespace, "test-namespace");
        assert!(deployment.content.contains("kind: Deployment"));
        assert!(deployment.content.contains("replicas: 1"));
        
        // Check service
        let service = manifests.iter()
            .find(|m| matches!(m.kind, ManifestKind::Service))
            .expect("Should have a service");
        assert_eq!(service.name, "test-app");
        assert!(service.content.contains("kind: Service"));
        assert!(service.content.contains("port: 8080"));
    }

    #[tokio::test]
    async fn test_generate_with_registry() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "test".to_string(),
        ).with_registry(
            Some("gcr.io".to_string()),
            Some("my-project".to_string()),
        );

        let component = create_test_component("test-app", Some(8080));
        let manifests = generator.generate_manifests(&[component], None).await.unwrap();

        let deployment = manifests.iter()
            .find(|m| matches!(m.kind, ManifestKind::Deployment))
            .unwrap();
        
        // Check that image has registry prefix
        assert!(deployment.content.contains("image: gcr.io/my-project/test-app:latest"));
    }

    #[tokio::test]
    async fn test_generate_ingress() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "test".to_string(),
        );

        let mut component1 = create_test_component("frontend", Some(3000));
        component1.mount_point = Some("/".to_string());
        
        let mut component2 = create_test_component("backend", Some(8080));
        component2.mount_point = Some("/api".to_string());
        
        let manifests = generator.generate_manifests(&[component1, component2], None).await.unwrap();

        // Should have 2 deployments, 2 services, and 1 ingress
        assert_eq!(manifests.len(), 5);
        
        let ingress = manifests.iter()
            .find(|m| matches!(m.kind, ManifestKind::Ingress))
            .expect("Should have an ingress");
        assert!(ingress.content.contains("kind: Ingress"));
        assert!(ingress.content.contains("path: /"));
        assert!(ingress.content.contains("path: /api"));
    }

    #[tokio::test]
    async fn test_generate_secrets() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "test".to_string(),
        );

        let component = create_test_component("test-app", None);
        
        let mut secrets = BTreeMap::new();
        secrets.insert("API_KEY".to_string(), "secret123".to_string());
        secrets.insert("DB_PASSWORD".to_string(), "pass456".to_string());
        
        let manifests = generator.generate_manifests(&[component], Some(secrets)).await.unwrap();

        // Should have deployment and secret (no service since no port)
        assert_eq!(manifests.len(), 2);
        
        let secret = manifests.iter()
            .find(|m| matches!(m.kind, ManifestKind::Secret))
            .expect("Should have a secret");
        assert_eq!(secret.name, "test-secrets");
        assert!(secret.content.contains("kind: Secret"));
        assert!(secret.content.contains("type: Opaque"));
        assert!(secret.content.contains("API_KEY:"));
        assert!(secret.content.contains("DB_PASSWORD:"));
    }

    #[tokio::test]
    async fn test_skip_local_service() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "test".to_string(),
        );

        let config = Arc::new(Config::default());
        let variables = Arc::new(Variables::new());
        
        let local_service = ComponentBuildSpec {
            build_type: BuildType::LocalService {
                service_type: "postgres".to_string(),
                version: Some("14".to_string()),
            },
            product_name: "test-product".to_string(),
            component_name: "postgres".to_string(),
            color: "blue".to_string(),
            depends_on: vec![],
            build: None,
            mount_point: None,
            subdomain: None,
            artefacts: None,
            artefact_output_dir: "/tmp".to_string(),
            docker_extra_run_args: vec![],
            env: None,
            volumes: None,
            port: Some(5432),
            target_port: None,
            k8s: None,
            priority: 0,
            watch: None,
            config,
            variables,
            service_spec: None,
        };

        let manifests = generator.generate_manifests(&[local_service], None).await.unwrap();
        
        // LocalService should be skipped
        assert_eq!(manifests.len(), 0);
    }

    #[tokio::test]
    async fn test_environment_variables() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "production".to_string(),
        );

        let mut component = create_test_component("test-app", Some(8080));
        let mut env = HashMap::new();
        env.insert("LOG_LEVEL".to_string(), "info".to_string());
        env.insert("FEATURE_FLAG".to_string(), "enabled".to_string());
        component.env = Some(env);
        
        let manifests = generator.generate_manifests(&[component], None).await.unwrap();

        let deployment = manifests.iter()
            .find(|m| matches!(m.kind, ManifestKind::Deployment))
            .unwrap();
        
        // Check environment variables are included
        assert!(deployment.content.contains("name: ENVIRONMENT"));
        assert!(deployment.content.contains("value: production"));
        assert!(deployment.content.contains("name: LOG_LEVEL"));
        assert!(deployment.content.contains("value: info"));
        assert!(deployment.content.contains("name: FEATURE_FLAG"));
        assert!(deployment.content.contains("value: enabled"));
    }
}