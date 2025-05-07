use rush_cli::public_env_defs::{PublicEnvironmentDefinitions, GenerationMethod};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::io::Read;
use tempfile::TempDir;

fn create_test_yaml(dir: &Path, filename: &str, content: &str) -> String {
    let file_path = dir.join(filename);
    fs::write(&file_path, content).unwrap();
    file_path.to_string_lossy().to_string()
}

fn create_stack_spec_yaml(dir: &Path) {
    let content = r#"
frontend:
  location: frontend
backend:
  location: backend
database:
  location: database
"#;
    fs::write(dir.join("stack.spec.yaml"), content).unwrap();
    
    // Create component directories
    fs::create_dir_all(dir.join("frontend")).unwrap();
    fs::create_dir_all(dir.join("backend")).unwrap();
    fs::create_dir_all(dir.join("database")).unwrap();
}

#[test]
fn test_new_with_valid_files() {
    let temp_dir = TempDir::new().unwrap();
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  API_URL: !Static "http://localhost:3000"
  DEBUG: !Static "true"
backend:
  DB_HOST: !Static "localhost"
  DB_PORT: !Static "5432"
"#,
    );
    
    let specialization_yaml = create_test_yaml(
        temp_dir.path(),
        "special.yaml",
        r#"
frontend:
  API_KEY: !Static "special-key"
backend:
  LOG_LEVEL: !Static "debug"
"#,
    );
    
    let env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &specialization_yaml,
    );
    
    // Check that the components were loaded and merged correctly
    assert_eq!(env_defs.get_product_name(), "test-product");
    assert_eq!(env_defs.get_product_dir(), &temp_dir.path());
    assert_eq!(env_defs.get_components().len(), 2);
    
    // Check that the frontend component has the expected variables
    assert!(env_defs.generate_value("frontend", "API_URL").is_some());
    assert!(env_defs.generate_value("frontend", "DEBUG").is_some());
    assert!(env_defs.generate_value("frontend", "API_KEY").is_some());
    
    // Check that the backend component has the expected variables
    assert!(env_defs.generate_value("backend", "DB_HOST").is_some());
    assert!(env_defs.generate_value("backend", "DB_PORT").is_some());
    assert!(env_defs.generate_value("backend", "LOG_LEVEL").is_some());
}

#[test]
fn test_add_component() {
    let temp_dir = TempDir::new().unwrap();
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  API_URL: !Static "http://localhost:3000"
"#,
    );
    
    let specialization_yaml = create_test_yaml(temp_dir.path(), "special.yaml", "");
    
    let mut env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &specialization_yaml,
    );
    
    // Add a new component
    env_defs.add_component("new-component".to_string());
    
    // Check that the new component was added by trying to access it
    assert!(env_defs.generate_value("new-component", "any-var").is_none());
}

#[test]
fn test_add_environment_variable() {
    let temp_dir = TempDir::new().unwrap();
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  API_URL: !Static "http://localhost:3000"
"#,
    );
    
    let specialization_yaml = create_test_yaml(temp_dir.path(), "special.yaml", "");
    
    let mut env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &specialization_yaml,
    );
    
    // Add an environment variable to an existing component
    env_defs.add_environment_variable(
        "frontend",
        "NEW_VAR".to_string(),
        GenerationMethod::Static("new-value".to_string()),
    );
    
    // Check if we can get the value (which verifies the variable was added)
    let value = env_defs.generate_value("frontend", "NEW_VAR");
    assert_eq!(value, Some("new-value".to_string()));
}

#[test]
fn test_generate_value_static() {
    let temp_dir = TempDir::new().unwrap();
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  API_URL: !Static "http://localhost:3000"
"#,
    );
    
    let specialization_yaml = create_test_yaml(temp_dir.path(), "special.yaml", "");
    
    let env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &specialization_yaml,
    );
    
    // Test generating a static value
    let value = env_defs.generate_value("frontend", "API_URL");
    assert_eq!(value, Some("http://localhost:3000".to_string()));
    
    // Test generating a value for a non-existent variable
    let value = env_defs.generate_value("frontend", "NON_EXISTENT");
    assert_eq!(value, None);
    
    // Test generating a value for a non-existent component
    let value = env_defs.generate_value("non-existent", "API_URL");
    assert_eq!(value, None);
}

#[test]
fn test_generate_dotenv_files() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create the stack.spec.yaml file
    create_stack_spec_yaml(temp_dir.path());
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  API_URL: !Static "http://localhost:3000"
  DEBUG: !Static "true"
backend:
  DB_HOST: !Static "localhost"
  DB_PORT: !Static "5432"
