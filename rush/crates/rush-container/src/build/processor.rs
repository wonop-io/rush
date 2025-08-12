use colored::Colorize;
use log::{debug, error, info, warn};
use std::path::Path;

use crate::build::BuildContext;
use crate::build::BuildScript;
use crate::build::BuildType;
use crate::container::ImageBuilder;
use rush_core::error::{Error, Result};
use rush_core::shutdown;
use crate::utils::{run_command_in_window, Directory};

/// Manages the build process for containers
pub struct BuildProcessor {
    /// Flag to determine if we're running in verbose mode
    verbose: bool,
}

impl BuildProcessor {
    /// Creates a new BuildProcessor
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    /// Builds a Docker image for a component
    ///
    /// # Arguments
    ///
    /// * `image` - The Docker image to build (mutable to update cache state)
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn build_image(&self, image: &mut ImageBuilder) -> Result<()> {
        let shutdown_token = shutdown::global_shutdown().cancellation_token();

        // Check for shutdown before starting build
        if shutdown_token.is_cancelled() {
            info!("Build cancelled due to shutdown signal");
            return Err(Error::Terminated("Build cancelled due to shutdown".into()));
        }

        debug!("Building image: {}", image.component_name());

        // Check if rebuild is needed based on cache
        let needs_rebuild = match image.evaluate_rebuild_needed().await {
            Ok(needed) => needed,
            Err(e) => {
                warn!(
                    "Failed to evaluate cache status: {}, proceeding with build",
                    e
                );
                true
            }
        };

        if !needs_rebuild {
            info!(
                "Image {} already exists in cache with clean git tag, skipping build",
                image.component_name()
            );
            return Ok(());
        }

        let component_name = image.component_name().to_string();

        info!("Building {}", component_name);

        // Check for shutdown again before expensive operations
        if shutdown_token.is_cancelled() {
            info!(
                "Build cancelled for {} due to shutdown signal",
                component_name
            );
            return Err(Error::Terminated("Build cancelled due to shutdown".into()));
        }

        // Generate build context
        let _build_context = tokio::select! {
            result = image.generate_build_context() => {
                match result {
                    Ok(context) => context,
                    Err(e) => return Err(format!("Failed to generate build context: {e}").into()),
                }
            }
            _ = shutdown_token.cancelled() => {
                info!("Build context generation cancelled for {} due to shutdown", component_name);
                return Err(Error::Terminated("Build cancelled due to shutdown".into()));
            }
        };

        // Execute build with cancellation
        let build_result = tokio::select! {
            result = image.build() => result,
            _ = shutdown_token.cancelled() => {
                info!("Build cancelled for {} due to shutdown signal", component_name);
                return Err(Error::Terminated("Build cancelled due to shutdown".into()));
            }
        };

        match build_result {
            Ok(_) => {
                info!("Successfully built {}", component_name);
                // Mark that the image was recently rebuilt
                image.set_was_recently_rebuilt(true);
                Ok(())
            }
            Err(e) => {
                error!("Build failed for {}: {}", component_name, e);
                Err(e)
            }
        }
    }

    /// Gets the build script for a component if needed
    ///
    /// # Arguments
    ///
    /// * `image` - The Docker image
    /// * `context` - The build context
    ///
    /// # Returns
    ///
    /// Optional build script string
    fn get_build_script(&self, image: &ImageBuilder, context: &BuildContext) -> Option<String> {
        let spec = image.spec();
        let build_type = match spec.lock() {
            Ok(spec) => spec.build_type.clone(),
            Err(_) => return None,
        };

        match &build_type {
            BuildType::PureDockerImage { .. } => None,
            BuildType::PureKubernetes => None,
            BuildType::KubernetesInstallation { .. } => None,
            _ => {
                let script = BuildScript::new(build_type).render(context);
                if script.is_empty() {
                    None
                } else {
                    Some(script)
                }
            }
        }
    }

    /// Builds a Docker image
    ///
    /// # Arguments
    ///
    /// * `image` - The Docker image to build
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    async fn build_docker_image(&self, image: &ImageBuilder) -> Result<()> {
        let spec = image.spec();
        let spec_guard = match spec.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(Error::Internal("Failed to lock spec".into())),
        };

