#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use rush_build::{BuildType, ComponentBuildSpec, Variables};
    use rush_config::Config;
    use rush_k8s::{ManifestGenerator, ManifestKind};
    use tempfile::TempDir;

    fn create_test_component(name: &str, port: Option<u16>) -> ComponentBuildSpec {
        let config = Config::test_default();
        let variables = Variables::new(std::path::Path::new("."), "test");

        ComponentBuildSpec {
            build_type: BuildType::RustBinary {
                location: "src".to_string(),
                dockerfile_path: "Dockerfile".to_string(),
                context_dir: None,
                features: None,
                precompile_commands: None,
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
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: HashMap::new(),
            dotenv_secrets: HashMap::new(),
            domain: "test.local".to_string(),
            cross_compile: "native".to_string(),
            health_check: None,
            startup_probe: None,
        }
    }

    #[tokio::test]
    async fn test_secrets_integration_with_deployments() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "production".to_string(),
        );

        let component = create_test_component("api-server", Some(8080));

        // Create some test secrets
        let mut secrets = BTreeMap::new();
        secrets.insert(
            "DATABASE_URL".to_string(),
            "postgres://localhost/mydb".to_string(),
        );
        secrets.insert("API_KEY".to_string(), "secret-api-key-123".to_string());
        secrets.insert("JWT_SECRET".to_string(), "jwt-secret-456".to_string());

        let manifests = generator
            .generate_manifests(&[component], Some(secrets))
            .await
            .unwrap();

        // Should generate deployment, service, and secret
        assert_eq!(manifests.len(), 3);

        // Check deployment has envFrom referencing the secret
        let deployment = manifests
            .iter()
            .find(|m| matches!(m.kind, ManifestKind::Deployment))
            .expect("Should have a deployment");

        assert!(deployment.content.contains("envFrom:"));
        assert!(deployment.content.contains("secretRef:"));
        assert!(deployment.content.contains("name: production-secrets"));

        // Check secret exists
        let secret = manifests
            .iter()
            .find(|m| matches!(m.kind, ManifestKind::Secret))
            .expect("Should have a secret");

        assert_eq!(secret.name, "production-secrets");
        assert!(secret.content.contains("kind: Secret"));
        assert!(secret.content.contains("type: Opaque"));
        assert!(secret.content.contains("DATABASE_URL:"));
        assert!(secret.content.contains("API_KEY:"));
        assert!(secret.content.contains("JWT_SECRET:"));

        // Verify base64 encoding
        // "postgres://localhost/mydb" base64 = "cG9zdGdyZXM6Ly9sb2NhbGhvc3QvbXlkYg=="
        assert!(secret
            .content
            .contains("cG9zdGdyZXM6Ly9sb2NhbGhvc3QvbXlkYg=="));
    }

    #[tokio::test]
    async fn test_volume_mounted_secrets() {
        // Set environment variable to enable volume mounting
        std::env::set_var("K8S_MOUNT_SECRETS", "true");

        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "production".to_string(),
        );

        let component = create_test_component("api-server", Some(8080));

        let mut secrets = BTreeMap::new();
        secrets.insert("TLS_CERT".to_string(), "cert-content".to_string());
        secrets.insert("TLS_KEY".to_string(), "key-content".to_string());

        let manifests = generator
            .generate_manifests(&[component], Some(secrets))
            .await
            .unwrap();

        let deployment = manifests
            .iter()
            .find(|m| matches!(m.kind, ManifestKind::Deployment))
            .expect("Should have a deployment");

        // Check for volume mounts
        assert!(deployment.content.contains("volumeMounts:"));
        assert!(deployment.content.contains("mountPath: /etc/secrets"));
        assert!(deployment.content.contains("readOnly: true"));

        // Check for volumes
        assert!(deployment.content.contains("volumes:"));
        assert!(deployment.content.contains("secret:"));
        assert!(deployment
            .content
            .contains("secretName: production-secrets"));

        // Clean up env var
        std::env::remove_var("K8S_MOUNT_SECRETS");
    }

    #[tokio::test]
    async fn test_no_secrets_no_references() {
        let temp_dir = TempDir::new().unwrap();
        let generator = ManifestGenerator::new(
            temp_dir.path().to_path_buf(),
            "test-namespace".to_string(),
            "production".to_string(),
        );

        let component = create_test_component("api-server", Some(8080));

        // No secrets provided
        let manifests = generator
            .generate_manifests(&[component], None)
            .await
            .unwrap();

        // Should only generate deployment and service (no secret)
        assert_eq!(manifests.len(), 2);

        let deployment = manifests
            .iter()
            .find(|m| matches!(m.kind, ManifestKind::Deployment))
            .expect("Should have a deployment");

        // Should not have envFrom or volume mounts
        assert!(!deployment.content.contains("envFrom:"));
        assert!(!deployment.content.contains("secretRef:"));
        assert!(!deployment.content.contains("volumeMounts:"));
        assert!(!deployment.content.contains("volumes:"));
    }
}
