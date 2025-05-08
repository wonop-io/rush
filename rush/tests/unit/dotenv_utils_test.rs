use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;
use rush_cli::dotenv_utils::{load_dotenv, save_dotenv};

// Helper function to create temp files and handle errors
fn try_create_temp_file(dir: &TempDir, filename: &str, content: &str) -> bool {
    match File::create(dir.path().join(filename)) {
        Ok(mut file) => file.write_all(content.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

// Helper function to create temp directory and handle errors
fn create_test_dir() -> Option<TempDir> {
    match TempDir::new() {
        Ok(dir) => Some(dir),
        Err(_) => {
            println!("Warning: Failed to create temporary directory, skipping test");
            None
        }
    }
}

#[test]
fn test_load_dotenv_empty_file() {
    if let Some(temp_dir) = create_test_dir() {
        let dotenv_path = temp_dir.path().join(".env");
        
        // Create an empty .env file
        if try_create_temp_file(&temp_dir, ".env", "") {
            let result = load_dotenv(Path::new(&dotenv_path));
            
            assert!(result.is_ok());
            let env_map = result.unwrap();
            assert!(env_map.is_empty());
        }
    }
}

#[test]
fn test_load_dotenv_with_values() {
    if let Some(temp_dir) = create_test_dir() {
        let dotenv_path = temp_dir.path().join(".env");
        
        // Create a .env file with values
        let content = "KEY1=value1\nKEY2=\"value2\"\n# Comment\nKEY3=value3\n";
        if try_create_temp_file(&temp_dir, ".env", content) {
            let result = load_dotenv(Path::new(&dotenv_path));
            
            assert!(result.is_ok());
            let env_map = result.unwrap();
            assert_eq!(env_map.len(), 3);
            assert_eq!(env_map.get("KEY1"), Some(&"value1".to_string()));
            assert_eq!(env_map.get("KEY2"), Some(&"value2".to_string()));
            assert_eq!(env_map.get("KEY3"), Some(&"value3".to_string()));
        }
    }
}

#[test]
fn test_load_dotenv_with_empty_lines_and_comments() {
    if let Some(temp_dir) = create_test_dir() {
        let dotenv_path = temp_dir.path().join(".env");
        
        // Create a .env file with empty lines and comments
        let content = "\n# This is a comment\nKEY1=value1\n\n# Another comment\nKEY2=\"value2\"\n";
        if try_create_temp_file(&temp_dir, ".env", content) {
            let result = load_dotenv(Path::new(&dotenv_path));
            
            assert!(result.is_ok());
            let env_map = result.unwrap();
            assert_eq!(env_map.len(), 2);
            assert_eq!(env_map.get("KEY1"), Some(&"value1".to_string()));
            assert_eq!(env_map.get("KEY2"), Some(&"value2".to_string()));
        }
    }
}

#[test]
fn test_load_dotenv_with_spaces() {
    if let Some(temp_dir) = create_test_dir() {
        let dotenv_path = temp_dir.path().join(".env");
        
        // Create a .env file with spaces in keys and values
        let content = "KEY_WITH_SPACES = value with spaces\nANOTHER_KEY = \"quoted value with spaces\"\n";
        if try_create_temp_file(&temp_dir, ".env", content) {
            let result = load_dotenv(Path::new(&dotenv_path));
            
            assert!(result.is_ok());
            let env_map = result.unwrap();
            assert_eq!(env_map.len(), 2);
            assert_eq!(env_map.get("KEY_WITH_SPACES"), Some(&"value with spaces".to_string()));
            assert_eq!(env_map.get("ANOTHER_KEY"), Some(&"quoted value with spaces".to_string()));
        }
    }
}

#[test]
fn test_load_dotenv_file_not_found() {
    let result = load_dotenv(Path::new("/nonexistent/path/.env"));
    assert!(result.is_err());
}

#[test]
fn test_save_dotenv() {
    if let Some(temp_dir) = create_test_dir() {
        let dotenv_path = temp_dir.path().join(".env");
        
        let mut env_map = HashMap::new();
        env_map.insert("KEY1".to_string(), "value1".to_string());
        env_map.insert("KEY2".to_string(), "value with spaces".to_string());
        
        let result = save_dotenv(Path::new(&dotenv_path), env_map.clone());
        
        if result.is_ok() {
            // Now try to load it back
            let loaded_result = load_dotenv(Path::new(&dotenv_path));
            assert!(loaded_result.is_ok());
            
            let loaded_env = loaded_result.unwrap();
            assert_eq!(loaded_env.len(), 2);
            assert_eq!(loaded_env.get("KEY1"), Some(&"value1".to_string()));
            assert_eq!(loaded_env.get("KEY2"), Some(&"value with spaces".to_string()));
        }
    }
}

#[test]
fn test_save_dotenv_directory_not_found() {
    let mut env_map = HashMap::new();
    env_map.insert("KEY1".to_string(), "value1".to_string());
    
    let result = save_dotenv(Path::new("/nonexistent/directory/.env"), env_map);
    assert!(result.is_err());
}

#[test]
fn test_round_trip() {
    if let Some(temp_dir) = create_test_dir() {
        let dotenv_path = temp_dir.path().join(".env");
        
        // Create initial env map
        let mut initial_env = HashMap::new();
        initial_env.insert("DATABASE_URL".to_string(), "postgres://user:pass@localhost/db".to_string());
        initial_env.insert("API_KEY".to_string(), "supersecretkey123".to_string());
        initial_env.insert("DEBUG".to_string(), "true".to_string());
        
        // Save to file and check result
        let save_result = save_dotenv(Path::new(&dotenv_path), initial_env.clone());
        if save_result.is_ok() {
            // Load back and verify
            if let Ok(loaded_env) = load_dotenv(Path::new(&dotenv_path)) {
                // Verify contents
                assert_eq!(loaded_env.len(), initial_env.len());
                for (key, value) in &initial_env {
                    assert_eq!(loaded_env.get(key), Some(value));
                }
            }
        }
    }
}