use rush_cli::builder::BuildContext;
use rush_cli::builder::BuildType;
use rush_cli::container::ServicesSpec;
use rush_cli::toolchain::Platform;
use rush_cli::toolchain::ToolchainContext;
use std::collections::HashMap;

#[test]
fn test_build_context_serialization() {
    // Create a test BuildContext instance
    let build_context = create_test_build_context();
    
    // Serialize to JSON
    let serialized = serde_json::to_string(&build_context).expect("Failed to serialize BuildContext");
    
    // Deserialize back
    let deserialized: BuildContext = serde_json::from_str(&serialized).expect("Failed to deserialize BuildContext");
    
    // Verify fields match
    assert_eq!(deserialized.product_name, "test-product");
    assert_eq!(deserialized.product_uri, "test-uri");
    assert_eq!(deserialized.component, "test-component");
    assert_eq!(deserialized.environment, "dev");
    assert_eq!(deserialized.domain, "test-domain.com");
    assert_eq!(deserialized.docker_registry, "test-registry.com");
    assert_eq!(deserialized.image_name, "test-image");
}

#[test]
fn test_build_context_with_environment_variables() {
    // Create a context with environment variables
    let mut build_context = create_test_build_context();
    
    // Add environment variables
    build_context.env.insert("API_URL".to_string(), "https://api.example.com".to_string());
    build_context.env.insert("DEBUG".to_string(), "true".to_string());
    
    // Verify the environment variables are correctly stored
    assert_eq!(build_context.env.get("API_URL"), Some(&"https://api.example.com".to_string()));
    assert_eq!(build_context.env.get("DEBUG"), Some(&"true".to_string()));
}

#[test]
fn test_build_context_with_secrets() {
    // Create a context with secrets
    let mut build_context = create_test_build_context();
    
    // Add secrets
    build_context.secrets.insert("DB_PASSWORD".to_string(), "secure123".to_string());
    build_context.secrets.insert("API_KEY".to_string(), "xyz-api-key".to_string());
    
    // Verify the secrets are correctly stored
    assert_eq!(build_context.secrets.get("DB_PASSWORD"), Some(&"secure123".to_string()));
    assert_eq!(build_context.secrets.get("API_KEY"), Some(&"xyz-api-key".to_string()));
}

#[test]
fn test_build_context_with_domains() {
    // Create a context with multiple domains
    let mut build_context = create_test_build_context();
    
    // Add domains
    build_context.domains.insert("api".to_string(), "api.test-domain.com".to_string());
    build_context.domains.insert("admin".to_string(), "admin.test-domain.com".to_string());
    
    // Verify the domains are correctly stored
    assert_eq!(build_context.domains.get("api"), Some(&"api.test-domain.com".to_string()));
    assert_eq!(build_context.domains.get("admin"), Some(&"admin.test-domain.com".to_string()));
}

// Helper function to create a test BuildContext
fn create_test_build_context() -> BuildContext {
    BuildContext {
        build_type: BuildType::RustBinary {
            location: "/path/to/rust".to_string(),
            dockerfile_path: "/path/to/Dockerfile".to_string(),
            context_dir: None,
            features: None,
            precompile_commands: None,
        },
        location: Some("/path/to/source".to_string()),
        target: Platform { os: rush_cli::toolchain::platform::OperatingSystem::Linux, arch: rush_cli::toolchain::platform::ArchType::X86_64 },
        host: Platform { os: rush_cli::toolchain::platform::OperatingSystem::Linux, arch: rush_cli::toolchain::platform::ArchType::X86_64 },
        rust_target: "x86_64-unknown-linux-gnu".to_string(),
        toolchain: ToolchainContext::default(),
        services: ServicesSpec::default(),
        environment: "dev".to_string(),
        domain: "test-domain.com".to_string(),
        product_name: "test-product".to_string(),
        product_uri: "test-uri".to_string(),
        component: "test-component".to_string(),
        docker_registry: "test-registry.com".to_string(),
        image_name: "test-image".to_string(),
        domains: HashMap::new(),
        env: HashMap::new(),
        secrets: HashMap::new(),
    }
}