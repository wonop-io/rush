//! Security module for secret management and environment definitions
//!
//! This module provides implementations for managing secrets and
//! environment-specific configurations across different environments.

mod env_defs;
mod secrets;
mod vault;

pub use env_defs::EnvironmentDefinitions;
pub use secrets::{
    adapter::{create_secrets_provider, SecretsAdapter},
    definitions::{ComponentSecrets, GenerationMethod, GenerationResult, SecretsDefinitions},
    encoder::{Base64SecretsEncoder, EncryptedSecretsEncoder, NoopEncoder, SecretsEncoder},
    provider::{Environment, SecretError, SecretsProvider, SecretsProviderFactory},
};
pub use vault::{
    adapter::{create_vault_provider, VaultAdapter},
    DotenvVault, FileVault, OnePassword, Vault,
};
