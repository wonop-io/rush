//! Provides the core interface for accessing secrets across different implementations.
//!
//! This module defines a common interface for secrets providers, allowing
//! different backend implementations to be used interchangeably.

use async_trait::async_trait;
use log::{debug, info, trace, warn};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;

/// Defines the possible environments a secret can be used in
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Environment {
    Development,
    Testing,
    Staging,
    Production,
    Custom(String),
}

impl Display for Environment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Development => write!(f, "development"),
            Environment::Testing => write!(f, "testing"),
            Environment::Staging => write!(f, "staging"),
            Environment::Production => write!(f, "production"),
            Environment::Custom(name) => write!(f, "{}", name),
        }
    }
}

impl From<&str> for Environment {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "development" | "dev" => Environment::Development,
            "testing" | "test" => Environment::Testing,
            "staging" | "stage" => Environment::Staging,
            "production" | "prod" => Environment::Production,
            custom => Environment::Custom(custom.to_string()),
        }
    }
}

/// Error type for secret provider operations
#[derive(Debug)]
pub enum SecretError {
    NotFound(String),
    AccessDenied(String),
    ConnectionError(String),
    ValidationError(String),
    Other(Box<dyn Error + Send + Sync>),
}

impl Display for SecretError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretError::NotFound(msg) => write!(f, "Secret not found: {}", msg),
            SecretError::AccessDenied(msg) => write!(f, "Access denied: {}", msg),
            SecretError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            SecretError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            SecretError::Other(err) => write!(f, "Other error: {}", err),
        }
    }
}

impl Error for SecretError {}

impl From<std::io::Error> for SecretError {
    fn from(err: std::io::Error) -> Self {
        SecretError::Other(Box::new(err))
    }
}

/// Represents a set of operations that a secret provider must implement
#[async_trait]
pub trait SecretsProvider: Debug + Send + Sync {
    /// Get all secrets for a given product, component, and environment
    async fn get_secrets(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
    ) -> Result<HashMap<String, String>, SecretError>;

    /// Get a specific secret by key
    async fn get_secret(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
        key: &str,
    ) -> Result<String, SecretError> {
        let secrets = self
            .get_secrets(product_name, component_name, environment)
            .await?;
        secrets.get(key).cloned().ok_or_else(|| {
            SecretError::NotFound(format!(
                "Secret '{}' not found for {}/{}/{}",
                key, product_name, component_name, environment
            ))
        })
    }

    /// Set all secrets for a given product, component, and environment
    async fn set_secrets(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
        secrets: HashMap<String, String>,
    ) -> Result<(), SecretError>;

    /// Set a specific secret by key
    async fn set_secret(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
        key: &str,
        value: &str,
    ) -> Result<(), SecretError> {
        // Get existing secrets first
        let mut current_secrets = match self
            .get_secrets(product_name, component_name, environment)
            .await
        {
            Ok(secrets) => secrets,
            Err(SecretError::NotFound(_)) => HashMap::new(),
            Err(e) => return Err(e),
        };

        // Update the specific key
        current_secrets.insert(key.to_string(), value.to_string());

        // Set all secrets back
        self.set_secrets(product_name, component_name, environment, current_secrets)
            .await
    }

    /// Delete a specific secret by key
    async fn delete_secret(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
        key: &str,
    ) -> Result<(), SecretError> {
        // Get existing secrets first
        let mut current_secrets = self
            .get_secrets(product_name, component_name, environment)
            .await?;

        // If key doesn't exist, return an error
        if !current_secrets.contains_key(key) {
            return Err(SecretError::NotFound(format!(
                "Secret '{}' not found for {}/{}/{}",
                key, product_name, component_name, environment
            )));
        }

        // Remove the key
        current_secrets.remove(key);

        // Set the updated secrets
        self.set_secrets(product_name, component_name, environment, current_secrets)
            .await
    }

    /// Delete all secrets for a given product, component, and environment
    async fn delete_all_secrets(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
    ) -> Result<(), SecretError>;

    /// Check if a specific secret exists
    async fn has_secret(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
        key: &str,
    ) -> Result<bool, SecretError> {
        match self
            .get_secrets(product_name, component_name, environment)
            .await
        {
            Ok(secrets) => Ok(secrets.contains_key(key)),
            Err(SecretError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

/// Factory type for creating secret providers
pub type SecretsProviderFactory = Box<dyn Fn() -> Arc<dyn SecretsProvider> + Send + Sync>;
