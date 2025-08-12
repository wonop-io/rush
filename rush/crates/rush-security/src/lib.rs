//! Rush Security - Security and secrets management

pub mod env_defs;
pub mod secrets;
pub mod vault;

pub use secrets::{SecretsEncoder, SecretsProvider};
pub use vault::{Vault, FileVault, DotenvVault};

// Re-export common types
pub use secrets::definitions::*;
pub use secrets::encoder::{Base64SecretsEncoder, NoopEncoder};
