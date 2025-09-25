//! Configuration repository pattern for centralized config management
//!
//! This module provides a unified interface for managing all Rush configurations
//! with versioning, validation, and hot-reload capabilities.

use std::collections::HashMap;
use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
#[cfg(test)]
use serde::Serialize;
use serde_json::Value as JsonValue;
use tokio::fs;
use tokio::sync::RwLock;

use crate::{Error, Result};

/// Configuration source types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    File(PathBuf),
    Environment,
    Memory,
    Remote(String),
}

/// Configuration entry with metadata
#[derive(Debug, Clone)]
pub struct ConfigEntry {
    /// Configuration key
    pub key: String,
    /// Configuration value as JSON
    pub value: JsonValue,
    /// Source of the configuration
    pub source: ConfigSource,
    /// Version/revision number
    pub version: u64,
    /// Last modified timestamp
    pub last_modified: std::time::SystemTime,
    /// Optional schema for validation
    pub schema: Option<JsonValue>,
}

/// Configuration change event
#[derive(Debug, Clone)]
pub enum ConfigChange {
    Added(String),
    Updated(String),
    Removed(String),
    Reloaded,
}

/// Trait for configuration watchers
#[async_trait]
pub trait ConfigWatcher: Send + Sync {
    /// Called when configuration changes
    async fn on_change(&self, change: ConfigChange, entry: Option<&ConfigEntry>);
}

/// Trait for configuration validators
pub trait ConfigValidator: Send + Sync {
    /// Validate a configuration entry
    fn validate(&self, entry: &ConfigEntry) -> Result<()>;
}

/// Trait for configuration loaders
#[async_trait]
pub trait ConfigLoader: Send + Sync {
    /// Load configuration from source
    async fn load(&self, source: &ConfigSource) -> Result<HashMap<String, JsonValue>>;

    /// Watch for changes in the source
    async fn watch(&self, source: &ConfigSource) -> Result<()>;
}

/// File-based configuration loader
pub struct FileConfigLoader;

#[async_trait]
impl ConfigLoader for FileConfigLoader {
    async fn load(&self, source: &ConfigSource) -> Result<HashMap<String, JsonValue>> {
        match source {
            ConfigSource::File(path) => {
                let content = fs::read_to_string(path)
                    .await
                    .map_err(|e| Error::Internal(format!("Failed to read config file: {e}")))?;

                let value: JsonValue = if path.extension() == Some(std::ffi::OsStr::new("yaml"))
                    || path.extension() == Some(std::ffi::OsStr::new("yml"))
                {
                    serde_yaml::from_str(&content)
                        .map_err(|e| Error::Internal(format!("Failed to parse YAML: {e}")))?
                } else {
                    serde_json::from_str(&content)
                        .map_err(|e| Error::Internal(format!("Failed to parse JSON: {e}")))?
                };

                let mut configs = HashMap::new();
                if let Some(obj) = value.as_object() {
                    for (key, val) in obj {
                        configs.insert(key.clone(), val.clone());
                    }
                }
                Ok(configs)
            }
            _ => Ok(HashMap::new()),
        }
    }

    async fn watch(&self, _source: &ConfigSource) -> Result<()> {
        // TODO: Implement file watching
        Ok(())
    }
}

/// Environment-based configuration loader
pub struct EnvConfigLoader {
    prefix: String,
}

impl EnvConfigLoader {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

#[async_trait]
impl ConfigLoader for EnvConfigLoader {
    async fn load(&self, source: &ConfigSource) -> Result<HashMap<String, JsonValue>> {
        match source {
            ConfigSource::Environment => {
                let mut configs = HashMap::new();
                for (key, value) in std::env::vars() {
                    if key.starts_with(&self.prefix) {
                        let config_key = key
                            .strip_prefix(&self.prefix)
                            .unwrap()
                            .to_lowercase()
                            .replace('_', ".");
                        configs.insert(config_key, JsonValue::String(value));
                    }
                }
                Ok(configs)
            }
            _ => Ok(HashMap::new()),
        }
    }

    async fn watch(&self, _source: &ConfigSource) -> Result<()> {
        // Environment variables don't change during runtime
        Ok(())
    }
}

/// Configuration repository
pub struct ConfigRepository {
    /// Configuration entries
    entries: Arc<RwLock<HashMap<String, ConfigEntry>>>,
    /// Configuration loaders
    loaders: Arc<RwLock<Vec<Box<dyn ConfigLoader>>>>,
    /// Configuration validators
    validators: Arc<RwLock<Vec<Box<dyn ConfigValidator>>>>,
    /// Configuration watchers
    watchers: Arc<RwLock<Vec<Box<dyn ConfigWatcher>>>>,
    /// Current version counter
    version_counter: Arc<RwLock<u64>>,
}

impl ConfigRepository {
    /// Create a new configuration repository
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            loaders: Arc::new(RwLock::new(Vec::new())),
            validators: Arc::new(RwLock::new(Vec::new())),
            watchers: Arc::new(RwLock::new(Vec::new())),
            version_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Register a configuration loader
    pub async fn register_loader(&self, loader: Box<dyn ConfigLoader>) {
        let mut loaders = self.loaders.write().await;
        loaders.push(loader);
    }

