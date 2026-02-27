//! Product management functionality
//!
//! This module provides definitions and loaders for products and components.

mod loader;
mod types;

pub use loader::ProductLoader;
pub use types::{Product, ProductComponent, ProductConfig};
