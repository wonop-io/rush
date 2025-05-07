use rush_cli::dotenv_utils::{load_dotenv, save_dotenv};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_load_dotenv_empty_file() {
    let temp_dir = TempDir::new().unwrap();
    let dotenv_path = temp_dir.path().join(".env");
    File::create(&dotenv_path).unwrap();

    let result = load_dotenv(&dotenv_path).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_load_dotenv_with_comments_and_empty_lines() {
    let temp_dir = TempDir::new().unwrap();
    let dotenv_path = temp_dir.path().join(".env");
    let content = r#"
# This is a comment
TEST_KEY=test_value

# Another comment
  
TEST_KEY2=test_value2
"#;
    fs::write(&dotenv_path, content).unwrap();

    let result = load_dotenv(&dotenv_path).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result.get("TEST_KEY"), Some(&"test_value".to_string()));
    assert_eq!(result.get("TEST_KEY2"), Some(&"test_value2".to_string()));
}

#[test]
fn test_load_dotenv_with_quoted_values() {
    let temp_dir = TempDir::new().unwrap();
    let dotenv_path = temp_dir.path().join(".env");
    let content = r#"
QUOTED_VALUE="This is a quoted value"
UNQUOTED_VALUE=This is an unquoted value
"#;
    fs::write(&dotenv_path, content).unwrap();

    let result = load_dotenv(&dotenv_path).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(
        result.get("QUOTED_VALUE"),
        Some(&"This is a quoted value".to_string())
    );
    assert_eq!(
        result.get("UNQUOTED_VALUE"),
        Some(&"This is an unquoted value".to_string())
    );
}

#[test]
fn test_load_dotenv_with_spaces() {
    let temp_dir = TempDir::new().unwrap();
    let dotenv_path = temp_dir.path().join(".env");
    let content = r#"
  SPACE_BEFORE = value_with_space
KEY_WITH_SPACE = "value with space"
"#;
    fs::write(&dotenv_path, content).unwrap();

    let result = load_dotenv(&dotenv_path).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(
        result.get("SPACE_BEFORE"),
        Some(&"value_with_space".to_string())
    );
    assert_eq!(
        result.get("KEY_WITH_SPACE"),
        Some(&"value with space".to_string())
    );
}

#[test]
fn test_load_dotenv_file_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let dotenv_path = temp_dir.path().join("nonexistent.env");

    let result = load_dotenv(&dotenv_path);
    assert!(result.is_err());
}

#[test]
fn test_save_dotenv() {
    let temp_dir = TempDir::new().unwrap();
    let dotenv_path = temp_dir.path().join(".env");

    let mut env_map = HashMap::new();
    env_map.insert("KEY1".to_string(), "value1".to_string());
    env_map.insert("KEY2".to_string(), "value with spaces".to_string());

    let result = save_dotenv(&dotenv_path, env_map);
    assert!(result.is_ok());

    // Now read the file and check the content
    let content = fs::read_to_string(&dotenv_path).unwrap();
    assert!(content.contains(r#"KEY1="value1""#));
    assert!(content.contains(r#"KEY2="value with spaces""#));
}

#[test]
fn test_save_and_load_dotenv() {
    let temp_dir = TempDir::new().unwrap();
    let dotenv_path = temp_dir.path().join(".env");

    let mut env_map = HashMap::new();
    env_map.insert("TEST_KEY".to_string(), "test_value".to_string());
    env_map.insert("COMPLEX_KEY".to_string(), "value with \"quotes\" inside".to_string());

    save_dotenv(&dotenv_path, env_map).unwrap();

    let loaded_map = load_dotenv(&dotenv_path).unwrap();
    assert_eq!(loaded_map.len(), 2);
    assert_eq!(loaded_map.get("TEST_KEY"), Some(&"test_value".to_string()));
    assert_eq!(
        loaded_map.get("COMPLEX_KEY"),
        Some(&r#"value with "quotes" inside"#.to_string())
    );
}

#[test]
fn test_save_dotenv_to_nonexistent_directory() {
    let temp_dir = TempDir::new().unwrap();
    let nonexistent_dir = temp_dir.path().join("nonexistent");
    let dotenv_path = nonexistent_dir.join(".env");

    let mut env_map = HashMap::new();
    env_map.insert("KEY1".to_string(), "value1".to_string());

    let result = save_dotenv(&dotenv_path, env_map);
    assert!(result.is_err());
}