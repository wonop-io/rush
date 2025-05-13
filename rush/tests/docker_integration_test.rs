// Integration tests for Docker functionality
#[path = "common/mod.rs"]
mod common;

#[cfg(test)]
mod integration_tests {
    use std::sync::Arc;
    use std::collections::HashMap;
    
    use rush_cli::container::docker::DockerImage;
    use rush_cli::toolchain::{Platform, ToolchainContext};
    
    #[test]
    fn test_docker_image_lifecycle() {
        // Create test configuration
        let config = crate::common::create_test_config();
        let spec = crate::common::create_test_spec(config);
        
        // Create DockerImage from spec
        let result = DockerImage::from_docker_spec(spec.clone());
        assert!(result.is_ok(), "Failed to create DockerImage: {:?}", result.err());
        
        let mut image = result.unwrap();
        
        // Verify image initial state
        assert_eq!(image.component_name(), "test-component");
        assert!(!image.should_rebuild());
        
        // Modify image state
        image.set_should_rebuild(true);
        assert!(image.should_rebuild());
        
        // Test tagging
        image.set_tag("integration-test".to_string());
        let tagged_name = image.tagged_image_name();
        assert!(tagged_name.contains("integration-test"),
                "Tagged image name should contain the tag: {}", tagged_name);
        
        // Set toolchain
        let host = Platform::new("linux", "x86_64");
        let target = Platform::new("linux", "x86_64");
        let toolchain = Arc::new(ToolchainContext::new(host, target));
        image.set_toolchain(toolchain);
        
        // Generate build context with empty secrets map
        let build_context = image.generate_build_context(HashMap::new());
        
        // Basic verification of build context
        assert!(format!("{:?}", build_context.build_type).contains("PureDockerImage"));
        
        // This is an integration test, so we're not actually building or running 
        // the container here, just verifying that the API behaves as expected
    }
}