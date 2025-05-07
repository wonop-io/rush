use crate::*;
use std::collections::HashMap;
use std::path::Path;
use tempfile::NamedTempFile;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_dotenv_empty() {
        let temp_file = NamedTempFile::new().unwrap();
        let env_map = dotenv_utils::load_dotenv(temp_file.path()).unwrap();
        assert!(env_map.is_empty());
    }

    #[test]
    fn test_load_dotenv_with_values() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut file = std::fs::File::create(temp_file.path()).unwrap();
        writeln!(file, "KEY1=value1").unwrap();
        writeln!(file, "KEY2=").unwrap();
        writeln!(file, "KEY3=\"quoted value\"").unwrap();
        writeln!(file, "# comment").unwrap();
        writeln!(file, "").unwrap();
        writeln!(file, "KEY4=value with spaces").unwrap();
        
        let env_map = dotenv_utils::load_dotenv(temp_file.path()).unwrap();
        
        assert_eq!(env_map.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(env_map.get("KEY2"), Some(&"".to_string()));
        assert_eq!(env_map.get("KEY3"), Some(&"quoted value".to_string()));
        assert_eq!(env_map.get("KEY4"), Some(&"value with spaces".to_string()));
        assert_eq!(env_map.len(), 4);
    }

    #[test]
    fn test_save_dotenv() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut env_map = HashMap::new();
        env_map.insert("TEST_KEY1".to_string(), "test value 1".to_string());
        env_map.insert("TEST_KEY2".to_string(), "test value 2".to_string());
        
        dotenv_utils::save_dotenv(temp_file.path(), env_map).unwrap();
        
        let loaded_env = dotenv_utils::load_dotenv(temp_file.path()).unwrap();
        assert_eq!(loaded_env.get("TEST_KEY1"), Some(&"test value 1".to_string()));
        assert_eq!(loaded_env.get("TEST_KEY2"), Some(&"test value 2".to_string()));
        assert_eq!(loaded_env.len(), 2);
    }

    #[test]
    fn test_load_dotenv_nonexistent_file() {
        let result = dotenv_utils::load_dotenv(Path::new("/nonexistent/file.env"));
        assert!(result.is_err());
    }
}