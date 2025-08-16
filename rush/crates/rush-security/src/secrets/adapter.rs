//! Secrets adapter connecting to vault implementations
//!
//! This module provides an adapter that allows Secrets implementations to be used with the
//! Vault trait, enabling a consistent interface for accessing secrets across
//! different storage backends.

use crate::secrets::{Environment, SecretError, SecretsProvider};
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

/// Adapter that converts secure secret values to encrypted values suitable for Kubernetes
pub struct SecretsAdapter<S: SecretsProvider + Send + Sync> {
    secrets_provider: S,
}

impl<S: SecretsProvider + Send + Sync> SecretsAdapter<S> {
    /// Create a new SecretsAdapter wrapping the given secrets provider
    pub fn new(secrets_provider: S) -> Self {
        Self { secrets_provider }
    }
}

impl<S: SecretsProvider + Send + Sync> Debug for SecretsAdapter<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretsAdapter")
            .field("secrets_provider", &"SecretsProvider Implementation")
            .finish()
    }
}

/// Provides access to secrets through the SecretsProvider interface
#[async_trait]
impl<S: SecretsProvider + Send + Sync> SecretsProvider for SecretsAdapter<S> {
    async fn get_secrets(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
    ) -> Result<HashMap<String, String>, SecretError> {
        self.secrets_provider
            .get_secrets(product_name, component_name, environment)
            .await
    }

    async fn set_secrets(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
        secrets: HashMap<String, String>,
    ) -> Result<(), SecretError> {
        self.secrets_provider
            .set_secrets(product_name, component_name, environment, secrets)
            .await
    }

    async fn delete_all_secrets(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
    ) -> Result<(), SecretError> {
        self.secrets_provider
            .delete_all_secrets(product_name, component_name, environment)
            .await
    }
}

/// Factory function to create a SecretsAdapter wrapped in an Arc
pub fn create_secrets_provider<S: SecretsProvider + Send + Sync + 'static>(
    secrets_provider: S,
) -> Arc<dyn SecretsProvider> {
    Arc::new(SecretsAdapter::new(secrets_provider))
}
