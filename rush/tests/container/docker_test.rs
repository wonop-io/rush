#[cfg(test)]
mod docker_tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::path::PathBuf;
    
    use rush_cli::builder::BuildType;
    use rush_cli::container::docker::DockerImage;

    #[test]
    fn test_docker_image_creation_from_spec() {
        let config = crate::common::create_test_config();
        let spec = crate::common::create_test_spec(config);
        
        // Create DockerImage from spec
        let result = DockerImage::from_docker_spec(spec.clone());
        
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
        let config = crate::common::create_test_config();
        let spec = crate::common::create_test_spec(config);
        
        let result = DockerImage::from_docker_spec(spec.clone());
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
        let config = crate::common::create_test_config();
        let spec = crate::common::create_test_spec(config);
        
        let result = DockerImage::from_docker_spec(spec.clone());
        assert!(result.is_ok());
        let mut image = result.unwrap();
        
        // Test tagging
        image.set_tag("v1.0.0".to_string());
        
        // Tagged image name should now contain the tag
        // Note: The actual format of the tagged image name might be different
        // depending on the implementation, so this test might need adjustment
        let tagged_name = image.tagged_image_name();
        assert!(tagged_name.contains("v1.0.0"), 
                "Tagged image name should contain the tag: {}", tagged_name);
    }
    
    #[test]
    fn test_docker_image_toolchain() {
        let config = crate::common::create_test_config();
        let spec = crate::common::create_test_spec(config);
        let toolchain = crate::common::create_test_toolchain();
        
        let result = DockerImage::from_docker_spec(spec.clone());
        assert!(result.is_ok());
        let mut image = result.unwrap();
        
        // Test setting toolchain
        image.set_toolchain(toolchain);
        // Toolchain is private, so we can't directly test it
        // But setting it shouldn't fail
    }
    
    #[test]
    fn test_docker_image_build_context() {
        let config = crate::common::create_test_config();
        let spec = crate::common::create_test_spec(config);
        
        let result = DockerImage::from_docker_spec(spec.clone());
        assert!(result.is_ok());
        let image = result.unwrap();
        
        // Generate build context with empty secrets map
        let build_context = image.generate_build_context(HashMap::new());
        
        // Verify build context contains expected values
        assert_eq!(build_context.location, None);
    }
    
    #[test]
    fn test_docker_image_dependencies() {
        let config = crate::common::create_test_config();
        
        // Create a spec with dependencies
        let variables = crate::common::create_test_variables();
        
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
        let result1 = DockerImage::from_docker_spec(spec1.clone());
        let result2 = DockerImage::from_docker_spec(spec2.clone());
        
        assert!(result1.is_ok());
        assert!(result2.is_ok());
        
        let image1 = result1.unwrap();
        let image2 = result2.unwrap();
        
        // Verify dependencies
        assert!(image1.depends_on().is_empty(), "First image should have no dependencies");
        assert_eq!(image2.depends_on().len(), 1, "Second image should have one dependency");
        assert_eq!(image2.depends_on()[0], "component1", "Second image should depend on component1");
    }
    
    #[test]
    fn test_docker_image_context_changes() {
        // This test would integrate with the filesystem to check context change detection
        // In a real test, you'd create files and check if they affect the Docker image
        // For now, we'll just test the API functionality
        
        let config = crate::common::create_test_config();
        let spec = crate::common::create_test_spec(config);
        
        let result = DockerImage::from_docker_spec(spec.clone());
        assert!(result.is_ok());
        let image = result.unwrap();
        
        // Create a list of test files
        let file_paths = vec![
            PathBuf::from("/tmp/test_product/test_file.txt"),
            PathBuf::from("/tmp/test_product/unrelated_file.rs"),
        ];
        
        // In a real test with actual files, this would check if changes to these files
        // would trigger a rebuild of the image
        let affects_image = image.is_any_file_in_context(&file_paths);
        
        // We expect false in this mock test since our test spec doesn't have a real context directory
        // but the important part is that the API call doesn't fail
        assert!(!affects_image, "Mock files should not affect the image context");
    }
    
    use rush_cli::builder::ComponentBuildSpec;
}