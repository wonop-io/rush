use std::fs;
use std::path::Path;
use crate::test_utils::TestProjectBuilder;

#[cfg(test)]
mod tests {
    use super::*;

    // This is a basic integration test to make sure the rush config loading works
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

    // Integration test for dotenv functionality
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

    // Integration test for Dockerfile functionality
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

    // Test with custom rushd.yaml
    #[test]
    fn test_custom_rushd_yaml() {
        let custom_rushd = r#"
env:
  - name: CUSTOM_ENV
    value: custom_value
  - name: ANOTHER_ENV
    value: another_value
"#;
        
        let (_temp_dir, project_path) = TestProjectBuilder::new()
            .with_rushd_yaml(custom_rushd)
            .build();
        
        // Check that the file was created with custom content
        let rushd_yaml_path = project_path.join("rushd.yaml");
        assert!(rushd_yaml_path.exists());
        
        // Read the content to verify
        let content = fs::read_to_string(rushd_yaml_path).unwrap();
        assert!(content.contains("CUSTOM_ENV"));
        assert!(content.contains("custom_value"));
        assert!(content.contains("ANOTHER_ENV"));
        assert!(content.contains("another_value"));
    }
}