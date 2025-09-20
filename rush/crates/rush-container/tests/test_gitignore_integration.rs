use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use rush_container::tagging::ImageTagGenerator;
use rush_toolchain::ToolchainContext;

#[test]
fn test_gitignore_integration_in_hash_computation() {
    // Create a temporary directory with git repo
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Initialize git repo
    std::process::Command::new("git")
        .args(&["init"])
        .current_dir(base_path)
        .output()
        .unwrap();

    // Create component directory structure
    let products_dir = base_path.join("products");
    let product_dir = products_dir.join("test-product");
    let component_dir = product_dir.join("backend");
    fs::create_dir_all(&component_dir).unwrap();

    // Create source files
    fs::write(component_dir.join("main.rs"), "fn main() {}").unwrap();
    fs::write(component_dir.join("lib.rs"), "pub fn lib() {}").unwrap();

    // Create build artifacts that should be ignored
    let target_dir = component_dir.join("target");
    fs::create_dir_all(&target_dir).unwrap();
    fs::write(target_dir.join("debug.exe"), "binary data").unwrap();

    // Create .gitignore
    fs::write(component_dir.join(".gitignore"), "target/\n*.log\n").unwrap();

    // Create a log file that should be ignored
    fs::write(component_dir.join("debug.log"), "log data").unwrap();

    // Commit initial state
    std::process::Command::new("git")
        .args(&["add", "."])
        .current_dir(base_path)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(&["commit", "-m", "initial"])
        .current_dir(base_path)
        .output()
        .unwrap();

    // Create tag generator
    let toolchain = Arc::new(ToolchainContext::default());
    let tag_generator = ImageTagGenerator::new(toolchain, base_path.to_path_buf());

    // Create a simple spec
    std::env::set_var("LOCAL_CTX", "docker-desktop");
    std::env::set_var("LOCAL_VAULT", "test-vault");
    std::env::set_var("K8S_ENCODER_LOCAL", "kubeseal");
    std::env::set_var("K8S_VALIDATOR_LOCAL", "kubeval");
    std::env::set_var("K8S_VERSION_LOCAL", "1.28.0");
    std::env::set_var("LOCAL_DOMAIN", "localhost");
    std::env::set_var("INFRASTRUCTURE_REPOSITORY", "https://github.com/test/infra");

    let config = rush_config::Config::new(
        base_path.to_str().unwrap(),
        "test-product",
        "local",
        "localhost:5000",
        8000,
    ).expect("Failed to create config");

    let spec = rush_build::ComponentBuildSpec {
        build_type: rush_build::BuildType::RustBinary {
            location: "products/test-product/backend".to_string(),
            dockerfile_path: "Dockerfile".to_string(),
            context_dir: Some(".".to_string()),
            features: None,
            precompile_commands: None,
        },
        product_name: "test-product".to_string(),
        component_name: "backend".to_string(),
        color: "blue".to_string(),
        depends_on: vec![],
        build: None,
        mount_point: None,
        subdomain: None,
        artefacts: None,
        artefact_output_dir: "dist".to_string(),
        docker_extra_run_args: vec![],
        env: None,
        volumes: None,
        port: None,
        target_port: None,
        k8s: None,
        priority: 0,
        watch: None,
        config,
        variables: rush_build::Variables::empty(),
        services: None,
        domains: None,
        tagged_image_name: None,
        dotenv: Default::default(),
        dotenv_secrets: Default::default(),
        domain: "localhost".to_string(),
        cross_compile: "native".to_string(),
        health_check: None,
        startup_probe: None,
    };

    // The main purpose of this test is to verify that gitignored files are excluded
    // from the file list used for hash computation

    // Create a new source file that will be included
    fs::write(component_dir.join("new.rs"), "// new file").unwrap();

    // Modify ignored file (the hash computation should not include it)
    fs::write(component_dir.join("debug.log"), "more log data that should be ignored").unwrap();

    // Create new ignored file in target/
    fs::write(target_dir.join("another.exe"), "another binary").unwrap();

    // The important test: the files included in the hash should not contain ignored files
    // We'll verify this by checking that the component walk doesn't include them
    let (files, _dirs) = tag_generator.get_watch_files_and_directories(&spec);

    println!("Files found: {}", files.len());
    for file in &files {
        println!("  - {}", file.display());
    }

    // Check if we found any files at all
    assert!(!files.is_empty(), "Should find at least some files in the component directory");

    // Verify gitignored files are excluded
    assert!(!files.iter().any(|f| f.ends_with("debug.log")),
            "Log files should be excluded by gitignore");
    assert!(!files.iter().any(|f| f.to_str().unwrap().contains("/target/")),
            "Target directory files should be excluded by gitignore");

    // Verify source files are included
    assert!(files.iter().any(|f| f.ends_with("main.rs")),
            "Source files should be included - main.rs");
    assert!(files.iter().any(|f| f.ends_with("lib.rs")),
            "Source files should be included - lib.rs");
    assert!(files.iter().any(|f| f.ends_with("new.rs")),
            "New source files should be included - new.rs");

    println!("Gitignore integration test passed!");
    println!("Total files found: {}", files.len());
    println!("Files: {:?}", files.iter().map(|f| f.file_name().unwrap()).collect::<Vec<_>>());
}