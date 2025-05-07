use crate::rush_cli::dotenv_utils::{load_dotenv, save_dotenv};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_dotenv_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let dotenv_path = temp_dir.path().join(".env");
    
    // Create a map of environment variables
    let mut env_map = HashMap::new();
    env_map.insert("TEST_VAR".to_string(), "test value".to_string());
    env_map.insert("ANOTHER_VAR".to_string(), "12345".to_string());
    env_map.insert("EMPTY_VAR".to_string(), "".to_string());
    
    // Save to .env file
    save_dotenv(&dotenv_path, env_map.clone()).unwrap();
    
    // Verify the file was created
    assert!(dotenv_path.exists());
    
    // Read the file content and check it
    let content = fs::read_to_string(&dotenv_path).unwrap();
    assert!(content.contains("TEST_VAR=\"test value\""));
    assert!(content.contains("ANOTHER_VAR=\"12345\""));
    assert!(content.contains("EMPTY_VAR=\"\""));
    
    // Load the .env file back
    let loaded_env = load_dotenv(&dotenv_path).unwrap();
    
    // Verify all values were loaded correctly
    assert_eq!(loaded_env.len(), 3);
    assert_eq!(loaded_env.get("TEST_VAR"), Some(&"test value".to_string()));
    assert_eq!(loaded_env.get("ANOTHER_VAR"), Some(&"12345".to_string()));
    assert_eq!(loaded_env.get("EMPTY_VAR"), Some(&"".to_string()));
}

#[test]
fn test_dotenv_complex_values() {
    let temp_dir = TempDir::new().unwrap();
    let dotenv_path = temp_dir.path().join(".env");
    
    // Create a test .env file with complex values
    let content = r#"BASIC=value
# Comment line
COMMENTED_VALUE=has # hash
EMPTY=
QUOTED="quoted value"
SPACES=value with spaces
MULTI_LINE="line 1
line 2"
LINE_BREAKS=break\nhere

"#;
    
    fs::write(&dotenv_path, content).unwrap();
    
    // Load the .env file
    let loaded_env = load_dotenv(&dotenv_path).unwrap();
    
    // Verify values were parsed correctly
    assert_eq!(loaded_env.get("BASIC"), Some(&"value".to_string()));
    assert_eq!(loaded_env.get("COMMENTED_VALUE"), Some(&"has # hash".to_string())); 
    assert_eq!(loaded_env.get("EMPTY"), Some(&"".to_string()));
    assert_eq!(loaded_env.get("QUOTED"), Some(&"quoted value".to_string()));
    assert_eq!(loaded_env.get("SPACES"), Some(&"value with spaces".to_string()));
    assert_eq!(loaded_env.get("MULTI_LINE"), Some(&"line 1\nline 2".to_string()));
    assert_eq!(loaded_env.get("LINE_BREAKS"), Some(&"break\\nhere".to_string()));
}

#[test]
fn test_dotenv_missing_file() {
    let temp_dir = TempDir::new().unwrap();
    let nonexistent_path = temp_dir.path().join("nonexistent.env");
    
    // Attempt to load a non-existent file
    let result = load_dotenv(&nonexistent_path);
    assert!(result.is_err());
}

#[test]
fn test_dotenv_save_to_nonexistent_directory() {
    let temp_dir = TempDir::new().unwrap();
    let nested_dir = temp_dir.path().join("nested/dir");
    let dotenv_path = nested_dir.join(".env");
    
    // Create an environment map
    let mut env_map = HashMap::new();
    env_map.insert("TEST_VAR".to_string(), "test value".to_string());
    
    // This should fail because the directory doesn't exist
    let result = save_dotenv(&dotenv_path, env_map);
    assert!(result.is_err());
    
    // Create the directory
    fs::create_dir_all(&nested_dir).unwrap();
    
    // Now it should succeed
    let mut env_map = HashMap::new();
    env_map.insert("TEST_VAR".to_string(), "test value".to_string());
    let result = save_dotenv(&dotenv_path, env_map);
    assert!(result.is_ok());
    assert!(dotenv_path.exists());
}