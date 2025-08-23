//! Build coordination for container images
//!
//! This module handles building Docker images from various build types.

mod error;
mod processor;

pub use error::BuildError;
pub use processor::BuildProcessor;