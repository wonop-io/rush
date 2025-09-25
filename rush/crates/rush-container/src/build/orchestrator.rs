//! Build orchestration for container images
//!
//! This module coordinates the building of multiple components,
//! handling dependencies and parallel builds where possible.

use crate::{
    build::{BuildProcessor, BuildCache, CacheEntry, CacheStats, ParallelBuildExecutor},
    docker::DockerClient,
    events::{Event, EventBus, ContainerEvent},
    profiling,
    reactor::state::SharedReactorState,
    simple_output,
    tagging::ImageTagGenerator,
};
use rush_build::{BuildType, ComponentBuildSpec};
use rush_core::error::Result;
use rush_core::shutdown::global_shutdown;
use rush_output::simple::Sink;
use rush_toolchain::ToolchainContext;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use log::{debug, error, info, warn};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{instrument, info_span};

/// Configuration for the build orchestrator
#[derive(Debug, Clone)]
pub struct BuildOrchestratorConfig {
    /// Product name
    pub product_name: String,
    /// Product directory
    pub product_dir: PathBuf,
    /// Build timeout
    pub build_timeout: Duration,
    /// Enable parallel builds
    pub parallel_builds: bool,
    /// Max parallel builds
    pub max_parallel: usize,
    /// Enable build caching
    pub enable_cache: bool,
    /// Cache directory
    pub cache_dir: PathBuf,
}

impl BuildOrchestratorConfig {
    /// Alias for parallel_builds (for compatibility)
    pub fn enable_parallel_builds(&self) -> bool {
        self.parallel_builds
    }
    
    /// Alias for enable_cache (for compatibility)  
    pub fn enable_caching(&self) -> bool {
        self.enable_cache
    }
}

impl Default for BuildOrchestratorConfig {
    fn default() -> Self {
        Self {
            product_name: String::new(),
            product_dir: PathBuf::new(),
            build_timeout: Duration::from_secs(300),
            parallel_builds: true,
            max_parallel: 4,
            enable_cache: true,
            cache_dir: PathBuf::from(".rush/cache"),
        }
    }
}

/// Orchestrates the building of container images
pub struct BuildOrchestrator {
    config: BuildOrchestratorConfig,
    docker_client: Arc<dyn DockerClient>,
    event_bus: EventBus,
    state: SharedReactorState,
    pub(crate) cache: Arc<Mutex<BuildCache>>,
    build_processor: Arc<BuildProcessor>,
    pub(crate) tag_generator: Arc<ImageTagGenerator>,
    /// Output sink for build logs
    output_sink: Arc<tokio::sync::RwLock<Option<Arc<tokio::sync::Mutex<Box<dyn Sink>>>>>>,
    /// Track active build containers for cancellation
    active_builds: Arc<Mutex<HashSet<String>>>,
    /// Flag to indicate builds are being cancelled
    cancelling: Arc<AtomicBool>,
}

