// Helper module for test utilities
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rush_cli::build::{BuildType, ComponentBuildSpec, Variables};
use rush_cli::core::Config;
use rush_cli::toolchain::{Platform, ToolchainContext};

// Create a test config suitable for basic tests
pub fn create_test_config() -> Arc<Config> {
    Config::test_default()
}

// Create test variables
pub fn create_test_variables() -> Arc<Variables> {
    // Creates empty variables for the dev environment
    Variables::new("/nonexistent/path", "dev")
}

// Create a test toolchain context
pub fn create_test_toolchain() -> Arc<ToolchainContext> {
    let host = Platform::new("macos", "aarch64");
    let target = Platform::new("linux", "x86_64");
    Arc::new(ToolchainContext::new(host, target))
}

// Create a simple component build spec for testing
pub fn create_test_spec(config: Arc<Config>) -> Arc<Mutex<ComponentBuildSpec>> {
    let variables = create_test_variables();

    // BuildType::Image is a simplification - you'll need to use one of the actual enum variants
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
        cross_compile: "native".to_string(),
        dotenv_secrets: HashMap::new(),
        domain: "test.test.app".to_string(),
    };

    Arc::new(Mutex::new(spec))
}
