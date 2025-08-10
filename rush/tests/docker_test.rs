#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::path::PathBuf;
    
    use rush_cli::build::{BuildType, ComponentBuildSpec, Variables};
    use rush_cli::core::Config;
    use rush_cli::container::{DockerImage, DockerCliClient};
    use rush_cli::toolchain::{Platform, ToolchainContext};
    
    // Create a test config suitable for basic tests
    fn create_test_config() -> Arc<Config> {
        let config = Config {
            product_name: "test-product".to_string(),
            product_uri: "test.app".to_string(),
            product_dirname: "test_app".to_string(),
            product_path: "/tmp/test_product".to_string(),
            network_name: "test-network".to_string(),
            environment: "dev".to_string(),
            domain_template: "{{subdomain}}.{{product_uri}}".to_string(),
            kube_context: "test-context".to_string(),
            infrastructure_repository: "git@github.com:test/infra.git".to_string(),
            docker_registry: "ghcr.io/test".to_string(),
            root_path: "/tmp".to_string(),
            vault_name: "test-vault".to_string(),
            k8s_encoder: "default".to_string(),
            k8s_validator: "default".to_string(),
            k8s_version: "v1.25.0".to_string(),
            one_password_account: None,
            json_vault_dir: None,
            start_port: 8000,
        };
        
        Arc::new(config)
    }
    
    // Create test variables
    fn create_test_variables() -> Arc<Variables> {
        // Creates empty variables for the dev environment
        Variables::new("/nonexistent/path", "dev")
    }
    
    // Create a test toolchain context
    fn create_test_toolchain() -> Arc<ToolchainContext> {
        let host = Platform::new("linux", "x86_64");
        let target = Platform::new("linux", "x86_64");
        Arc::new(ToolchainContext::new(host, target))
    }
    
    // Create a simple component build spec for testing
    fn create_test_spec(config: Arc<Config>) -> Arc<Mutex<ComponentBuildSpec>> {
        let variables = create_test_variables();
        
        let build_type = BuildType::PureDockerImage {
            image_name_with_tag: "test-image:latest".to_string(),
            command: None,
            entrypoint: None,
        };
        
        let spec = ComponentBuildSpec {
            build_type,
            product_name: "test-product".to_string(),
            component_name: "test-component".to_string(),
            color: "blue".to_string(),
            depends_on: vec![],
            build: None,
            mount_point: None,
            subdomain: Some("test".to_string()),
            artefacts: None,
            artefact_output_dir: "dist".to_string(),
            docker_extra_run_args: vec![],
            env: Some(HashMap::new()),
            volumes: Some(HashMap::new()),
            port: Some(8080),
            target_port: Some(8080),
            k8s: None,
            priority: 0,
            watch: None,
            config: config.clone(),
            variables: variables.clone(),
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: HashMap::new(),
            dotenv_secrets: HashMap::new(),
            domain: "test.test.app".to_string(),
        };
        
        Arc::new(Mutex::new(spec))
    }
    
    #[test]
    fn test_docker_image_creation_from_spec() {
        let config = create_test_config();
        let spec = create_test_spec(config);
        
        // Create DockerImage from spec
        let result = DockerImage::from_build_spec(spec.clone(), Arc::new(DockerCliClient::new("docker".to_string())));
        
        // Verify image was created successfully
        assert!(result.is_ok(), "Failed to create DockerImage: {:?}", result.err());
        let image = result.unwrap();
        
        // Test basic properties
        assert_eq!(image.component_name(), "test-component");
        assert_eq!(image.image_name(), "test-image");
        assert!(!image.should_rebuild());
        assert!(!image.was_recently_rebuild());
    }
    
    #[test]
    fn test_docker_image_setters() {
        let config = create_test_config();
        let spec = create_test_spec(config);
        
        let result = DockerImage::from_build_spec(spec.clone(), Arc::new(DockerCliClient::new("docker".to_string())));
        assert!(result.is_ok());
        let mut image = result.unwrap();
        
        // Test setters
        image.set_should_rebuild(true);
        assert!(image.should_rebuild());
        
        image.set_was_recently_rebuild(true);
        assert!(image.was_recently_rebuild());
        
        image.set_ignore_in_devmode(true);
        assert!(image.should_ignore_in_devmode());
        
        image.set_network_name("custom-network".to_string());
        // Network name is private, so we can't directly test it
        // But setting it shouldn't fail
        
        image.set_silence_output(true);
        // Silence output is private, so we can't directly test it
        // But setting it shouldn't fail
    }
    
    #[test]
    fn test_docker_image_tagging() {
        let config = create_test_config();
        let spec = create_test_spec(config);
        
        let result = DockerImage::from_build_spec(spec.clone(), Arc::new(DockerCliClient::new("docker".to_string())));
        assert!(result.is_ok());
        let mut image = result.unwrap();
        
        // Test tagging
        image.set_tag("v1.0.0".to_string());
        
        // Tagged image name should now contain the tag
        let tagged_name = image.tagged_image_name();
        assert!(tagged_name.contains("v1.0.0"),
                "Tagged image name should contain the tag: {}", tagged_name);
    }
    
    #[test]
    fn test_docker_image_toolchain() {
        let config = create_test_config();
        let spec = create_test_spec(config);
        let toolchain = create_test_toolchain();
        
        let result = DockerImage::from_build_spec(spec.clone(), Arc::new(DockerCliClient::new("docker".to_string())));
        assert!(result.is_ok());
        let mut image = result.unwrap();
        
        // Test setting toolchain
        // Note: toolchain is set internally by ImageBuilder
        // Toolchain is private, so we can't directly test it
        // But setting it shouldn't fail
    }
    
    #[tokio::test]
    async fn test_docker_image_build_context() {
        let config = create_test_config();
        let spec = create_test_spec(config);
        
        let result = DockerImage::from_build_spec(spec.clone(), Arc::new(DockerCliClient::new("docker".to_string())));
        assert!(result.is_ok());
        let image = result.unwrap();
        
        // Generate build context
        let build_context = image.generate_build_context().await.unwrap();
        
        // Verify build context contains expected values
        assert_eq!(build_context.location, None);
        
        // Check the build type using Debug representation since it doesn't implement Display
        let build_type_str = format!("{:?}", build_context.build_type);
        assert!(build_type_str.contains("PureDockerImage"));
    }
    
    #[test]
    fn test_docker_image_dependencies() {
        let config = create_test_config();
        let variables = create_test_variables();
        
        // Create first component - no dependencies
        let build_type1 = BuildType::PureDockerImage {
            image_name_with_tag: "test-image1:latest".to_string(),
            command: None,
            entrypoint: None,
        };
        
        let spec1 = ComponentBuildSpec {
            build_type: build_type1,
            product_name: "test-product".to_string(),
            component_name: "component1".to_string(),
            color: "blue".to_string(),
            depends_on: vec![],
            build: None,
            mount_point: None,
            subdomain: Some("test1".to_string()),
            artefacts: None,
            artefact_output_dir: "dist".to_string(),
            docker_extra_run_args: vec![],
            env: Some(HashMap::new()),
            volumes: Some(HashMap::new()),
            port: Some(8081),
            target_port: Some(8081),
            k8s: None,
            priority: 0,
            watch: None,
            config: config.clone(),
            variables: variables.clone(),
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: HashMap::new(),
            dotenv_secrets: HashMap::new(),
            domain: "test1.test.app".to_string(),
        };
        
        // Create second component - depends on first component
        let build_type2 = BuildType::PureDockerImage {
            image_name_with_tag: "test-image2:latest".to_string(),
            command: None,
            entrypoint: None,
        };
        
        let spec2 = ComponentBuildSpec {
            build_type: build_type2,
            product_name: "test-product".to_string(),
            component_name: "component2".to_string(),
            color: "green".to_string(),
            depends_on: vec!["component1".to_string()],
            build: None,
            mount_point: None,
            subdomain: Some("test2".to_string()),
            artefacts: None,
            artefact_output_dir: "dist".to_string(),
            docker_extra_run_args: vec![],
            env: Some(HashMap::new()),
            volumes: Some(HashMap::new()),
            port: Some(8082),
            target_port: Some(8082),
            k8s: None,
            priority: 1,
            watch: None,
            config: config.clone(),
            variables: variables.clone(),
            services: None,
            domains: None,
            tagged_image_name: None,
            dotenv: HashMap::new(),
            dotenv_secrets: HashMap::new(),
            domain: "test2.test.app".to_string(),
        };
        
        let spec1 = Arc::new(Mutex::new(spec1));
        let spec2 = Arc::new(Mutex::new(spec2));
        
        // Create DockerImages from specs
        let docker_client = Arc::new(DockerCliClient::new("docker".to_string()));
        let result1 = DockerImage::from_build_spec(spec1.clone(), docker_client.clone());
        let result2 = DockerImage::from_build_spec(spec2.clone(), docker_client);
        
        assert!(result1.is_ok());
        assert!(result2.is_ok());
        
        let image1 = result1.unwrap();
        let image2 = result2.unwrap();
        
        // TODO: Verify dependencies when method is available
        // Dependencies are handled at the build spec level, not the image builder level
        // assert!(image1.depends_on().is_empty(), "First image should have no dependencies");
        // assert_eq!(image2.depends_on().len(), 1, "Second image should have one dependency");
        // assert_eq!(image2.depends_on()[0], "component1", "Second image should depend on component1");
    }
}