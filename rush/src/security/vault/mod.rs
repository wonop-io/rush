//! Vault module for secret management
//!
//! This module provides abstractions for secure storage and retrieval
//! of sensitive information like passwords, API keys, and certificates.

pub mod adapter;
pub mod dotenv;
pub mod file;
pub mod onepassword;
pub mod vault_trait;

pub use dotenv::DotenvVault;
pub use file::FileVault;
pub use onepassword::OnePassword;
pub use vault_trait::Vault;
