//! Secret definitions and provider interfaces
//!
//! This module provides interfaces and implementations for managing secrets
//! across different backends and environments.

pub mod adapter;
pub mod definitions;
pub mod encoder;
pub mod provider;

pub use provider::{Environment, SecretError, SecretsProvider};
