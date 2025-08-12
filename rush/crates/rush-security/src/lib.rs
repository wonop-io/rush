//! Rush Security - Security and secrets management

pub mod env_defs;
pub mod secrets;
pub mod vault;

pub use secrets::{SecretsProvider};
pub use vault::{Vault, FileVault, DotenvVault};

// Re-export common types
pub use secrets::definitions::*;
pub use secrets::encoder::{SecretsEncoder, Base64SecretsEncoder, NoopEncoder};
pub use env_defs::EnvironmentDefinitions;
