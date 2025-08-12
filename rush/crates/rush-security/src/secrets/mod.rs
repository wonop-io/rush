pub mod adapter;
pub mod definitions;
pub mod encoder;

pub use definitions::*;
pub use encoder::{SecretsEncoder, Base64SecretsEncoder, NoopEncoder};

use async_trait::async_trait;
use std::collections::HashMap;

#[async_trait]
pub trait SecretsProvider: Send + Sync {
    async fn get_secrets(&self, context: &str) -> Result<HashMap<String, String>, anyhow::Error>;
}
