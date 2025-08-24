//! Build coordination for container images
//!
//! This module handles building Docker images from various build types.

mod error;
mod processor;
pub mod orchestrator;
pub mod cache;

pub use error::BuildError;
pub use processor::BuildProcessor;
pub use orchestrator::{BuildOrchestrator, BuildOrchestratorConfig};
pub use cache::{BuildCache, CacheEntry, CacheStats};