impl BuildOrchestrator {
    /// Create a new build orchestrator
    pub fn new(
        config: BuildOrchestratorConfig,
        docker_client: Arc<dyn DockerClient>,
        event_bus: EventBus,
        state: SharedReactorState,
    ) -> Self {
        let cache = Arc::new(Mutex::new(BuildCache::new(&config.cache_dir, &config.product_dir)));
        let build_processor = Arc::new(BuildProcessor::new(false));

        // Create toolchain and tag generator
        let toolchain = Arc::new(ToolchainContext::default());
        let tag_generator = Arc::new(ImageTagGenerator::new(
            toolchain,
            config.product_dir.clone(),
        ));

        Self {
            config,
            docker_client,
            event_bus,
            state,
            cache,
            build_processor,
            tag_generator,
            output_sink: Arc::new(tokio::sync::RwLock::new(None)),
            active_builds: Arc::new(Mutex::new(HashSet::new())),
            cancelling: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a new build orchestrator with custom toolchain
    pub fn with_toolchain(
        config: BuildOrchestratorConfig,
        docker_client: Arc<dyn DockerClient>,
        event_bus: EventBus,
        state: SharedReactorState,
        toolchain: Arc<ToolchainContext>,
    ) -> Self {
        let cache = Arc::new(Mutex::new(BuildCache::new(&config.cache_dir, &config.product_dir)));
        let build_processor = Arc::new(BuildProcessor::new(false));

        // Create tag generator with provided toolchain
        let tag_generator = Arc::new(ImageTagGenerator::new(
            toolchain,
            config.product_dir.clone(),
        ));

        Self {
            config,
            docker_client,
            event_bus,
            state,
            cache,
            build_processor,
            tag_generator,
            output_sink: Arc::new(tokio::sync::RwLock::new(None)),
            active_builds: Arc::new(Mutex::new(HashSet::new())),
            cancelling: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Set the output sink for build logs
    pub async fn set_output_sink(&self, sink: Arc<tokio::sync::Mutex<Box<dyn Sink>>>) {
        let mut output_sink = self.output_sink.write().await;
        *output_sink = Some(sink);
    }

    /// Get the product name
    pub fn product_name(&self) -> &str {
        &self.config.product_name
    }

    /// Get the docker client
    pub fn docker_client(&self) -> &Arc<dyn DockerClient> {
        &self.docker_client
    }

    /// Cancel all active builds
    pub async fn cancel_all_builds(&self) {
        info!("Cancelling all active builds");
        self.cancelling.store(true, Ordering::SeqCst);

        // Get and kill all active build containers
        let active_builds = {
            let builds = self.active_builds.lock().await;
            builds.clone()
        };

        for container_id in active_builds {
            info!("Killing build container: {}", container_id);
            if let Err(e) = self.docker_client.kill_container(&container_id).await {
                warn!("Failed to kill build container {}: {}", container_id, e);
            }
        }

        // Clear the active builds set
        let mut builds = self.active_builds.lock().await;
        builds.clear();
    }

    /// Check if builds are being cancelled
    pub fn is_cancelling(&self) -> bool {
        self.cancelling.load(Ordering::SeqCst)
    }

    /// Build all components with parallel execution if enabled
    pub async fn build_components_parallel(
        self: Arc<Self>,
        component_specs: Vec<ComponentBuildSpec>,
        force_rebuild: bool,
    ) -> Result<HashMap<String, String>> {
        if self.config.parallel_builds {
            info!("Using parallel build execution");
            let executor = ParallelBuildExecutor::new(
                Arc::clone(&self),
                self.config.max_parallel,
            );
            executor.build_optimized(component_specs, force_rebuild).await
        } else {
            // Fall back to sequential build
            self.build_components(component_specs, force_rebuild).await
        }
    }

    /// Build all components (sequential)
    #[instrument(
        level = "info",
        skip(self, component_specs),
        fields(
            component_count = component_specs.len(),
            force_rebuild = force_rebuild
        )
    )]
    pub async fn build_components(
        &self,
        component_specs: Vec<ComponentBuildSpec>,
        force_rebuild: bool,
    ) -> Result<HashMap<String, String>> {
        info!("Building {} components", component_specs.len());
        let start_time = Instant::now();

        // Record start in performance tracker
        let perf_tracker = profiling::global_tracker();
        
        // Don't change state here - let the caller manage state transitions
        
        // Publish build started event
        if let Err(e) = self.event_bus.publish(Event::new(
            "build",
            ContainerEvent::BuildStarted {
                component: "all".to_string(),
                timestamp: Instant::now(),
            },
        )).await {
            debug!("Failed to publish build started event: {}", e);
        }
        
        let mut built_images = HashMap::new();
        let mut build_errors = Vec::new();
        
        // Build each component
        for spec in &component_specs {
            // Check for cancellation
            if self.is_cancelling() || global_shutdown().is_shutting_down() {
                info!("Build cancelled by shutdown signal");
                return Err(rush_core::error::Error::Cancelled("Build cancelled by user".to_string()));
            }

            let component_span = info_span!(
                "build_component",
                component = %spec.component_name,
                build_type = ?spec.build_type
            );
            let _enter = component_span.enter();

            info!("[BUILD DECISION] Component '{}': Evaluating build requirement", spec.component_name);
            let component_start = Instant::now();

            // Compute current tag
            let current_tag = self.tag_generator.compute_tag(&spec)
                .unwrap_or_else(|e| {
                    warn!("Failed to compute tag for {}: {}, using timestamp",
                        spec.component_name, e);
                    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
                    format!("{}", timestamp)
                });

            let image_name = format!(
                "{}/{}",
                self.config.product_name,
                spec.component_name
            );
            let full_image_name = format!("{}:{}", image_name, current_tag);

            // Check if image already exists with this tag (unless force rebuild)
            if !force_rebuild {
                info!("[BUILD DECISION] Component '{}': Checking if image '{}' exists",
                    spec.component_name, full_image_name);

                // Try to check if Docker image exists locally
                if let Ok(exists) = self.docker_client.image_exists(&full_image_name).await {
                    if exists {
                        info!("[BUILD DECISION] Component '{}': ✓ Image '{}' already exists, skipping build",
                            spec.component_name, full_image_name);

                        // Record skip in performance tracker
                        let skip_duration = component_start.elapsed();
                        perf_tracker.record_with_component(
                            "component_skip",
                            &spec.component_name,
                            skip_duration
                        ).await;

                        built_images.insert(spec.component_name.clone(), full_image_name.clone());

                        // CRITICAL FIX: Always update cache with spec, even when skipping build
                        // This ensures cache invalidation can work on subsequent file changes
                        if self.config.enable_cache {
                            debug!("[BUILD DECISION] Component '{}': Adding to cache with spec for future invalidation",
                                spec.component_name);
                            let mut cache_guard = self.cache.lock().await;
                            cache_guard.put_with_spec(
                                spec.component_name.clone(),
                                full_image_name.clone(),
                                spec.clone(),
                            ).await;
                        }

                        // Update state
                        {
                            let mut state = self.state.write().await;
                            state.mark_component_built(&spec.component_name, full_image_name);
                        }

                        continue;
                    } else {
                        info!("[BUILD DECISION] Component '{}': Image '{}' not found locally, will build",
                            spec.component_name, full_image_name);
                    }
                } else {
                    // If we can't check, assume we need to build
                    info!("[BUILD DECISION] Component '{}': Unable to check image existence, will build",
                        spec.component_name);
                }
            } else {
                info!("[BUILD DECISION] Component '{}': Force rebuild requested, ignoring existing images",
                    spec.component_name);
            }
            
            // Build the component
            info!("[BUILD DECISION] Component '{}': ⚙️  Starting build", spec.component_name);
            match self.build_single(spec.clone(), &component_specs).await {
                Ok(image_name) => {
                    let component_duration = component_start.elapsed();
                    info!("[BUILD DECISION] Component '{}': ✓ Successfully built new image '{}'",
                        spec.component_name, image_name);

                    // Record timing in performance tracker
                    perf_tracker.record_with_component(
                        "component_build",
                        &spec.component_name,
                        component_duration
                    ).await;

                    built_images.insert(spec.component_name.clone(), image_name.clone());
                    
                    // Update cache
                    if self.config.enable_cache {
                        let mut cache_guard = self.cache.lock().await;
                        cache_guard.put_with_spec(
                            spec.component_name.clone(),
                            image_name.clone(),
                            spec.clone(),
                        ).await;
                    }
                    
                    // Update state
                    {
                        let mut state = self.state.write().await;
                        state.mark_component_built(&spec.component_name, image_name.clone());
                    }
                    
                    // Publish success event
                    if let Err(e) = self.event_bus.publish(Event::new(
                        "build",
                        ContainerEvent::BuildCompleted {
                            component: spec.component_name.clone(),
                            success: true,
                            duration: start_time.elapsed(),
                            error: None,
                        },
                    )).await {
                        debug!("Failed to publish build completed event: {}", e);
                    }
                }
                Err(e) => {
                    error!("[BUILD DECISION] Component '{}': ✗ Build failed: {}", spec.component_name, e);
                    build_errors.push((spec.component_name.clone(), e.to_string()));
                    
                    // Update state
                    {
                        let mut state = self.state.write().await;
                        state.record_component_error(&spec.component_name, e.to_string());
                    }
                    
                    // Publish error event
                    if let Err(pub_err) = self.event_bus.publish(Event::error(
                        "build",
                        format!("Failed to build {}: {}", spec.component_name, e),
                        false,
                    )).await {
                        debug!("Failed to publish error event: {}", pub_err);
                    }
                }
            }
        }
        
        // Check if any builds failed
        if !build_errors.is_empty() {
            error!("Build failed for {} components", build_errors.len());
            for (component, error) in &build_errors {
                error!("  {}: {}", component, error);
            }
            return Err(rush_core::error::Error::Build(
                format!("Failed to build {} components", build_errors.len())
            ));
        }
        
        let total_duration = start_time.elapsed();
        info!("All components built successfully in {:?}", total_duration);

        // Record total build time
        perf_tracker.record("build_all_components", total_duration, {
            let mut metadata = HashMap::new();
            metadata.insert("component_count".to_string(), component_specs.len().to_string());
            metadata.insert("force_rebuild".to_string(), force_rebuild.to_string());
            metadata
        }).await;

        Ok(built_images)
    }

    /// Build a single component
    #[instrument(
        level = "debug",
        skip(self, all_specs),
        fields(component = %spec.component_name)
    )]
    pub async fn build_single(&self, spec: ComponentBuildSpec, all_specs: &[ComponentBuildSpec]) -> Result<String> {
        debug!("Building component: {}", spec.component_name);
        let start_time = Instant::now();
        let total_build_start = std::time::Instant::now();

        // Prepare build artifacts
        let artifacts_start = std::time::Instant::now();
        let artifacts_dir = self.prepare_artifacts(&spec).await?;
        crate::profiling::global_tracker()
            .record_with_component("build_single", "prepare_artifacts", artifacts_start.elapsed())
            .await;

        // Determine image name and tag
        let tag_start = std::time::Instant::now();
        let image_name = format!(
            "{}/{}",
            self.config.product_name,
            spec.component_name
        );
        let tag = self.tag_generator.compute_tag(&spec)
            .unwrap_or_else(|e| {
                warn!("Failed to compute tag for {}: {}, using timestamp",
                    spec.component_name, e);
                let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
                format!("{}", timestamp)
            });
        let full_image_name = format!("{}:{}", image_name, tag);
        
        // Build based on type
        match &spec.build_type {
            BuildType::RustBinary { .. } 
            | BuildType::TrunkWasm { .. }
            | BuildType::DixiousWasm { .. }
            | BuildType::Script { .. }
            | BuildType::Zola { .. }
            | BuildType::Book { .. }
            | BuildType::Ingress { .. } => {
                // Build using Dockerfile
                if let Some(dockerfile) = spec.build_type.dockerfile_path() {
                    let dockerfile_path = self.config.product_dir.join(dockerfile);
                    
                    // Run build script first if needed (e.g., compile Rust binary)
                    if let Err(e) = self.run_build_script_for_component(&spec).await {
                        error!("Failed to run build script for {}: {}", spec.component_name, e);
                        return Err(e);
                    }
                    
                    // Determine the Docker build context directory
                    // Context is always relative to the component's location directory
                    // When context_dir is omitted, defaults to the component's location
                    let docker_context = match &spec.build_type {
                        BuildType::TrunkWasm { context_dir, location, .. } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                // context_dir is relative to the component's location
                                component_base.join(ctx)
                            } else {
                                // Default to the component's directory
                                component_base
                            }
                        }
                        BuildType::DixiousWasm { context_dir, location, .. } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                component_base.join(ctx)
                            } else {
                                component_base
                            }
                        }
                        BuildType::RustBinary { context_dir, location, .. } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                component_base.join(ctx)
                            } else {
                                component_base
                            }
                        }
                        BuildType::Book { context_dir, location, .. } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                component_base.join(ctx)
                            } else {
                                component_base
                            }
                        }
                        BuildType::Zola { context_dir, location, .. } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                component_base.join(ctx)
                            } else {
                                component_base
                            }
                        }
                        BuildType::Script { context_dir, location, .. } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                component_base.join(ctx)
                            } else {
                                component_base
                            }
                        }
                        BuildType::Ingress { context_dir, .. } => {
                            // Ingress doesn't have a location field, so context_dir is relative to product
                            if let Some(ctx) = context_dir {
                                self.config.product_dir.join(ctx)
                            } else {
                                // Default to product directory
                                self.config.product_dir.clone()
                            }
                        }
                        _ => self.config.product_dir.clone(),
                    };
                    
                    debug!("Docker context for {}: {}", spec.component_name, docker_context.display());
                    debug!("Dockerfile path: {}", dockerfile_path.display());
                    
                    // Render artifacts (e.g., nginx.conf from templates) to the Docker context
                    if let Err(e) = self.render_artifacts_for_component(&spec, all_specs, &docker_context).await {
                        error!("Failed to render artifacts for {}: {}", spec.component_name, e);
                        return Err(e);
                    }
                    
                    // Build the image
                    self.docker_client.build_image(
                        &full_image_name,
                        &dockerfile_path.to_string_lossy(),
                        &docker_context.to_string_lossy(),
                    ).await?;
                    
                    info!("Built {} in {:?}", spec.component_name, start_time.elapsed());
                    Ok(full_image_name)
                } else {
                    Err(rush_core::error::Error::Build(
                        format!("No Dockerfile specified for {}", spec.component_name)
                    ))
                }
            }
            BuildType::PureDockerImage { image_name_with_tag, .. } => {
                // Use pre-built image
                info!("Using pre-built image for {}: {}", spec.component_name, image_name_with_tag);
                Ok(image_name_with_tag.clone())
            }
            BuildType::LocalService { .. } => {
                // Local services don't need container images
                debug!("Skipping build for local service {}", spec.component_name);
                Ok(String::new())
            }
            BuildType::PureKubernetes => {
                // Pure Kubernetes doesn't need a container image
                debug!("Skipping build for pure kubernetes {}", spec.component_name);
                Ok(String::new())
            }
            BuildType::KubernetesInstallation { .. } => {
                // Kubernetes installation doesn't need a container image
                debug!("Skipping build for kubernetes installation {}", spec.component_name);
                Ok(String::new())
            }
        }
    }

    /// Prepare build artifacts for a component
    pub async fn prepare_artifacts(&self, spec: &ComponentBuildSpec) -> Result<PathBuf> {
        debug!("Preparing artifacts for {}", spec.component_name);
        let artifacts_start = std::time::Instant::now();

        // Create artifacts directory
        let dir_start = std::time::Instant::now();
        let artifacts_dir = self.config.product_dir
            .join(".rush")
            .join("artifacts")
            .join(&spec.component_name);

        tokio::fs::create_dir_all(&artifacts_dir).await
            .map_err(|e| rush_core::error::Error::Io(e))?;

        crate::profiling::global_tracker()
            .record_with_component("prepare_artifacts", "create_dir", dir_start.elapsed())
            .await;

        // Render templates based on build type
        let render_start = std::time::Instant::now();
        match &spec.build_type {
            BuildType::RustBinary { .. } => {
                // Prepare Rust binary artifacts
                self.prepare_rust_artifacts(spec, &artifacts_dir).await?;
            }
            BuildType::TrunkWasm { .. } => {
                // Prepare Trunk/WASM artifacts
                self.prepare_wasm_artifacts(spec, &artifacts_dir).await?;
            }
            _ => {
                // Other build types don't need artifact preparation
                debug!("No artifacts to prepare for build type");
            }
        }
        crate::profiling::global_tracker()
            .record_with_component("prepare_artifacts", "render_templates", render_start.elapsed())
            .await;

        crate::profiling::global_tracker()
            .record_with_component("prepare_artifacts", "total", artifacts_start.elapsed())
            .await;

        Ok(artifacts_dir)
    }

    /// Prepare Rust binary artifacts
    async fn prepare_rust_artifacts(
        &self,
        spec: &ComponentBuildSpec,
        artifacts_dir: &Path,
    ) -> Result<()> {
        debug!("Preparing Rust artifacts for {}", spec.component_name);
        
        // Copy Cargo.toml and source files
        // Get source directory from build type - context_dir is relative to component location
        let source_dir = if let BuildType::RustBinary { context_dir, location, .. } = &spec.build_type {
            let component_base = self.config.product_dir.join(location);
            if let Some(ctx) = context_dir {
                // context_dir is relative to the component's location
                component_base.join(ctx)
            } else {
                // Default to the component's directory itself
                component_base
            }
        } else {
            self.config.product_dir.join(&spec.component_name)
        };
        
        // Copy Cargo.toml
        let cargo_src = source_dir.join("Cargo.toml");
        let cargo_dst = artifacts_dir.join("Cargo.toml");
        if cargo_src.exists() {
            tokio::fs::copy(&cargo_src, &cargo_dst).await
                .map_err(|e| rush_core::error::Error::Io(e))?;
        }
        
        // Copy src directory
        let src_dir = source_dir.join("src");
        let dst_src_dir = artifacts_dir.join("src");
        if src_dir.exists() {
            self.copy_dir_recursive(&src_dir, &dst_src_dir).await?;
        }
        
        Ok(())
    }

    /// Prepare WASM artifacts
    async fn prepare_wasm_artifacts(
        &self,
        spec: &ComponentBuildSpec,
        artifacts_dir: &Path,
    ) -> Result<()> {
        debug!("Preparing WASM artifacts for {}", spec.component_name);
        
        // Copy Trunk.toml and source files
        // Get source directory from build type - context_dir is relative to component location
        let source_dir = if let BuildType::TrunkWasm { context_dir, location, .. } = &spec.build_type {
            let component_base = self.config.product_dir.join(location);
            if let Some(ctx) = context_dir {
                // context_dir is relative to the component's location
                component_base.join(ctx)
            } else {
                // Default to the component's directory itself
                component_base
            }
        } else {
            self.config.product_dir.join(&spec.component_name)
        };
        
        // Copy Trunk.toml if it exists
        let trunk_src = source_dir.join("Trunk.toml");
        let trunk_dst = artifacts_dir.join("Trunk.toml");
        if trunk_src.exists() {
            tokio::fs::copy(&trunk_src, &trunk_dst).await
                .map_err(|e| rush_core::error::Error::Io(e))?;
        }
        
        // Copy index.html
        let index_src = source_dir.join("index.html");
        let index_dst = artifacts_dir.join("index.html");
        if index_src.exists() {
            tokio::fs::copy(&index_src, &index_dst).await
                .map_err(|e| rush_core::error::Error::Io(e))?;
        }
        
        // Copy src directory
        let src_dir = source_dir.join("src");
        let dst_src_dir = artifacts_dir.join("src");
        if src_dir.exists() {
            self.copy_dir_recursive(&src_dir, &dst_src_dir).await?;
        }
        
        Ok(())
    }

    /// Copy directory recursively
    fn copy_dir_recursive<'a>(&'a self, src: &'a Path, dst: &'a Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
        tokio::fs::create_dir_all(dst).await
            .map_err(|e| rush_core::error::Error::Io(e))?;
        
        let mut entries = tokio::fs::read_dir(src).await
            .map_err(|e| rush_core::error::Error::Io(e))?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| rush_core::error::Error::Io(e))? {
            let path = entry.path();
            let file_name = entry.file_name();
            let dst_path = dst.join(&file_name);
            
            if path.is_dir() {
                self.copy_dir_recursive(&path, &dst_path).await?;
            } else {
                tokio::fs::copy(&path, &dst_path).await
                    .map_err(|e| rush_core::error::Error::Io(e))?;
            }
        }
        
        Ok(())
        })
    }


    /// Get build statistics
    pub async fn get_stats(&self) -> CacheStats {
        let cache_guard = self.cache.lock().await;
        cache_guard.get_stats()
    }

    /// Clear the build cache
    pub async fn clear_cache(&self) -> Result<()> {
        let mut cache_guard = self.cache.lock().await;
        cache_guard.clear().await;
        Ok(())
    }

    /// Invalidate cache entries based on file changes
    pub async fn invalidate_cache_for_files(&self, changed_files: &[PathBuf]) -> Result<()> {
        let mut cache_guard = self.cache.lock().await;
        cache_guard.invalidate_changed(changed_files).await;
        info!("Invalidated cache entries for {} changed files", changed_files.len());
        Ok(())
    }
    
    /// Runs the build script for a component before Docker build
    async fn run_build_script_for_component(&self, spec: &ComponentBuildSpec) -> Result<()> {
        use rush_build::{BuildContext, BuildScript};
        use rush_toolchain::{Platform, ToolchainContext};
        
        // Skip components that don't need build scripts
        if !spec.build_type.requires_docker_build() {
            return Ok(());
        }
        
        info!("Running build script for {}", spec.component_name);
        
        // Create toolchain context for cross-compilation
        let host_platform = Platform::default();
        let target_platform = Platform::new("linux", "x86_64");
        let toolchain = ToolchainContext::create_with_platforms(host_platform.clone(), target_platform.clone());
        toolchain.setup_env();
        
        // Get location from build type
        let location = spec.build_type.location().unwrap_or(".");
        
        // Create build context
        let context = BuildContext {
            build_type: spec.build_type.clone(),
            location: Some(location.to_string()),
            target: target_platform,
            host: host_platform,
            rust_target: "x86_64-unknown-linux-gnu".to_string(),
            toolchain,
            services: Default::default(),
            environment: "local".to_string(),
            domain: spec.domain.clone(),
            product_name: spec.product_name.clone(),
            product_uri: format!("{}.local", spec.product_name),
            component: spec.component_name.clone(),
            docker_registry: String::new(),
            image_name: rush_core::naming::NamingConvention::image_name(&spec.product_name, &spec.component_name),
            domains: Default::default(),
            env: {
                // Merge dotenv and dotenv_secrets for build context
                let mut env = spec.dotenv.clone();
                env.extend(spec.dotenv_secrets.clone());
                env
            },
            secrets: Default::default(),
            cross_compile: spec.cross_compile.clone(),
        };
        
        // Generate and run build script
        let build_script = BuildScript::new(spec.build_type.clone());
        let script_content = build_script.render(&context);
        
        if script_content.is_empty() {
            debug!("No build script for {}", spec.component_name);
            return Ok(());
        }
        
        // Execute the build script
        let script_path = self.config.product_dir.join(".rush").join("build.sh");
        debug!("Creating script at: {}", script_path.display());
        std::fs::create_dir_all(script_path.parent().unwrap())?;
        std::fs::write(&script_path, script_content)?;
        debug!("Script written successfully to: {}", script_path.display());
        
        // Use output capture if sink is available
        let sink_option = {
            let output_sink_guard = self.output_sink.read().await;
            output_sink_guard.clone()
        };
        
        if let Some(sink) = sink_option {
            simple_output::follow_build_output_simple(
                spec.component_name.clone(),
                vec!["/bin/sh".to_string(), script_path.to_string_lossy().to_string()],
                sink,
                Some(self.config.product_dir.clone()),
            ).await?;
        } else {
            // Fallback to direct execution without output capture
            let output = tokio::process::Command::new("/bin/sh")
                .arg(&script_path)
                .current_dir(&self.config.product_dir)
                .output()
                .await
                .map_err(|e| rush_core::error::Error::Build(format!("Failed to run build script: {}", e)))?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(rush_core::error::Error::Build(
                    format!("Build script failed for {}: {}", spec.component_name, stderr)
                ));
            }
        }
        
        info!("Build script completed for {}", spec.component_name);
        Ok(())
    }
    
    /// Renders artifacts for a component before Docker build
    async fn render_artifacts_for_component(&self, spec: &ComponentBuildSpec, all_specs: &[ComponentBuildSpec], docker_context: &Path) -> Result<()> {
        use rush_build::{Artefact, BuildContext};
        use rush_toolchain::{Platform, ToolchainContext};
        use std::collections::HashMap;
        
        // Check if this component has artifacts to render
        if spec.artefacts.is_none() {
            debug!("No artifacts to render for {}", spec.component_name);
            return Ok(());
        }
        
        let artifact_count = spec.artefacts.as_ref().map(|a| a.len()).unwrap_or(0);
        info!("Rendering {} artifacts for {}", artifact_count, spec.component_name);
        
        // Create build context for rendering
        let host_platform = Platform::default();
        let target_platform = Platform::new("linux", "x86_64");
        let toolchain = ToolchainContext::default();
        
        let location = spec.build_type.location().unwrap_or(".");
        
        // For Ingress, we need special handling for services
        let services = if let rush_build::BuildType::Ingress { components, .. } = &spec.build_type {
            // Build a services map using actual ports from component specs
            let mut services_map = HashMap::new();
            for component_name in components {
                // Find the actual component spec to get its resolved ports
                let component_spec = all_specs.iter()
                    .find(|s| &s.component_name == component_name);
                    
                if let Some(component_spec) = component_spec {
                    let service_spec = rush_build::ServiceSpec {
                        name: component_name.clone(),
                        host: rush_core::naming::NamingConvention::container_name(&spec.product_name, &component_name),
                        port: component_spec.port.unwrap_or(8080),
                        target_port: component_spec.target_port.unwrap_or(80),
                        mount_point: component_spec.mount_point.clone(),
                        domain: component_spec.domain.clone(),
                        docker_host: rush_core::naming::NamingConvention::container_name(&spec.product_name, &component_name),
                    };
                    services_map.entry(component_spec.domain.clone())
                        .or_insert_with(Vec::new)
                        .push(service_spec);
                } else {
                    warn!("Component {} referenced by ingress not found in specs", component_name);
                }
            }
            services_map
        } else {
            HashMap::new()
        };
        
        let context = BuildContext {
            build_type: spec.build_type.clone(),
            location: Some(location.to_string()),
            target: target_platform,
            host: host_platform,
            rust_target: "x86_64-unknown-linux-gnu".to_string(),
            toolchain,
            services,
            environment: "local".to_string(),
            domain: spec.domain.clone(),
            product_name: spec.product_name.clone(),
            product_uri: format!("{}.local", spec.product_name),
            component: spec.component_name.clone(),
            docker_registry: String::new(),
            image_name: rush_core::naming::NamingConvention::image_name(&spec.product_name, &spec.component_name),
            domains: Default::default(),
            env: {
                // Merge dotenv and dotenv_secrets for build context
                let mut env = spec.dotenv.clone();
                env.extend(spec.dotenv_secrets.clone());
                env
            },
            secrets: Default::default(),
            cross_compile: spec.cross_compile.clone(),
        };
        
        // Render each artifact from the spec
        if let Some(artefacts_map) = &spec.artefacts {
            for (input_path, output_path) in artefacts_map {
                // Create the artifact
                let full_input_path = self.config.product_dir.join(input_path);
                let artefact = Artefact::new(
                    full_input_path.to_string_lossy().to_string(),
                    output_path.clone()
                )?;
                
                // Render the artifact
                match artefact.render(&context) {
                    Ok(content) => {
                        // 1. Write to .rush/artifacts for tracking and cache
                        let rush_output_path = self.config.product_dir
                            .join(".rush")
                            .join("artifacts")
                            .join(&spec.component_name)
                            .join(output_path);
                            
                        std::fs::create_dir_all(rush_output_path.parent().unwrap())?;
                        std::fs::write(&rush_output_path, &content)?;
                        debug!("Rendered artifact to .rush: {}", rush_output_path.display());
                        
                        // 2. Write to the Docker build context's dist directory
                        // Use the docker_context that was passed in, which is already correctly calculated
                        let dist_output_path = docker_context.join("dist").join(output_path);
                        debug!("Docker context: {}", docker_context.display());
                        debug!("Dist output path: {}", dist_output_path.display());
                        
                        std::fs::create_dir_all(dist_output_path.parent().unwrap())?;
                        std::fs::write(&dist_output_path, &content)?;
                        info!("Rendered artifact to component dist: {}", dist_output_path.display());
                        
                        // Verify the file was written
                        if dist_output_path.exists() {
                            debug!("Verified artifact exists at: {}", dist_output_path.display());
                        } else {
                            error!("WARNING: Artifact was NOT written to: {}", dist_output_path.display());
                        }
                    }
                    Err(e) => {
                        warn!("Failed to render artifact {}: {}", input_path, e);
                    }
                }
            }
        }
        
        info!("Artifacts rendered for {}", spec.component_name);
        Ok(())
    }
}