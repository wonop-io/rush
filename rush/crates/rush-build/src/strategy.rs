//! Build strategy trait and implementations
//!
//! This module provides a trait-based approach to building different component types.

use std::path::Path;

use async_trait::async_trait;
use rush_core::{Error, Result};

use crate::{BuildContext, BuildType, ComponentBuildSpec};

/// Trait for build strategies
#[async_trait]
pub trait BuildStrategy: Send + Sync {
    /// Get the name of this build strategy
    fn name(&self) -> &str;

    /// Check if this strategy can handle the given build type
    fn can_handle(&self, build_type: &BuildType) -> bool;

    /// Validate the build specification
    fn validate(&self, spec: &ComponentBuildSpec) -> Result<()>;

    /// Execute the build
    async fn build(&self, spec: &ComponentBuildSpec, context: &BuildContext) -> Result<()>;

    /// Check if Docker build is required
    fn requires_docker(&self, spec: &ComponentBuildSpec) -> bool {
        spec.build_type.dockerfile_path().is_some()
    }

    /// Get build artifacts location
    fn artifacts_path(&self, spec: &ComponentBuildSpec) -> Option<String> {
        spec.build_type.location().map(|s| s.to_string())
    }
}

/// Rust binary build strategy
pub struct RustBinaryStrategy;

#[async_trait]
impl BuildStrategy for RustBinaryStrategy {
    fn name(&self) -> &str {
        "RustBinary"
    }

    fn can_handle(&self, build_type: &BuildType) -> bool {
        matches!(build_type, BuildType::RustBinary { .. })
    }

    fn validate(&self, spec: &ComponentBuildSpec) -> Result<()> {
        if let BuildType::RustBinary { location, .. } = &spec.build_type {
            // Check if Cargo.toml exists
            let cargo_path = Path::new(location).join("Cargo.toml");
            if !cargo_path.exists() {
                return Err(Error::Build(format!(
                    "Cargo.toml not found at {}",
                    cargo_path.display()
                )));
            }
        }
        Ok(())
    }

    async fn build(&self, spec: &ComponentBuildSpec, _context: &BuildContext) -> Result<()> {
        if let BuildType::RustBinary {
            location,
            features,
            precompile_commands,
            ..
        } = &spec.build_type
        {
            log::info!("Building Rust binary at {}", location);

            // Run precompile commands if any
            if let Some(commands) = precompile_commands {
                for cmd in commands {
                    log::info!("Running precompile command: {}", cmd);
                    let parts: Vec<&str> = cmd.split_whitespace().collect();
                    if !parts.is_empty() {
                        let mut command = rush_utils::CommandConfig::new(parts[0]);
                        for arg in &parts[1..] {
                            command = command.arg(*arg);
                        }
                        command = command.working_dir(location);

                        let output = rush_utils::CommandRunner::run(command).await?;
                        if !output.success() {
                            return Err(Error::Build(format!(
                                "Precompile command failed: {}",
                                output.stderr
                            )));
                        }
                    }
                }
            }

            // Use rush_utils command runner
            let mut cmd = rush_utils::CommandConfig::new("cargo");
            cmd = cmd.arg("build").arg("--release").working_dir(location);

            // Add features if specified
            if let Some(features) = features {
                if !features.is_empty() {
                    cmd = cmd.arg("--features").arg(features.join(","));
                }
            }

            let output = rush_utils::CommandRunner::run(cmd).await?;
            if !output.success() {
                return Err(Error::Build(format!(
                    "Cargo build failed: {}",
                    output.stderr
                )));
            }
        }

        Ok(())
    }
}

/// Trunk WASM build strategy
pub struct TrunkWasmStrategy;

#[async_trait]
impl BuildStrategy for TrunkWasmStrategy {
    fn name(&self) -> &str {
        "TrunkWasm"
    }

    fn can_handle(&self, build_type: &BuildType) -> bool {
        matches!(build_type, BuildType::TrunkWasm { .. })
    }

    fn validate(&self, spec: &ComponentBuildSpec) -> Result<()> {
        if let BuildType::TrunkWasm { location, .. } = &spec.build_type {
            // Check if index.html exists
            let index_path = Path::new(location).join("index.html");
            if !index_path.exists() {
                return Err(Error::Build(format!(
                    "index.html not found at {}",
                    index_path.display()
                )));
            }
        }
        Ok(())
    }

    async fn build(&self, spec: &ComponentBuildSpec, _context: &BuildContext) -> Result<()> {
        if let BuildType::TrunkWasm {
            location,
            features,
            precompile_commands,
            ..
        } = &spec.build_type
        {
            log::info!("Building Trunk WASM at {}", location);

            // Run precompile commands if any
            if let Some(commands) = precompile_commands {
                for cmd in commands {
                    log::info!("Running precompile command: {}", cmd);
                    let parts: Vec<&str> = cmd.split_whitespace().collect();
                    if !parts.is_empty() {
                        let mut command = rush_utils::CommandConfig::new(parts[0]);
                        for arg in &parts[1..] {
                            command = command.arg(*arg);
                        }
                        command = command.working_dir(location);

                        let output = rush_utils::CommandRunner::run(command).await?;
                        if !output.success() {
                            return Err(Error::Build(format!(
                                "Precompile command failed: {}",
                                output.stderr
                            )));
                        }
                    }
                }
            }

            let mut cmd = rush_utils::CommandConfig::new("trunk");
            cmd = cmd.arg("build").arg("--release").working_dir(location);

            // Add features if specified
            if let Some(features) = features {
                if !features.is_empty() {
                    // Trunk uses --features flag differently
                    for feature in features {
                        cmd = cmd.arg("--features").arg(feature);
                    }
                }
            }

            let output = rush_utils::CommandRunner::run(cmd).await?;
            if !output.success() {
                return Err(Error::Build(format!(
                    "Trunk build failed: {}",
                    output.stderr
                )));
            }
        }

        Ok(())
    }
}

