//! Docker cross-compilation support utilities
//!
//! This module provides tools for managing Docker build environments
//! when cross-compiling between different architectures.

use std::env;

use log::{debug, trace};

/// Guards Docker cross-compilation environment variables
///
/// Sets appropriate environment variables when cross-compiling with Docker,
/// and ensures they are restored to their original values when dropped.
pub struct DockerCrossCompileGuard {
    cross_container_opts: Option<String>,
    docker_default_platform: Option<String>,
    target: String,
}

impl DockerCrossCompileGuard {
    /// Creates a new guard for the specified target platform
    ///
    /// # Arguments
    ///
    /// * `target` - Docker platform target string (e.g., "linux/amd64")
    pub fn new(target: &str) -> Self {
        debug!(
            "Creating new DockerCrossCompileGuard with target: {}",
            target
        );

        // Save original environment variables if they exist
        let cross_container_opts = match env::var("CROSS_CONTAINER_OPTS") {
            Ok(val) => {
                debug!("Found existing CROSS_CONTAINER_OPTS: {}", val);
                Some(val)
            }
            Err(_) => {
                debug!("No existing CROSS_CONTAINER_OPTS found");
                None
            }
        };

        let docker_default_platform = match env::var("DOCKER_DEFAULT_PLATFORM") {
            Ok(val) => {
                debug!("Found existing DOCKER_DEFAULT_PLATFORM: {}", val);
                Some(val)
            }
            Err(_) => {
                debug!("No existing DOCKER_DEFAULT_PLATFORM found");
                None
            }
        };

        // Set environment variables for cross-compilation
        env::set_var("CROSS_CONTAINER_OPTS", format!("--platform {target}"));
        env::set_var("DOCKER_DEFAULT_PLATFORM", target);
        trace!(
            "Set CROSS_CONTAINER_OPTS and DOCKER_DEFAULT_PLATFORM to {}",
            target
        );

        Self {
            cross_container_opts,
            docker_default_platform,
            target: target.to_string(),
        }
    }

    /// Returns the target platform
    pub fn target(&self) -> &str {
        &self.target
    }
}

impl Drop for DockerCrossCompileGuard {
    fn drop(&mut self) {
        debug!("Dropping DockerCrossCompileGuard");

        // Restore original environment variables or remove them
        match &self.cross_container_opts {
            Some(v) => {
                env::set_var("CROSS_CONTAINER_OPTS", v);
                debug!("Restored CROSS_CONTAINER_OPTS to: {}", v);
            }
            None => {
                env::remove_var("CROSS_CONTAINER_OPTS");
                debug!("Removed CROSS_CONTAINER_OPTS");
            }
        }

        match &self.docker_default_platform {
            Some(v) => {
                env::set_var("DOCKER_DEFAULT_PLATFORM", v);
                debug!("Restored DOCKER_DEFAULT_PLATFORM to: {}", v);
            }
            None => {
                env::remove_var("DOCKER_DEFAULT_PLATFORM");
                debug!("Removed DOCKER_DEFAULT_PLATFORM");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_sets_and_restores_env_vars() {
        // Save original environment
        let orig_opts = env::var("CROSS_CONTAINER_OPTS").ok();
        let orig_platform = env::var("DOCKER_DEFAULT_PLATFORM").ok();

        // Test scope for guard
        {
            let guard = DockerCrossCompileGuard::new("linux/arm64");
            assert_eq!(guard.target(), "linux/arm64");
            assert_eq!(
                env::var("CROSS_CONTAINER_OPTS").unwrap(),
                "--platform linux/arm64"
            );
            assert_eq!(env::var("DOCKER_DEFAULT_PLATFORM").unwrap(), "linux/arm64");
        }

        // Check that environment was restored
        if let Some(ref val) = orig_opts {
            assert_eq!(env::var("CROSS_CONTAINER_OPTS").unwrap(), *val);
        } else {
            assert!(env::var("CROSS_CONTAINER_OPTS").is_err());
        }

        if let Some(ref val) = orig_platform {
            assert_eq!(env::var("DOCKER_DEFAULT_PLATFORM").unwrap(), val.clone());
        } else {
            assert!(env::var("DOCKER_DEFAULT_PLATFORM").is_err());
        }

        // Clean up any changes
        if orig_opts.is_none() {
            env::remove_var("CROSS_CONTAINER_OPTS");
        }
        if orig_platform.is_none() {
            env::remove_var("DOCKER_DEFAULT_PLATFORM");
        }
    }
}
