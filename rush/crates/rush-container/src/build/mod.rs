//! Build coordination for container images
//!
//! This module handles building Docker images from various build types.

pub mod cache;
pub mod dependency_optimizer;
pub mod dependency_resolver;
mod error;
pub mod incremental;
pub mod orchestrator;
pub mod parallel;
pub mod performance_analyzer;
pub mod persistent_cache;
pub mod predictive_cache;
mod processor;

pub use cache::{BuildCache, CacheEntry, CacheStats};
pub use dependency_optimizer::{BuildGroup, DependencyOptimizer, OptimizationReport};
pub use dependency_resolver::{BuildStats, BuildTimeEstimate, DependencyResolver};
pub use error::BuildError;
pub use incremental::{BuildState, BuildStatistics, ContentHasher, IncrementalBuilder};
pub use orchestrator::{BuildOrchestrator, BuildOrchestratorConfig};
pub use parallel::{DependencyGraph, ParallelBuildExecutor};
pub use performance_analyzer::{BuildPerformanceAnalyzer, PerformanceAnalysisReport};
pub use persistent_cache::{BuildArtifactMetadata, PersistentBuildCache, PersistentCacheConfig};
pub use predictive_cache::{CachePerformanceReport, PredictiveCache};
pub use processor::BuildProcessor;