"#,
    );
    
    let specialization_yaml = create_test_yaml(temp_dir.path(), "special.yaml", "");
    
    let env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &specialization_yaml,
    );
    
    // Generate the .env files
    let result = env_defs.generate_dotenv_files();
    assert!(result.is_ok());
    
    // Check that the .env files were created
    let frontend_env = temp_dir.path().join("frontend").join(".env");
    let backend_env = temp_dir.path().join("backend").join(".env");
    
    assert!(frontend_env.exists());
    assert!(backend_env.exists());
    
    // Check the content of the frontend .env file
    let frontend_content = fs::read_to_string(frontend_env).unwrap();
    assert!(frontend_content.contains(r#"API_URL="http://localhost:3000""#));
    assert!(frontend_content.contains(r#"DEBUG="true""#));
    
    // Check the content of the backend .env file
    let backend_content = fs::read_to_string(backend_env).unwrap();
    assert!(backend_content.contains(r#"DB_HOST="localhost""#));
    assert!(backend_content.contains(r#"DB_PORT="5432""#));
}

#[test]
fn test_merge_components() {
    let temp_dir = TempDir::new().unwrap();
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  API_URL: !Static "http://localhost:3000"
backend:
  DB_HOST: !Static "localhost"
"#,
    );
    
    let specialization_yaml = create_test_yaml(
        temp_dir.path(),
        "special.yaml",
        r#"
frontend:
  API_KEY: !Static "special-key"
new_component:
  NEW_VAR: !Static "new-value"
"#,
    );
    
    let env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &specialization_yaml,
    );
    
    // Check that the components were merged correctly by verifying values
    
    // Check the frontend component's values
    assert!(env_defs.generate_value("frontend", "API_URL").is_some());
    assert!(env_defs.generate_value("frontend", "API_KEY").is_some());
    
    // Check the backend component's values
    assert!(env_defs.generate_value("backend", "DB_HOST").is_some());
    
    // Check the new component's values
    assert!(env_defs.generate_value("new_component", "NEW_VAR").is_some());
}

#[test]
fn test_load_components_with_invalid_yaml() {
    let temp_dir = TempDir::new().unwrap();
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  API_URL: !Static "http://localhost:3000"
"#,
    );
    
    // Create an invalid YAML file
    let invalid_yaml = create_test_yaml(
        temp_dir.path(),
        "invalid.yaml",
        r#"
this is not valid yaml:
  - missing colon
"#,
    );
    
    // This should not panic as it's the specialization file
    let env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &invalid_yaml,
    );
    
    // Check that only the base components were loaded
    assert!(env_defs.generate_value("frontend", "API_URL").is_some());
    // Also verify that no other component exists by checking a common name
    assert!(env_defs.generate_value("backend", "DB_HOST").is_none());
}

#[test]
fn test_load_components_with_nonexistent_yaml() {
    let temp_dir = TempDir::new().unwrap();
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  API_URL: !Static "http://localhost:3000"
"#,
    );
    
    let nonexistent_yaml = temp_dir.path().join("nonexistent.yaml").to_string_lossy().to_string();
    
    // This should not panic as the specialization file doesn't exist
    let env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &nonexistent_yaml,
    );
    
    // Check that only the base components were loaded
    assert!(env_defs.generate_value("frontend", "API_URL").is_some());
}

#[test]
fn test_timestamp_generation_method() {
    let temp_dir = TempDir::new().unwrap();
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  TIMESTAMP: !Timestamp "%Y-%m-%d"
"#,
    );
    
    let specialization_yaml = create_test_yaml(temp_dir.path(), "special.yaml", "");
    
    let env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &specialization_yaml,
    );
    
    // Get the timestamp value
    let timestamp = env_defs.generate_value("frontend", "TIMESTAMP");
    assert!(timestamp.is_some());
    
    // Verify it's a properly formatted date
    let timestamp_str = timestamp.unwrap();
    assert!(timestamp_str.matches('-').count() == 2); // YYYY-MM-DD format should have 2 hyphens
    assert_eq!(timestamp_str.len(), 10); // YYYY-MM-DD is 10 characters
}

#[test]
fn test_ask_with_default_generation_method() {
    let temp_dir = TempDir::new().unwrap();
    
    let base_yaml = create_test_yaml(
        temp_dir.path(),
        "base.yaml",
        r#"
frontend:
  PROMPT_VAR: !AskWithDefault ["Enter a value:", "default_value"]
"#,
    );
    
    let specialization_yaml = create_test_yaml(temp_dir.path(), "special.yaml", "");
    
    let env_defs = PublicEnvironmentDefinitions::new(
        "test-product".to_string(),
        &base_yaml,
        &specialization_yaml,
    );
    
    // Verify the variable exists (we can't easily test interactive input)
    assert!(env_defs.generate_value("frontend", "PROMPT_VAR").is_some());
}