    /// Register a configuration validator
    pub async fn register_validator(&self, validator: Box<dyn ConfigValidator>) {
        let mut validators = self.validators.write().await;
        validators.push(validator);
    }

    /// Register a configuration watcher
    pub async fn register_watcher(&self, watcher: Box<dyn ConfigWatcher>) {
        let mut watchers = self.watchers.write().await;
        watchers.push(watcher);
    }

    /// Load configuration from a source
    pub async fn load_from(&self, source: ConfigSource) -> Result<()> {
        let loaders = self.loaders.read().await;

        for loader in loaders.iter() {
            let configs = loader.load(&source).await?;

            for (key, value) in configs {
                self.set_with_source(key, value, source.clone()).await?;
            }
        }

        // Notify watchers
        let watchers = self.watchers.read().await;
        for watcher in watchers.iter() {
            watcher.on_change(ConfigChange::Reloaded, None).await;
        }

        Ok(())
    }

    /// Get a configuration value
    pub async fn get(&self, key: &str) -> Option<JsonValue> {
        let entries = self.entries.read().await;
        entries.get(key).map(|e| e.value.clone())
    }

    /// Get a typed configuration value
    pub async fn get_typed<T>(&self, key: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let value = self
            .get(key)
            .await
            .ok_or_else(|| Error::Internal(format!("Configuration key '{key}' not found")))?;

        serde_json::from_value(value)
            .map_err(|e| Error::Internal(format!("Failed to deserialize config: {e}")))
    }

    /// Set a configuration value
    pub async fn set(&self, key: impl Into<String>, value: JsonValue) -> Result<()> {
        self.set_with_source(key, value, ConfigSource::Memory).await
    }

    /// Set a configuration value with source
    async fn set_with_source(
        &self,
        key: impl Into<String>,
        value: JsonValue,
        source: ConfigSource,
    ) -> Result<()> {
        let key = key.into();
        let mut version_counter = self.version_counter.write().await;
        *version_counter += 1;
        let version = *version_counter;

        let entry = ConfigEntry {
            key: key.clone(),
            value: value.clone(),
            source,
            version,
            last_modified: std::time::SystemTime::now(),
            schema: None,
        };

        // Validate
        let validators = self.validators.read().await;
        for validator in validators.iter() {
            validator.validate(&entry)?;
        }

        // Store
        let mut entries = self.entries.write().await;
        let is_update = entries.contains_key(&key);
        entries.insert(key.clone(), entry.clone());

        // Notify watchers
        let change = if is_update {
            ConfigChange::Updated(key)
        } else {
            ConfigChange::Added(key)
        };

        let watchers = self.watchers.read().await;
        for watcher in watchers.iter() {
            watcher.on_change(change.clone(), Some(&entry)).await;
        }

        Ok(())
    }

    /// Remove a configuration value
    pub async fn remove(&self, key: &str) -> Result<()> {
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.remove(key) {
            // Notify watchers
            let watchers = self.watchers.read().await;
            for watcher in watchers.iter() {
                watcher
                    .on_change(ConfigChange::Removed(key.to_string()), Some(&entry))
                    .await;
            }
        }
        Ok(())
    }

    /// List all configuration keys
    pub async fn list_keys(&self) -> Vec<String> {
        let entries = self.entries.read().await;
        entries.keys().cloned().collect()
    }

    /// Get all configurations
    pub async fn get_all(&self) -> HashMap<String, JsonValue> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .map(|(k, v)| (k.clone(), v.value.clone()))
            .collect()
    }

    /// Merge configurations from another repository
    pub async fn merge(&self, other: &ConfigRepository) -> Result<()> {
        let other_entries = other.entries.read().await;
        for (key, entry) in other_entries.iter() {
            self.set_with_source(key.clone(), entry.value.clone(), entry.source.clone())
                .await?;
        }
        Ok(())
    }

    /// Export configurations to JSON
    pub async fn export_json(&self) -> JsonValue {
        let entries = self.entries.read().await;
        let configs: HashMap<String, JsonValue> = entries
            .iter()
            .map(|(k, v)| (k.clone(), v.value.clone()))
            .collect();
        serde_json::to_value(configs).unwrap_or(JsonValue::Null)
    }

