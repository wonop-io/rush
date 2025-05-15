use colored::Colorize;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::path::Path;

use crate::build::BuildContext;
use crate::build::BuildScript;
use crate::build::BuildType;
use crate::container::docker::DockerImage;
use crate::security::Vault;
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
    /// * `image` - The Docker image to build
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub async fn build_image(&self, image: &DockerImage) -> Result<(), String> {
        debug!("Building image: {}", image.component_name());

        if image.should_ignore_in_devmode() {
            info!(
                "Skipping build for {} (ignored in dev mode)",
                image.component_name()
            );
            return Ok(());
        }

        if !image.should_rebuild() {
            debug!("Image {} doesn't need rebuilding", image.component_name());
            return Ok(());
        }

        let component_name = image.component_name();
        let identifier = image.identifier();

        info!("Building {}", identifier);

        // Get the vault and secrets
        let vault = match image.vault() {
            Some(vault) => vault,
            None => return Err("No vault configured for image".to_string()),
        };

        let toolchain = match image.toolchain() {
            Some(toolchain) => toolchain,
            None => return Err("No toolchain configured for image".to_string()),
        };

        let spec = image.spec();
        let environment = spec.config().environment().to_string();

        // Get secrets from vault
        let secrets = {
            let vault_guard = vault.lock().unwrap();
            match vault_guard
                .get(&spec.product_name, &component_name, &environment)
                .await
            {
                Ok(secrets) => secrets,
                Err(e) => {
                    warn!("Failed to get secrets for {}: {}", component_name, e);
                    HashMap::new()
                }
            }
        };

        // Create build context
        let build_context = image.generate_build_context(secrets);

        // Change to product directory
        let _dir_guard = Directory::chpath(&spec.product_dir);

        // Execute build script if needed
        if let Some(build_script) = self.get_build_script(image, &build_context) {
            trace!("Executing build script for {}", component_name);

            let window_size = if self.verbose { 20 } else { 10 };
            let start_time = std::time::Instant::now();

            match run_command_in_window(
                window_size,
                &format!("Building {}", component_name),
                "sh",
                vec!["-c", &build_script],
            )
            .await
            {
                Ok(_) => {
                    let duration = start_time.elapsed();
                    info!(
                        "Build script for {} completed in {:?}",
                        component_name, duration
                    );
                }
                Err(e) => {
                    let duration = start_time.elapsed();
                    error!(
                        "Build script for {} failed after {:?}: {}",
                        component_name, duration, e
                    );
                    return Err(format!("Build script failed: {}", e));
                }
            }
        }

        // Build Docker image if required
        if let Err(e) = self.build_docker_image(image).await {
            error!("Docker build for {} failed: {}", component_name, e);
            return Err(e);
        }

        info!("Successfully built {}", identifier);
        Ok(())
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
    fn get_build_script(&self, image: &DockerImage, context: &BuildContext) -> Option<String> {
        match &image.spec().build_type {
            BuildType::PureDockerImage { .. } => None,
            BuildType::PureKubernetes => None,
            BuildType::KubernetesInstallation { .. } => None,
            _ => {
                let script = BuildScript::new(image.spec().build_type.clone()).render(context);
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
    async fn build_docker_image(&self, image: &DockerImage) -> Result<(), String> {
        let spec = image.spec();

        match &spec.build_type {
            BuildType::PureDockerImage { .. } => Ok(()),
            BuildType::PureKubernetes => Ok(()),
            BuildType::KubernetesInstallation { .. } => Ok(()),
            _ => {
                let dockerfile_path = match spec.build_type.dockerfile_path() {
                    Some(path) => path,
                    None => return Err("No Dockerfile path specified".to_string()),
                };

                let context_dir = match &spec.build_type {
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
                    .ok_or_else(|| "Invalid Dockerfile path".to_string())?;
                let dockerfile_name = Path::new(dockerfile_path)
                    .file_name()
                    .ok_or_else(|| "Invalid Dockerfile path".to_string())?
                    .to_str()
                    .ok_or_else(|| "Invalid Dockerfile name".to_string())?;

                let _dir_guard = Directory::chpath(dockerfile_dir);

                let toolchain = match image.toolchain() {
                    Some(toolchain) => toolchain,
                    None => return Err("No toolchain configured for image".to_string()),
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
                    Err(e) => Err(e),
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
    ) -> Result<(), String>
    where
        F: Fn() -> bool,
    {
        // Colorize error messages for better visibility
        let colorized_error = error
            .replace("error:", &format!("{}:", "error".red().bold()))
            .replace("error[", &format!("{}[", "error".red().bold()))
            .replace("warning:", &format!("{}:", "warning".yellow().bold()));

        error!("Build failed for {}: {}", component_name, colorized_error);

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
                    warn!("Build error recovery timeout reached for
            {}", component_name);
                    return Err(format!("Build failed for {} and recovery timeout reached", component_name));
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Termination signal received during build error recovery");
                    return Err("Build process terminated by user".to_string());
                }
            }
        }
    }
}
