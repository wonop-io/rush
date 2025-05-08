// run_integration_tests.rs - Integration test runner
//
// This file allows us to run the integration tests by importing them directly.

// Include the dotenv integration tests
#[path = "integration/dotenv_integration_test.rs"]
mod dotenv_tests;

// Include the basic tests
#[cfg(test)]
mod basic_tests {
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};
    use crate::test_utils::TestProjectBuilder;
    use std::io::Write;

    #[test]
    fn test_config_loading() {
        let (_temp_dir, project_path) = TestProjectBuilder::new().build();
        
        // Check that the rushd.yaml file was created
        let rushd_yaml_path = project_path.join("rushd.yaml");
        assert!(rushd_yaml_path.exists());
        
        // Read the content to verify
        let content = fs::read_to_string(rushd_yaml_path).unwrap();
        assert!(content.contains("TEST_ENV"));
        assert!(content.contains("test_value"));
    }

    #[test]
    fn test_dotenv_integration() {
        let (_temp_dir, project_path) = TestProjectBuilder::new()
            .with_dotenv()
            .build();
        
        // Check that the .env file was created
        let dotenv_path = project_path.join(".env");
        assert!(dotenv_path.exists());
        
        // Read the content to verify
        let content = fs::read_to_string(dotenv_path).unwrap();
        assert!(content.contains("TEST_VAR=value"));
        assert!(content.contains("ANOTHER_TEST_VAR=another_value"));
    }

    #[test]
    fn test_dockerfile_integration() {
        let (_temp_dir, project_path) = TestProjectBuilder::new()
            .with_dockerfile()
            .with_docker_compose()
            .build();
        
        // Check that both files were created
        let dockerfile_path = project_path.join("Dockerfile");
        let compose_path = project_path.join("docker-compose.yml");
        
        assert!(dockerfile_path.exists());
        assert!(compose_path.exists());
        
        // Read the content to verify
        let dockerfile_content = fs::read_to_string(dockerfile_path).unwrap();
        assert!(dockerfile_content.contains("FROM alpine:latest"));
        
        let compose_content = fs::read_to_string(compose_path).unwrap();
        assert!(compose_content.contains("test-service:"));
        assert!(compose_content.contains("image: alpine:latest"));
    }
}

// Import the rush_cli crate
extern crate rush_cli;

// Define a module for test utilities
mod test_utils {
    use std::env;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    /// Creates a temporary directory for testing
    pub fn create_temp_dir() -> TempDir {
        tempfile::tempdir().expect("Failed to create temporary directory")
    }
    
    /// Creates a temporary project for testing
    pub fn create_temp_rush_project() -> (TempDir, PathBuf) {
        let temp_dir = create_temp_dir();
        let project_path = temp_dir.path().to_path_buf();
        
        // Create basic project structure
        fs::create_dir_all(project_path.join("src")).unwrap();
        
        // Create a simple rushd.yaml file
        let rushd_yaml = r#"
env:
  - name: TEST_ENV
    value: test_value
"#;
        
        let rushd_path = project_path.join("rushd.yaml");
        let mut file = File::create(rushd_path).unwrap();
        file.write_all(rushd_yaml.as_bytes()).unwrap();
        
        (temp_dir, project_path)
    }
    
    /// Create a dummy Dockerfile for testing
    pub fn create_test_dockerfile(project_dir: &Path) -> PathBuf {
        let dockerfile_content = r#"FROM alpine:latest
RUN apk add --no-cache curl
WORKDIR /app
COPY . .
CMD ["sh", "-c", "echo 'Hello from Rush test'"]"#;
        
        let dockerfile_path = project_dir.join("Dockerfile");
        let mut file = File::create(&dockerfile_path).unwrap();
        file.write_all(dockerfile_content.as_bytes()).unwrap();
        
        dockerfile_path
    }
    
    /// Create a dummy docker-compose file for testing
    pub fn create_test_docker_compose(project_dir: &Path) -> PathBuf {
        let docker_compose_content = r#"version: '3'
services:
  test-service:
    image: alpine:latest
    environment:
      - TEST_VAR=test_value
    command: ["sh", "-c", "echo 'Hello from test container' && sleep 10"]
"#;
        
        let compose_path = project_dir.join("docker-compose.yml");
        let mut file = File::create(&compose_path).unwrap();
        file.write_all(docker_compose_content.as_bytes()).unwrap();
        
        compose_path
    }
    
    /// Create a test .env file
    pub fn create_test_dotenv(project_dir: &Path) -> PathBuf {
        let dotenv_content = r#"TEST_VAR=value
ANOTHER_TEST_VAR=another_value
"#;
        
        let dotenv_path = project_dir.join(".env");
        let mut file = File::create(&dotenv_path).unwrap();
        file.write_all(dotenv_content.as_bytes()).unwrap();
        
        dotenv_path
    }
    
    /// Struct for creating a test project 
    pub struct TestProjectBuilder {
        temp_dir: TempDir,
        project_path: PathBuf,
        has_dockerfile: bool,
        has_docker_compose: bool,
        has_dotenv: bool,
        rushd_yaml_content: String,
    }
    
    impl TestProjectBuilder {
        pub fn new() -> Self {
            let temp_dir = create_temp_dir();
            let project_path = temp_dir.path().to_path_buf();
            fs::create_dir_all(project_path.join("src")).unwrap();
            
            Self {
                temp_dir,
                project_path,
                has_dockerfile: false,
                has_docker_compose: false,
                has_dotenv: false,
                rushd_yaml_content: r#"
env:
  - name: TEST_ENV
    value: test_value
"#.to_string(),
            }
        }
        
        pub fn with_dockerfile(mut self) -> Self {
            create_test_dockerfile(&self.project_path);
            self.has_dockerfile = true;
            self
        }
        
        pub fn with_docker_compose(mut self) -> Self {
            create_test_docker_compose(&self.project_path);
            self.has_docker_compose = true;
            self
        }
        
        pub fn with_dotenv(mut self) -> Self {
            create_test_dotenv(&self.project_path);
            self.has_dotenv = true;
            self
        }
        
        pub fn with_rushd_yaml(mut self, content: &str) -> Self {
            self.rushd_yaml_content = content.to_string();
            self
        }
        
        pub fn build(self) -> (TempDir, PathBuf) {
            // Write rushd.yaml
            let rushd_path = self.project_path.join("rushd.yaml");
            let mut file = File::create(rushd_path).unwrap();
            file.write_all(self.rushd_yaml_content.as_bytes()).unwrap();
            
            (self.temp_dir, self.project_path)
        }
    }
}