    /// Import configurations from JSON
    pub async fn import_json(&self, json: JsonValue) -> Result<()> {
        if let Some(obj) = json.as_object() {
            for (key, value) in obj {
                self.set(key.clone(), value.clone()).await?;
            }
        }
        Ok(())
    }
}

impl Default for ConfigRepository {
    fn default() -> Self {
        Self::new()
    }
}

/// Schema-based configuration validator
pub struct SchemaValidator {
    schemas: HashMap<String, JsonValue>,
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaValidator {
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    pub fn add_schema(&mut self, pattern: impl Into<String>, schema: JsonValue) {
        self.schemas.insert(pattern.into(), schema);
    }
}

impl ConfigValidator for SchemaValidator {
    fn validate(&self, entry: &ConfigEntry) -> Result<()> {
        // Find matching schema
        for pattern in self.schemas.keys() {
            if entry.key.starts_with(pattern) {
                // TODO: Implement JSON schema validation
                // For now, just pass
                return Ok(());
            }
        }
        Ok(())
    }
}

/// Logging configuration watcher
pub struct LoggingWatcher;

#[async_trait]
impl ConfigWatcher for LoggingWatcher {
    async fn on_change(&self, change: ConfigChange, entry: Option<&ConfigEntry>) {
        match change {
            ConfigChange::Added(key) => {
                log::info!("Configuration added: {}", key);
            }
            ConfigChange::Updated(key) => {
                log::info!("Configuration updated: {}", key);
            }
            ConfigChange::Removed(key) => {
                log::info!("Configuration removed: {}", key);
            }
            ConfigChange::Reloaded => {
                log::info!("Configuration reloaded");
            }
        }

        if let Some(entry) = entry {
            log::debug!("  Source: {:?}", entry.source);
            log::debug!("  Version: {}", entry.version);
        }
    }
}

/// Global configuration repository instance
static CONFIG_REPOSITORY: once_cell::sync::Lazy<ConfigRepository> =
    once_cell::sync::Lazy::new(ConfigRepository::new);

/// Get the global configuration repository
pub fn config_repository() -> &'static ConfigRepository {
    &CONFIG_REPOSITORY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_operations() {
        let repo = ConfigRepository::new();

        // Set a value
        repo.set("test.key", JsonValue::String("value".to_string()))
            .await
            .unwrap();

        // Get the value
        let value = repo.get("test.key").await;
        assert_eq!(value, Some(JsonValue::String("value".to_string())));

        // Update the value
        repo.set("test.key", JsonValue::String("new_value".to_string()))
            .await
            .unwrap();
        let value = repo.get("test.key").await;
        assert_eq!(value, Some(JsonValue::String("new_value".to_string())));

        // Remove the value
        repo.remove("test.key").await.unwrap();
        let value = repo.get("test.key").await;
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_typed_access() {
        let repo = ConfigRepository::new();

        #[derive(Debug, Deserialize, Serialize, PartialEq)]
        struct TestConfig {
            name: String,
            port: u16,
        }

        let config = TestConfig {
            name: "test".to_string(),
            port: 8080,
        };

        let json = serde_json::to_value(&config).unwrap();
        repo.set("app.config", json).await.unwrap();

        let loaded: TestConfig = repo.get_typed("app.config").await.unwrap();
        assert_eq!(loaded, config);
    }

    #[tokio::test]
    async fn test_list_keys() {
        let repo = ConfigRepository::new();

        repo.set("key1", JsonValue::String("value1".to_string()))
            .await
            .unwrap();
        repo.set("key2", JsonValue::String("value2".to_string()))
            .await
            .unwrap();
        repo.set("key3", JsonValue::String("value3".to_string()))
            .await
            .unwrap();

        let keys = repo.list_keys().await;
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"key1".to_string()));
        assert!(keys.contains(&"key2".to_string()));
        assert!(keys.contains(&"key3".to_string()));
    }

    #[tokio::test]
    async fn test_export_import() {
        let repo1 = ConfigRepository::new();
        repo1
            .set("test.a", JsonValue::String("a".to_string()))
            .await
            .unwrap();
        repo1
            .set("test.b", JsonValue::String("b".to_string()))
            .await
            .unwrap();

        let exported = repo1.export_json().await;

        let repo2 = ConfigRepository::new();
        repo2.import_json(exported).await.unwrap();

        assert_eq!(
            repo2.get("test.a").await,
            Some(JsonValue::String("a".to_string()))
        );
        assert_eq!(
            repo2.get("test.b").await,
            Some(JsonValue::String("b".to_string()))
        );
    }
}
