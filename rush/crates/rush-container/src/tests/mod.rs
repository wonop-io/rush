//! Test module for container management functionality
//!
//! This module contains comprehensive tests for:
//! - Platform architecture validation
//! - Graceful shutdown handling
//! - Container crash detection
//! - Build failure recovery
//! - Log capture completeness

#[cfg(test)]
pub mod mock_docker;

#[cfg(test)]
pub mod image_builder_tests;

#[cfg(test)]
pub mod reactor_tests;

#[cfg(test)]
pub mod output_tests;

#[cfg(test)]
pub mod shutdown_tests;

#[cfg(test)]
pub mod test_helpers;

#[cfg(test)]
pub mod docker_push_test;

#[cfg(test)]
pub mod registry_config_test;
