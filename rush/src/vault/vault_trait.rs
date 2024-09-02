use async_trait::async_trait;
use std::collections::HashMap;
use std::error::Error;
use core::fmt::Debug;

impl Debug for dyn Vault {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Vault")
    }
}

impl Debug for dyn Vault + Send {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Vault")
    }
}

#[async_trait]
pub trait Vault {
    /// Retrieves secrets from the vault for a specific product, component, and environment.
    async fn get(
        &self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<HashMap<String, String>, Box<dyn Error>>;

    /// Stores secrets in the vault for a specific product, component, and environment.
    async fn set(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
        secrets: HashMap<String, String>,
    ) -> Result<(), Box<dyn Error>>;

    /// Creates a vault (product) if it does not exist.
    async fn create_vault(&mut self, product_name: &str) -> Result<(), Box<dyn Error>>;

    /// Removes secrets from the vault for a specific product, component, and environment.
    async fn remove(
        &mut self,
        product_name: &str,
        component_name: &str,
        environment: &str,
    ) -> Result<(), Box<dyn Error>>;

    /// Checks if a vault (product) exists.
    async fn check_if_vault_exists(&self, product_name: &str) -> Result<bool, Box<dyn Error>>;
}
