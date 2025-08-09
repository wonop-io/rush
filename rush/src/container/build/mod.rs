//! Build related functionality for containers
//!
//! This module provides facilities for building container images and
//! handling build errors.

mod error;
mod processor;

pub use error::{handle_build_error, BuildError};
pub use processor::BuildProcessor;