        match &spec_guard.build_type {
            BuildType::PureDockerImage { .. } => Ok(()),
            BuildType::PureKubernetes => Ok(()),
            BuildType::KubernetesInstallation { .. } => Ok(()),
            _ => {
                let dockerfile_path = match spec_guard.build_type.dockerfile_path() {
                    Some(path) => path,
                    None => return Err(Error::Setup("No Dockerfile path specified".into())),
                };

                let context_dir = match &spec_guard.build_type {
                    BuildType::TrunkWasm { context_dir, .. } => context_dir,
                    BuildType::RustBinary { context_dir, .. } => context_dir,
                    BuildType::DixiousWasm { context_dir, .. } => context_dir,
                    BuildType::Script { context_dir, .. } => context_dir,
                    BuildType::Zola { context_dir, .. } => context_dir,
                    BuildType::Book { context_dir, .. } => context_dir,
                    BuildType::Ingress { context_dir, .. } => context_dir,
                    _ => return Ok(()),
                };

                let context_dir = context_dir.clone().unwrap_or_else(|| ".".to_string());
                let dockerfile_dir = Path::new(dockerfile_path)
                    .parent()
                    .ok_or_else(|| Error::Setup("Invalid Dockerfile path".into()))?;
                let dockerfile_name = Path::new(dockerfile_path)
                    .file_name()
                    .ok_or_else(|| Error::Setup("Invalid Dockerfile path".into()))?
                    .to_str()
                    .ok_or_else(|| Error::Setup("Invalid Dockerfile name".into()))?;

                let _dir_guard = Directory::chpath(dockerfile_dir);

                // Get the toolchain
                let toolchain = image.toolchain();
                let toolchain = match toolchain {
                    Some(toolchain) => toolchain,
                    None => return Err(Error::Setup("No toolchain configured for image".into())),
                };

                let tag = image.tagged_image_name();
                let window_size = if self.verbose { 20 } else { 10 };

                let build_command_args =
                    vec!["build", "-t", &tag, "-f", dockerfile_name, &context_dir];

                match run_command_in_window(
                    window_size,
                    &format!("docker build {}", image.component_name()),
                    toolchain.docker(),
                    build_command_args,
                )
                .await
                {
                    Ok(_) => Ok(()),
                    Err(e) => Err(Error::Docker(e)),
                }
            }
        }
    }

    /// Handles a build error and determines what to do next
    ///
    /// # Arguments
    ///
    /// * `error` - The error message
    /// * `component_name` - Name of the component that failed
    /// * `test_if_files_changed` - Function to check if files have changed
    ///
    /// # Returns
    ///
    /// Result indicating whether to continue or abort
    pub async fn handle_build_error<F>(
        &self,
        error: String,
        component_name: &str,
        test_if_files_changed: F,
    ) -> Result<()>
    where
        F: Fn() -> bool,
    {
        let shutdown_token = shutdown::global_shutdown().cancellation_token();

        // Check if shutdown was initiated before handling the error
        if shutdown_token.is_cancelled() {
            info!(
                "Build error recovery cancelled for {} due to shutdown",
                component_name
            );
            return Err(Error::Terminated(
                "Build error recovery cancelled due to shutdown".into(),
            ));
        }

        // Colorize error messages for better visibility
        let colorized_error = error
            .replace("error:", &format!("{}:", "error".red().bold()))
            .replace("error[", &format!("{}[", "error".red().bold()))
            .replace("warning:", &format!("{}:", "warning".yellow().bold()));

        error!("Build failed for {}: {}", component_name, colorized_error);
        info!(
            "Waiting for file changes or termination signal to retry build for {}",
            component_name
        );

        // Check for file changes while waiting
        let mut check_interval = tokio::time::interval(tokio::time::Duration::from_millis(100));

        // Set a timeout for error recovery
        let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(300)); // 5 minute timeout
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = check_interval.tick() => {
                    if test_if_files_changed() {
                        info!("File changes detected. Attempting to rebuild...");
                        return Ok(());
                    }
                }
                _ = &mut timeout => {
                    warn!("Build error recovery timeout reached for {}", component_name);
                    return Err(format!("Build failed for {component_name} and recovery timeout reached").into());
                }
                _ = shutdown_token.cancelled() => {
                    info!("Build error recovery terminated for {} due to shutdown signal", component_name);
                    return Err(Error::Terminated("Build process terminated by user".into()));
                }
            }
        }
    }
}
