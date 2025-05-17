//! Secret definitions and provider interfaces
//!
//! This module provides interfaces and implementations for managing secrets
//! across different backends and environments.

pub mod adapter;
pub mod definitions;
pub mod encoder;
pub mod provider;

pub use adapter::{create_secrets_provider, SecretsAdapter};
pub use definitions::{ComponentSecrets, GenerationMethod, GenerationResult, SecretsDefinitions};
pub use encoder::{Base64SecretsEncoder, EncryptedSecretsEncoder, NoopEncoder, SecretsEncoder};
pub use provider::{Environment, SecretError, SecretsProvider, SecretsProviderFactory};
