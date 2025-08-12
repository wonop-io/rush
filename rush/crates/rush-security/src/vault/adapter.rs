//! Vault adapter that connects Vault implementations to the SecretsProvider interface
//!
//! This module provides an adapter that allows Vault implementations to be used with the
//! SecretsProvider trait, enabling a consistent interface for accessing secrets across
//! different storage backends.

use crate::secrets::{Environment, SecretError, SecretsProvider};
use crate::vault::vault_trait::Vault;
use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

/// Adapter that wraps a Vault implementation and exposes it as a SecretsProvider
pub struct VaultAdapter<V: Vault + Send + Sync> {
    vault: V,
}

impl<V: Vault + Send + Sync> VaultAdapter<V> {
    /// Create a new VaultAdapter wrapping the given vault implementation
    pub fn new(vault: V) -> Self {
        Self { vault }
    }

    /// Get the environment string used for vault storage
    fn env_to_string(env: &Environment) -> String {
        env.to_string()
    }

    // Convert Box<dyn Error> to thread-safe error
    fn convert_error(err: Box<dyn std::error::Error>) -> Box<dyn std::error::Error + Send + Sync> {
        let err_string = format!("{err}");
        Box::new(std::io::Error::other(err_string))
    }
}

impl<V: Vault + Send + Sync> Debug for VaultAdapter<V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultAdapter")
            .field("vault", &"Vault Implementation")
            .finish()
    }
}

#[async_trait]
impl<V: Vault + Send + Sync> SecretsProvider for VaultAdapter<V> {
    async fn get_secrets(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
    ) -> Result<HashMap<String, String>, SecretError> {
        let env_str = Self::env_to_string(environment);
        self.vault
            .get(product_name, component_name, &env_str)
            .await
            .map_err(|e| SecretError::Other(Self::convert_error(e)))
    }

    async fn set_secrets(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
        secrets: HashMap<String, String>,
    ) -> Result<(), SecretError> {
        let env_str = Self::env_to_string(environment);

        // Make sure vault exists first
        let vault_exists = self
            .vault
            .check_if_vault_exists(product_name)
            .await
            .map_err(|e| SecretError::Other(Self::convert_error(e)))?;

        if !vault_exists {
            self.vault
                .create_vault(product_name)
                .await
                .map_err(|e| SecretError::Other(Self::convert_error(e)))?;
        }

        self.vault
            .set(product_name, component_name, &env_str, secrets)
            .await
            .map_err(|e| SecretError::Other(Self::convert_error(e)))
    }

    async fn delete_all_secrets(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &Environment,
    ) -> Result<(), SecretError> {
        let env_str = Self::env_to_string(environment);
        self.vault
            .remove(product_name, component_name, &env_str)
            .await
            .map_err(|e| SecretError::Other(Self::convert_error(e)))
    }
}

/// Factory function to create a VaultAdapter wrapped in an Arc
pub fn create_vault_provider<V: Vault + Send + Sync + 'static>(
    vault: V,
) -> Arc<dyn SecretsProvider> {
    Arc::new(VaultAdapter::new(vault))
}
