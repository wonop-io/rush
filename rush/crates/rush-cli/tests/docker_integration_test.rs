// Integration tests for Docker functionality
#[path = "common/mod.rs"]
mod common;

#[cfg(test)]
mod integration_tests {
    
    use std::sync::Arc;

    use rush_container::{DockerCliClient, DockerImage};
    use rush_toolchain::{Platform, ToolchainContext};

    #[tokio::test]
    async fn test_docker_image_lifecycle() {
        // Create test configuration
        let config = crate::common::create_test_config();
        let spec = crate::common::create_test_spec(config);

        // Create DockerImage from spec
        let result = DockerImage::from_build_spec(
            spec.clone(),
            Arc::new(DockerCliClient::new("docker".to_string())),
        );
        assert!(
            result.is_ok(),
            "Failed to create DockerImage: {:?}",
            result.err()
        );

        let mut image = result.unwrap();

        // Verify image initial state
        assert_eq!(image.component_name(), "test-component");
        // ImageBuilder starts with should_rebuild set to true by default
        assert!(image.should_rebuild());

        // Modify image state - set to false then back to true
        image.set_should_rebuild(false);
        assert!(!image.should_rebuild());

        image.set_should_rebuild(true);
        assert!(image.should_rebuild());

        // Note: set_tag method doesn't exist on ImageBuilder
        // The tagged image name is based on what's in the build spec
        let tagged_name = image.tagged_image_name();
        assert!(
            !tagged_name.is_empty(),
            "Tagged image name should not be empty: {tagged_name}"
        );

        // Set toolchain
        let host = Platform::new("linux", "x86_64");
        let target = Platform::new("linux", "x86_64");
        let _toolchain = Arc::new(ToolchainContext::new(host, target));
        // Note: toolchain is set internally by ImageBuilder

        // Note: generate_build_context() requires a vault to be configured,
        // which is not available in this test environment

        // This is an integration test, so we're not actually building or running
        // the container here, just verifying that the API behaves as expected
    }
}
