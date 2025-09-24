//! Build coordination for container images
//!
//! This module handles building Docker images from various build types.

mod error;
mod processor;
pub mod orchestrator;
pub mod cache;
pub mod parallel;
pub mod persistent_cache;
pub mod dependency_resolver;
pub mod incremental;

pub use error::BuildError;
pub use processor::BuildProcessor;
pub use orchestrator::{BuildOrchestrator, BuildOrchestratorConfig};
pub use cache::{BuildCache, CacheEntry, CacheStats};
pub use parallel::{ParallelBuildExecutor, DependencyGraph};
pub use persistent_cache::{PersistentBuildCache, PersistentCacheConfig, BuildArtifactMetadata};
pub use dependency_resolver::{DependencyResolver, BuildTimeEstimate, BuildStats};
pub use incremental::{IncrementalBuilder, BuildState, ContentHasher, BuildStatistics};