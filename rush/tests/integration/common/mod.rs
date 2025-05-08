use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

// Re-export the test project builder from the test_utils module
pub use crate::test_utils::TestProjectBuilder;

// Helper function to create a file with content
pub fn create_file(path: &Path, content: &str) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

// Helper function to create a complete rush project with working files
pub fn create_complete_test_project() -> (TempDir, PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_path = temp_dir.path().to_path_buf();

    // Create directory structure
    fs::create_dir_all(project_path.join("src")).unwrap();
    fs::create_dir_all(project_path.join("src/app")).unwrap();
    fs::create_dir_all(project_path.join("src/utils")).unwrap();

    // Create a simple main file
    let main_content = r#"
fn main() {
    println!("Hello from Rush test project");
}
"#;
    create_file(&project_path.join("src/main.rs"), main_content).unwrap();

    // Create rushd.yaml
    let rushd_yaml = r#"
env:
  - name: TEST_ENV
    value: test_value
  - name: DOCKER_HOST
    value: unix:///var/run/docker.sock
"#;
    create_file(&project_path.join("rushd.yaml"), rushd_yaml).unwrap();

    // Create Dockerfile
    let dockerfile = r#"
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/app /app
CMD ["/app"]
"#;
    create_file(&project_path.join("Dockerfile"), dockerfile).unwrap();

    // Create docker-compose.yml
    let docker_compose = r#"
version: '3'
services:
  app:
    build: .
    environment:
      - TEST_ENV=${TEST_ENV}
    ports:
      - "8080:8080"
"#;
    create_file(&project_path.join("docker-compose.yml"), docker_compose).unwrap();

    // Create .env file
    let dotenv = r#"
TEST_ENV=development
DEBUG=true
"#;
    create_file(&project_path.join(".env"), dotenv).unwrap();

    (temp_dir, project_path)
}

/// Check if docker is available for testing
pub fn docker_available() -> bool {
    Command::new("docker")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Set up the environment for integration tests
pub fn setup_integration_test_env() {
    // Set test mode
    env::set_var("RUSH_TEST_MODE", "true");
    env::set_var("RUSH_INTEGRATION_TEST", "true");
}

/// Clean up after integration tests
pub fn cleanup_integration_test_env() {
    env::remove_var("RUSH_TEST_MODE");
    env::remove_var("RUSH_INTEGRATION_TEST");
}

/// Runs a command in the given directory and returns the output
pub fn run_command_in_dir(dir: &Path, command: &str, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new(command).args(args).current_dir(dir).output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Command failed: {} {}\nError: {}",
                command,
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            ),
        ))
    }
}

/// Sets up environment variables for integration testing
pub fn setup_test_env() {
    env::set_var("RUSH_TEST_MODE", "true");
    env::set_var("RUSH_LOG_LEVEL", "debug");
}

/// Cleans up environment variables after integration tests
pub fn cleanup_test_env() {
    env::remove_var("RUSH_TEST_MODE");
    env::remove_var("RUSH_LOG_LEVEL");
}

/// Creates a temporary directory for integration testing
pub fn create_temp_dir() -> TempDir {
    tempfile::tempdir().expect("Failed to create temporary directory")
}

/// Creates a temporary product directory with a basic rushd.yaml file
pub fn create_temp_product() -> (TempDir, PathBuf) {
    let temp_dir = create_temp_dir();
    let product_path = temp_dir.path().to_path_buf();

    // Create basic rushd.yaml
    let rushd_yaml = r#"
name: test-product
description: Test product for integration tests
env:
  - name: TEST_ENV
    value: test_value
"#;

    let rushd_path = product_path.join("rushd.yaml");
    let mut file = File::create(rushd_path).unwrap();
    file.write_all(rushd_yaml.as_bytes()).unwrap();

    (temp_dir, product_path)
}
