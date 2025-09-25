use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;

use async_trait::async_trait;

/// Vault trait defines the interface for secret storage providers.
///
/// Implementations of this trait can store secrets in different backends
/// such as environment files, cloud providers, or local encrypted storage.
#[async_trait]
pub trait Vault: Debug {
    /// Retrieves secrets from the vault for a specific product, component, and environment.
    ///
    /// # Arguments
    ///
    /// * `product_name` - The name of the product
    /// * `component_name` - The name of the component within the product
    /// * `environment` - The environment (e.g., "dev", "staging", "prod")
    ///
    /// # Returns
    ///
    /// A map of secret keys to values, or an error if retrieval fails
    async fn get(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<HashMap<String, String>, Box<dyn Error>>;

    /// Stores secrets in the vault for a specific product, component, and environment.
    ///
    /// # Arguments
    ///
    /// * `product_name` - The name of the product
    /// * `component_name` - The name of the component within the product
    /// * `environment` - The environment (e.g., "dev", "staging", "prod")
    /// * `secrets` - A map of secret keys to values to store
    ///
    /// # Returns
    ///
    /// Ok on success, or an error if storage fails
    async fn set(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
        secrets: HashMap<String, String>,
    ) -> Result<(), Box<dyn Error>>;

    /// Creates a vault (product-level container) if it does not exist.
    ///
    /// # Arguments
    ///
    /// * `product_name` - The name of the product
    ///
    /// # Returns
    ///
    /// Ok on success, or an error if vault creation fails
    async fn create_vault(&mut self, product_name: &str) -> Result<(), Box<dyn Error>>;

    /// Removes all secrets for a specific product, component, and environment.
    ///
    /// # Arguments
    ///
    /// * `product_name` - The name of the product
    /// * `component_name` - The name of the component within the product
    /// * `environment` - The environment (e.g., "dev", "staging", "prod")
    ///
    /// # Returns
    ///
    /// Ok on success, or an error if removal fails
    async fn remove(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<(), Box<dyn Error>>;

    /// Checks if a vault (product-level container) exists.
    ///
    /// # Arguments
    ///
    /// * `product_name` - The name of the product
    ///
    /// # Returns
    ///
    /// A boolean indicating if the vault exists, or an error if the check fails
    async fn check_if_vault_exists(&self, product_name: &str) -> Result<bool, Box<dyn Error>>;
}