/// Script-based build strategy
pub struct ScriptStrategy;

#[async_trait]
impl BuildStrategy for ScriptStrategy {
    fn name(&self) -> &str {
        "Script"
    }

    fn can_handle(&self, build_type: &BuildType) -> bool {
        matches!(build_type, BuildType::Script { .. })
    }

    fn validate(&self, spec: &ComponentBuildSpec) -> Result<()> {
        // Script builds require either a build command in the spec or a Dockerfile
        if spec.build.is_none() && spec.build_type.dockerfile_path().is_none() {
            return Err(Error::Build(
                "Script build type requires either 'build' command or Dockerfile".to_string(),
            ));
        }
        Ok(())
    }

    async fn build(&self, spec: &ComponentBuildSpec, _context: &BuildContext) -> Result<()> {
        if let BuildType::Script { location, .. } = &spec.build_type {
            // Check if there's a custom build command
            if let Some(build_cmd) = &spec.build {
                log::info!("Running build script: {}", build_cmd);

                // Parse command
                let parts: Vec<&str> = build_cmd.split_whitespace().collect();
                if parts.is_empty() {
                    return Err(Error::Build("Empty build command".to_string()));
                }

                let mut cmd = rush_utils::CommandConfig::new(parts[0]);
                for arg in &parts[1..] {
                    cmd = cmd.arg(*arg);
                }
                cmd = cmd.working_dir(location);

                let output = rush_utils::CommandRunner::run(cmd).await?;
                if !output.success() {
                    return Err(Error::Build(format!(
                        "Build script failed: {}",
                        output.stderr
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Docker image build strategy
pub struct DockerImageStrategy;

#[async_trait]
impl BuildStrategy for DockerImageStrategy {
    fn name(&self) -> &str {
        "PureDockerImage"
    }

    fn can_handle(&self, build_type: &BuildType) -> bool {
        matches!(build_type, BuildType::PureDockerImage { .. })
    }

    fn validate(&self, spec: &ComponentBuildSpec) -> Result<()> {
        if let BuildType::PureDockerImage {
            image_name_with_tag,
            ..
        } = &spec.build_type
        {
            if image_name_with_tag.is_empty() {
                return Err(Error::Build(
                    "PureDockerImage requires an image name".to_string(),
                ));
            }
        }
        Ok(())
    }

    async fn build(&self, spec: &ComponentBuildSpec, _context: &BuildContext) -> Result<()> {
        // Docker image build is handled separately by the image builder
        log::info!("Docker image build for {}", spec.component_name);
        Ok(())
    }

    fn requires_docker(&self, _spec: &ComponentBuildSpec) -> bool {
        true
    }
}

/// Local service strategy (no build required)
pub struct LocalServiceStrategy;

#[async_trait]
impl BuildStrategy for LocalServiceStrategy {
    fn name(&self) -> &str {
        "LocalService"
    }

    fn can_handle(&self, build_type: &BuildType) -> bool {
        matches!(build_type, BuildType::LocalService { .. })
    }

    fn validate(&self, _spec: &ComponentBuildSpec) -> Result<()> {
        Ok(())
    }

    async fn build(&self, spec: &ComponentBuildSpec, _context: &BuildContext) -> Result<()> {
        log::debug!(
            "LocalService {} doesn't require building",
            spec.component_name
        );
        Ok(())
    }

    fn requires_docker(&self, _spec: &ComponentBuildSpec) -> bool {
        false
    }
}

/// Registry of build strategies
pub struct BuildStrategyRegistry {
    strategies: Vec<Box<dyn BuildStrategy>>,
}

impl BuildStrategyRegistry {
    /// Create a new registry with default strategies
    pub fn new() -> Self {
        let strategies: Vec<Box<dyn BuildStrategy>> = vec![
            Box::new(RustBinaryStrategy),
            Box::new(TrunkWasmStrategy),
            Box::new(ScriptStrategy),
            Box::new(DockerImageStrategy),
            Box::new(LocalServiceStrategy),
        ];

        Self { strategies }
    }

    /// Register a custom build strategy
    pub fn register(&mut self, strategy: Box<dyn BuildStrategy>) {
        self.strategies.push(strategy);
    }

    /// Find a strategy for the given build type
    pub fn find_strategy(&self, build_type: &BuildType) -> Option<&dyn BuildStrategy> {
        self.strategies
            .iter()
            .find(|s| s.can_handle(build_type))
            .map(|s| s.as_ref())
    }

    /// Build a component using the appropriate strategy
    pub async fn build(&self, spec: &ComponentBuildSpec, context: &BuildContext) -> Result<()> {
        let strategy = self.find_strategy(&spec.build_type).ok_or_else(|| {
            Error::Build(format!(
                "No build strategy found for component {}",
                spec.component_name
            ))
        })?;

        strategy.validate(spec)?;
        strategy.build(spec, context).await?;

        Ok(())
    }
}

impl Default for BuildStrategyRegistry {
    fn default() -> Self {
        Self::new()
    }
}
