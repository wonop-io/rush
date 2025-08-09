//! Core domain models and business logic
//!
//! This module contains the foundational components for Rush CLI,
//! including configuration management, environment handling, and
//! product definitions.

pub mod config;
pub mod dotenv;
pub mod environment;
pub mod product;

pub mod types;

// Re-export commonly used types for convenience
pub use self::config::Config;
pub use self::dotenv::{load_and_set_dotenv, load_dotenv, merge_dotenv_files};
pub use self::environment::setup_environment;
pub use self::product::{Product, ProductComponent};
pub use self::types::{BuildStatus, ValidationResult};
