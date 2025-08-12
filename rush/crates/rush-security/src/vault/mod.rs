pub mod dotenv;
pub mod file;

pub use dotenv::DotenvVault;
pub use file::FileVault;

use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait Vault: Send + Sync {
    async fn get(&self, product: &str, component: &str, environment: &str) -> Result<HashMap<String, String>, anyhow::Error>;
    async fn set(&mut self, product: &str, component: &str, environment: &str, secrets: HashMap<String, String>) -> Result<(), anyhow::Error>;
}
