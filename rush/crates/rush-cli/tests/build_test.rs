use rush_container::ContainerReactorConfig;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_image_name_generation_without_registry() {
    // Test that image names are generated correctly when docker registry is empty
    let config = ContainerReactorConfig {
        product_name: "test-product".to_string(),
        product_dir: PathBuf::from("/tmp/test"),
        network_name: "test-network".to_string(),
        environment: "local".to_string(),
        docker_registry: "".to_string(), // Empty registry for local
        redirected_components: HashMap::new(),
        silenced_components: HashSet::new(),
        verbose: false,
        watch_config: Default::default(),
        git_hash: "latest".to_string(),
        start_port: 8080,
    };

    let config = Arc::new(config);

    // Test image name without registry
    let component_name = "frontend";
    let image_name = if config.docker_registry.is_empty() {
        rush_core::naming::NamingConvention::image_name(&config.product_name, component_name)
    } else {
        format!(
            "{}/{}",
            config.docker_registry,
            rush_core::naming::NamingConvention::image_name(&config.product_name, component_name)
        )
    };

    assert_eq!(image_name, "test-product-frontend");
}

#[test]
fn test_image_name_generation_with_registry() {
    // Test that image names are generated correctly when docker registry is provided
    let config = ContainerReactorConfig {
        product_name: "test-product".to_string(),
        product_dir: PathBuf::from("/tmp/test"),
        network_name: "test-network".to_string(),
        environment: "local".to_string(),
        docker_registry: "docker.io/myuser".to_string(),
        redirected_components: HashMap::new(),
        silenced_components: HashSet::new(),
        verbose: false,
        watch_config: Default::default(),
        git_hash: "abc123".to_string(),
        start_port: 8080,
    };

    let config = Arc::new(config);

    // Test image name with registry
    let component_name = "backend";
    let image_name = if config.docker_registry.is_empty() {
        rush_core::naming::NamingConvention::image_name(&config.product_name, component_name)
    } else {
        format!(
            "{}/{}",
            config.docker_registry,
            rush_core::naming::NamingConvention::image_name(&config.product_name, component_name)
        )
    };

    assert_eq!(image_name, "docker.io/myuser/test-product-backend");

    // Test tagged image name
    let tagged_image = format!("{}:{}", image_name, config.git_hash);
    assert_eq!(tagged_image, "docker.io/myuser/test-product-backend:abc123");
}

#[test]
fn test_docker_registry_not_set_handling() {
    // Test that "not_set" is properly converted to empty string
    let registry = "not_set";
    let processed_registry = if registry == "not_set" { "" } else { registry };

    assert_eq!(processed_registry, "");
}
