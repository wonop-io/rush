use std::fs;
use std::process::Command;
use std::sync::Arc;

use rush_container::tagging::ImageTagGenerator;
use rush_toolchain::ToolchainContext;
use tempfile::TempDir;

/// Helper function to set up test environment variables
fn setup_test_env() {
    std::env::set_var("LOCAL_CTX", "docker-desktop");
    std::env::set_var("LOCAL_VAULT", "test-vault");
    std::env::set_var("K8S_ENCODER_LOCAL", "kubeseal");
    std::env::set_var("K8S_VALIDATOR_LOCAL", "kubeval");
    std::env::set_var("K8S_VERSION_LOCAL", "1.28.0");
    std::env::set_var("LOCAL_DOMAIN", "localhost");
    std::env::set_var("INFRASTRUCTURE_REPOSITORY", "https://github.com/test/infra");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hash_excludes_gitignored_files() {
    setup_test_env();

    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Setup products directory structure
    let products_dir = base_path.join("products");
    let product_dir = products_dir.join("test-product");
    let component_dir = product_dir.join("backend");
    fs::create_dir_all(&component_dir).unwrap();

    // Create files
    fs::write(component_dir.join("main.rs"), "fn main() {}").unwrap();
    fs::write(component_dir.join("temp.log"), "log data").unwrap();

    // Create .gitignore
    fs::write(component_dir.join(".gitignore"), "*.log\n").unwrap();

    // Initialize git
    Command::new("git")
        .args(["init"])
        .current_dir(base_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(base_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "test"])
        .current_dir(base_path)
        .output()
        .unwrap();

    let toolchain = Arc::new(ToolchainContext::default());
    let tag_generator = ImageTagGenerator::new(toolchain.clone(), base_path.to_path_buf());

    // Create a simple spec
    let config = rush_config::Config::new(
        base_path.to_str().unwrap(),
        "test-product",
        "local",
        "localhost:5000",
        8000,
    )
    .expect("Failed to create config");

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

    // Get hash before
    let hash1 = tag_generator.compute_tag(&spec).unwrap();

    // Modify gitignored file
    fs::write(
        component_dir.join("temp.log"),
        "modified log data that should not affect hash",
    )
    .unwrap();

    // Hash should NOT change
    let hash2 = tag_generator.compute_tag(&spec).unwrap();
    assert_eq!(hash1, hash2, "Hash should not change for gitignored files");

    // Modify tracked file
    fs::write(
        component_dir.join("main.rs"),
        "fn main() { println!(\"hi\"); }",
    )
    .unwrap();

    // Add and commit the change to make it part of the repository
    Command::new("git")
        .args(["add", "."])
        .current_dir(base_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "modified"])
        .current_dir(base_path)
        .output()
        .unwrap();

    // Create a new tag generator to avoid caching issues
    let tag_generator2 = ImageTagGenerator::new(toolchain.clone(), base_path.to_path_buf());

    // Hash SHOULD change after committing
    let hash3 = tag_generator2.compute_tag(&spec).unwrap();
    assert_ne!(
        hash2, hash3,
        "Hash should change for tracked files after commit"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hash_respects_nested_gitignores() {
    setup_test_env();

    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Setup nested directory structure
    let products_dir = base_path.join("products");
    let product_dir = products_dir.join("test-product");
    let component_dir = product_dir.join("backend");
    let src_dir = component_dir.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    // Create files at different levels
    fs::write(component_dir.join("main.rs"), "fn main() {}").unwrap();
    fs::write(component_dir.join("build.log"), "build log").unwrap();
    fs::write(src_dir.join("lib.rs"), "pub fn lib() {}").unwrap();
    fs::write(src_dir.join("debug.log"), "debug log").unwrap();

    // Create nested .gitignore files
    fs::write(base_path.join(".gitignore"), "*.tmp\n").unwrap();
    fs::write(component_dir.join(".gitignore"), "build.log\n").unwrap();
    fs::write(src_dir.join(".gitignore"), "debug.log\n").unwrap();

    // Initialize git
    Command::new("git")
        .args(["init"])
        .current_dir(base_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(base_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "test"])
        .current_dir(base_path)
        .output()
        .unwrap();

    let toolchain = Arc::new(ToolchainContext::default());
    let tag_generator = ImageTagGenerator::new(toolchain.clone(), base_path.to_path_buf());

    let config = rush_config::Config::new(
        base_path.to_str().unwrap(),
        "test-product",
        "local",
        "localhost:5000",
        8000,
    )
    .expect("Failed to create config");

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

    // Get initial hash
    let hash1 = tag_generator.compute_tag(&spec).unwrap();

    // Create and modify files that match different gitignore patterns
    fs::write(base_path.join("test.tmp"), "temp file").unwrap();
    fs::write(component_dir.join("build.log"), "modified build log").unwrap();
    fs::write(src_dir.join("debug.log"), "modified debug log").unwrap();

    // Hash should NOT change for any gitignored files
    let hash2 = tag_generator.compute_tag(&spec).unwrap();
    assert_eq!(
        hash1, hash2,
        "Hash should not change for files ignored at any level"
    );

    // Modify a tracked file
    fs::write(src_dir.join("lib.rs"), "pub fn lib() { /* modified */ }").unwrap();

    // Add and commit the change
    Command::new("git")
        .args(["add", "."])
        .current_dir(base_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "modified"])
        .current_dir(base_path)
        .output()
        .unwrap();

    // Create a new tag generator to avoid caching issues
    let tag_generator2 = ImageTagGenerator::new(toolchain.clone(), base_path.to_path_buf());

    // Hash SHOULD change after committing
    let hash3 = tag_generator2.compute_tag(&spec).unwrap();
    assert_ne!(
        hash2, hash3,
        "Hash should change for tracked files in nested directories after commit"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hash_with_target_directory() {
    setup_test_env();

    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();

    // Setup component with target directory
    let products_dir = base_path.join("products");
    let product_dir = products_dir.join("test-product");
    let component_dir = product_dir.join("backend");
    let target_dir = component_dir.join("target");
    let debug_dir = target_dir.join("debug");
    fs::create_dir_all(&debug_dir).unwrap();

    // Create source files
    fs::write(component_dir.join("main.rs"), "fn main() {}").unwrap();
    fs::write(
        component_dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\n",
    )
    .unwrap();

    // Create build artifacts that should be ignored
    fs::write(debug_dir.join("binary"), "compiled binary data").unwrap();
    fs::write(target_dir.join("CACHEDIR.TAG"), "cache dir tag").unwrap();

    // Create .gitignore
    fs::write(component_dir.join(".gitignore"), "target/\n*.log\n").unwrap();

    // Initialize git
    Command::new("git")
        .args(["init"])
        .current_dir(base_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(base_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "test"])
        .current_dir(base_path)
        .output()
        .unwrap();

    let toolchain = Arc::new(ToolchainContext::default());
    let tag_generator = ImageTagGenerator::new(toolchain.clone(), base_path.to_path_buf());

    let config = rush_config::Config::new(
        base_path.to_str().unwrap(),
        "test-product",
        "local",
        "localhost:5000",
        8000,
    )
    .expect("Failed to create config");

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

    // Get initial hash
    let hash1 = tag_generator.compute_tag(&spec).unwrap();

    // Modify build artifacts (should not affect hash)
    fs::write(
        debug_dir.join("binary"),
        "recompiled binary with different data",
    )
    .unwrap();
    fs::write(debug_dir.join("deps.d"), "dependency file").unwrap();

    let hash2 = tag_generator.compute_tag(&spec).unwrap();
    assert_eq!(
        hash1, hash2,
        "Hash should not change when target/ directory contents change"
    );

    // Modify source file (should affect hash)
    fs::write(
        component_dir.join("main.rs"),
        "fn main() { println!(\"changed\"); }",
    )
    .unwrap();

    // Add and commit the change
    Command::new("git")
        .args(["add", "."])
        .current_dir(base_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "modified source"])
        .current_dir(base_path)
        .output()
        .unwrap();

    // Create a new tag generator to avoid caching issues
    let tag_generator2 = ImageTagGenerator::new(toolchain.clone(), base_path.to_path_buf());

    let hash3 = tag_generator2.compute_tag(&spec).unwrap();
    assert_ne!(
        hash2, hash3,
        "Hash should change when source files change after commit"
    );
}
