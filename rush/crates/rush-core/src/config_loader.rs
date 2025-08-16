use crate::error::{Error, Result};
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

/// Unified configuration loader for various file formats
pub struct ConfigLoader;

impl ConfigLoader {
    /// Load configuration from a YAML file
    pub fn load_yaml<T: DeserializeOwned>(path: &Path) -> Result<T> {
        let content = Self::read_file(path)?;
        serde_yaml::from_str(&content).map_err(|e| {
            Error::Config(format!(
                "Failed to parse YAML from {}: {}",
                path.display(),
                e
            ))
        })
    }

    /// Load configuration from a JSON file
    pub fn load_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
        let content = Self::read_file(path)?;
        serde_json::from_str(&content).map_err(|e| {
            Error::Config(format!(
                "Failed to parse JSON from {}: {}",
                path.display(),
                e
            ))
        })
    }

    /// Load configuration from a TOML file
    pub fn load_toml<T: DeserializeOwned>(path: &Path) -> Result<T> {
        let content = Self::read_file(path)?;
        toml::from_str(&content).map_err(|e| {
            Error::Config(format!(
                "Failed to parse TOML from {}: {}",
                path.display(),
                e
            ))
        })
    }

    /// Load configuration with auto-detection based on file extension
    pub fn load_auto<T: DeserializeOwned>(path: &Path) -> Result<T> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| Error::Config(format!("No file extension for {}", path.display())))?;

        match extension.to_lowercase().as_str() {
            "yaml" | "yml" => Self::load_yaml(path),
            "json" => Self::load_json(path),
            "toml" => Self::load_toml(path),
            _ => Err(Error::Config(format!(
                "Unsupported file extension: {extension}"
            ))),
        }
    }

    /// Read file contents with proper error handling
    fn read_file(path: &Path) -> Result<String> {
        if !path.exists() {
            return Err(Error::Config(format!(
                "Configuration file not found: {}",
                path.display()
            )));
        }

        fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("Failed to read {}: {}", path.display(), e)))
    }

    /// Check if a configuration file exists
    pub fn exists(path: &Path) -> bool {
        path.exists() && path.is_file()
    }

    /// Load configuration with optional fallback
    pub fn load_with_fallback<T: DeserializeOwned>(primary: &Path, fallback: &Path) -> Result<T> {
        if Self::exists(primary) {
            Self::load_auto(primary)
        } else if Self::exists(fallback) {
            Self::load_auto(fallback)
        } else {
            Err(Error::Config(format!(
                "Neither {} nor {} exists",
                primary.display(),
                fallback.display()
            )))
        }
    }

    /// Load and merge multiple configuration files
    pub fn load_merged<T: DeserializeOwned>(paths: &[&Path]) -> Result<T> {
        let mut merged = serde_json::Value::Null;

        for path in paths {
            if Self::exists(path) {
                let value: serde_json::Value = Self::load_auto(path)?;
                if merged.is_null() {
                    merged = value;
                } else {
                    Self::merge_json(&mut merged, value);
                }
            }
        }

        serde_json::from_value(merged)
            .map_err(|e| Error::Config(format!("Failed to deserialize merged config: {e}")))
    }

    /// Merge two JSON values (deep merge for objects)
    fn merge_json(base: &mut serde_json::Value, other: serde_json::Value) {
        match (base, other) {
            (serde_json::Value::Object(base_map), serde_json::Value::Object(other_map)) => {
                for (key, value) in other_map {
                    match base_map.get_mut(&key) {
                        Some(base_value) => Self::merge_json(base_value, value),
                        None => {
                            base_map.insert(key, value);
                        }
                    }
                }
            }
            (base, other) => *base = other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    #[test]
    fn test_load_yaml() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "name: test\nvalue: 42").unwrap();

        let config: TestConfig = ConfigLoader::load_yaml(file.path()).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_load_json() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"name": "test", "value": 42}}"#).unwrap();

        let config: TestConfig = ConfigLoader::load_json(file.path()).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_load_auto_yaml() {
        let file = tempfile::Builder::new().suffix(".yaml").tempfile().unwrap();
        writeln!(file.as_file(), "name: test\nvalue: 42").unwrap();

        let config: TestConfig = ConfigLoader::load_auto(file.path()).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.value, 42);
    }

    #[test]
    fn test_file_not_found() {
        let result = ConfigLoader::load_yaml::<TestConfig>(Path::new("/nonexistent/file.yaml"));
        assert!(result.is_err());
    }
}
