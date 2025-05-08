use std::collections::HashMap;
use tempfile::TempDir;
use std::fs::File;
use std::io::Write;
use rush_cli::builder::Variables;

#[test]
fn test_variables_new_with_valid_file() {
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let variables_path = temp_dir.path().join("variables.yaml");
    
    let yaml_content = r#"
dev:
  API_URL: "https://dev-api.example.com"
  DEBUG: "true"
staging:
  API_URL: "https://staging-api.example.com"
  DEBUG: "false"
prod:
  API_URL: "https://api.example.com"
  DEBUG: "false"
local:
  API_URL: "http://localhost:8080"
  DEBUG: "true"
"#;
    
    let mut file = File::create(&variables_path).expect("Failed to create variables file");
    file.write_all(yaml_content.as_bytes()).expect("Failed to write variables content");
    
    let variables = Variables::new(
        variables_path.to_str().unwrap(),
        "dev"
    );
    
    assert_eq!(variables.env, "dev");
    assert_eq!(variables.get("API_URL"), Some("https://dev-api.example.com".to_string()));
    assert_eq!(variables.get("DEBUG"), Some("true".to_string()));
}

#[test]
fn test_variables_new_with_invalid_file() {
    let non_existent_path = "non_existent_file.yaml";
    
    let variables = Variables::new(non_existent_path, "prod");
    
    // Should return an empty HashMap when file doesn't exist
    assert_eq!(variables.env, "prod");
    assert_eq!(variables.get("ANY_KEY"), None);
}

#[test]
fn test_variables_get_from_different_environments() {
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let variables_path = temp_dir.path().join("variables.yaml");
    
    let yaml_content = r#"
dev:
  API_KEY: "dev-key"
staging:
  API_KEY: "staging-key"
prod:
  API_KEY: "prod-key"
local:
  API_KEY: "local-key"
"#;
    
    let mut file = File::create(&variables_path).expect("Failed to create variables file");
    file.write_all(yaml_content.as_bytes()).expect("Failed to write variables content");
    
    // Test dev environment
    let dev_variables = Variables::new(
        variables_path.to_str().unwrap(),
        "dev"
    );
    assert_eq!(dev_variables.get("API_KEY"), Some("dev-key".to_string()));
    
    // Test staging environment
    let staging_variables = Variables::new(
        variables_path.to_str().unwrap(),
        "staging"
    );
    assert_eq!(staging_variables.get("API_KEY"), Some("staging-key".to_string()));
    
    // Test prod environment
    let prod_variables = Variables::new(
        variables_path.to_str().unwrap(),
        "prod"
    );
    assert_eq!(prod_variables.get("API_KEY"), Some("prod-key".to_string()));
    
    // Test local environment
    let local_variables = Variables::new(
        variables_path.to_str().unwrap(),
        "local"
    );
    assert_eq!(local_variables.get("API_KEY"), Some("local-key".to_string()));
}

#[test]
fn test_variables_get_with_invalid_environment() {
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let variables_path = temp_dir.path().join("variables.yaml");
    
    let yaml_content = r#"
dev:
  API_KEY: "dev-key"
staging:
  API_KEY: "staging-key"
prod:
  API_KEY: "prod-key"
local:
  API_KEY: "local-key"
"#;
    
    let mut file = File::create(&variables_path).expect("Failed to create variables file");
    file.write_all(yaml_content.as_bytes()).expect("Failed to write variables content");
    
    // Test invalid environment
    let invalid_variables = Variables::new(
        variables_path.to_str().unwrap(),
        "invalid"
    );
    assert_eq!(invalid_variables.get("API_KEY"), None);
}

#[test]
fn test_variables_get_nonexistent_key() {
    let temp_dir = TempDir::new().expect("Failed to create temporary directory");
    let variables_path = temp_dir.path().join("variables.yaml");
    
    let yaml_content = r#"
dev:
  API_KEY: "dev-key"
"#;
    
    let mut file = File::create(&variables_path).expect("Failed to create variables file");
    file.write_all(yaml_content.as_bytes()).expect("Failed to write variables content");
    
    let variables = Variables::new(
        variables_path.to_str().unwrap(),
        "dev"
    );
    
    assert_eq!(variables.get("NON_EXISTENT_KEY"), None);
}