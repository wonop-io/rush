use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;
use rush_cli::builder::{Artefact, BuildContext, BuildType};
use rush_cli::container::ServicesSpec;
use rush_cli::toolchain::{Platform, ToolchainContext};

#[test]
fn test_artefact_new() {
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let input_path = temp_dir.path().join("test_template.txt");
    let template_content = "Hello, {{ component }}!";
    
    fs::write(&input_path, template_content).expect("Failed to write test template");
    
    let output_path = temp_dir.path().join("output.txt");
    
    let artefact = Artefact::new(
        input_path.to_string_lossy().to_string(),
        output_path.to_string_lossy().to_string(),
    );
    
    assert_eq!(artefact.input_path, input_path.to_string_lossy().to_string());
    assert_eq!(artefact.output_path, output_path.to_string_lossy().to_string());
    assert_eq!(artefact.template, template_content);
}

#[test]
fn test_artefact_render() {
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let input_path = temp_dir.path().join("test_template.txt");
    let template_content = "Component: {{ component }}\nProduct: {{ product_name }}";
    
    fs::write(&input_path, template_content).expect("Failed to write test template");
    
    let output_path = temp_dir.path().join("output.txt");
    
    let artefact = Artefact::new(
        input_path.to_string_lossy().to_string(),
        output_path.to_string_lossy().to_string(),
    );
    
    let context = create_test_build_context();
    
    let rendered = artefact.render(&context);
    
    assert_eq!(rendered, "Component: test-component\nProduct: test-product");
}

#[test]
fn test_artefact_render_to_file() {
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let input_path = temp_dir.path().join("test_template.txt");
    let template_content = "Environment: {{ environment }}\nDomain: {{ domain }}";
    
    fs::write(&input_path, template_content).expect("Failed to write test template");
    
    let output_path = temp_dir.path().join("output.txt");
    
    let artefact = Artefact::new(
        input_path.to_string_lossy().to_string(),
        output_path.to_string_lossy().to_string(),
    );
    
    let context = create_test_build_context();
    
    artefact.render_to_file(&context);
    
    // Check if the file was created and has the expected content
    let output_content = fs::read_to_string(&output_path).expect("Failed to read output file");
    assert_eq!(output_content, "Environment: dev\nDomain: test-domain.com");
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