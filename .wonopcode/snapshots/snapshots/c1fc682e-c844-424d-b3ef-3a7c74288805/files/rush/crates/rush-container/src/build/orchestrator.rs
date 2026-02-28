//! Build orchestration for container images
//!
//! This module coordinates the building of multiple components,
//! handling dependencies and parallel builds where possible.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use rush_build::{BuildType, ComponentBuildSpec};
use rush_core::error::Result;
use rush_core::TargetArchitecture;
use rush_core::shutdown::global_shutdown;
use rush_output::simple::Sink;
use rush_toolchain::ToolchainContext;
use tokio::sync::Mutex;
use tracing::{info_span, instrument};

use crate::build::{BuildCache, BuildProcessor, CacheStats, ParallelBuildExecutor};
use crate::docker::DockerClient;
use crate::events::{ContainerEvent, Event, EventBus};
use crate::reactor::state::SharedReactorState;
use crate::tagging::ImageTagGenerator;
use crate::{profiling, simple_output};

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

    /// Resolve paths based on product_dir
    /// This should be called after product_dir is set
    pub fn resolve_paths(&mut self) {
        // Phase 4 validation: Check that product_dir is set
        if self.product_dir.as_os_str().is_empty() {
            warn!("Cannot resolve cache_dir: product_dir is not set");
            return;
        }

        // If cache_dir is not set (empty), derive it from product_dir
        if self.cache_dir.as_os_str().is_empty() {
            self.cache_dir = self.product_dir.join(".rush/cache");
            debug!("Resolved cache_dir to: {}", self.cache_dir.display());
        }

        // Phase 4 validation: Warn if cache_dir is outside product_dir
        if !self.cache_dir.starts_with(&self.product_dir) {
            warn!(
                "Cache directory '{}' is outside product directory '{}'.                 This may cause inconsistent behavior when running from different directories.",
                self.cache_dir.display(),
                self.product_dir.display()
            );
        }
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
            cache_dir: PathBuf::new(),  // Will be resolved from product_dir
        }
    }
}

/// Type alias for the output sink to reduce complexity
type OutputSink = Arc<tokio::sync::RwLock<Option<Arc<tokio::sync::Mutex<Box<dyn Sink>>>>>>;

/// Orchestrates the building of container images
pub struct BuildOrchestrator {
    config: BuildOrchestratorConfig,
    docker_client: Arc<dyn DockerClient>,
    event_bus: EventBus,
    state: SharedReactorState,
    pub(crate) cache: Arc<Mutex<BuildCache>>,
    _build_processor: Arc<BuildProcessor>,
    pub(crate) tag_generator: Arc<ImageTagGenerator>,
    /// Output sink for build logs
    output_sink: OutputSink,
    /// Track active build containers for cancellation
    active_builds: Arc<Mutex<HashSet<String>>>,
    /// Flag to indicate builds are being cancelled
    cancelling: Arc<AtomicBool>,
    /// Flag to track if this is the first build (initial startup)
    /// Ingress always rebuilds on initial startup
    is_initial_build: Arc<AtomicBool>,
}

impl BuildOrchestrator {
    /// Create a new build orchestrator
    pub fn new(
        config: BuildOrchestratorConfig,
        docker_client: Arc<dyn DockerClient>,
        event_bus: EventBus,
        state: SharedReactorState,
    ) -> Self {
        let cache = Arc::new(Mutex::new(BuildCache::new(
            &config.cache_dir,
            &config.product_dir,
        )));
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
            _build_processor: build_processor,
            tag_generator,
            output_sink: Arc::new(tokio::sync::RwLock::new(None)),
            active_builds: Arc::new(Mutex::new(HashSet::new())),
            cancelling: Arc::new(AtomicBool::new(false)),
            is_initial_build: Arc::new(AtomicBool::new(true)),
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
        let cache = Arc::new(Mutex::new(BuildCache::new(
            &config.cache_dir,
            &config.product_dir,
        )));
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
            _build_processor: build_processor,
            tag_generator,
            output_sink: Arc::new(tokio::sync::RwLock::new(None)),
            active_builds: Arc::new(Mutex::new(HashSet::new())),
            cancelling: Arc::new(AtomicBool::new(false)),
            is_initial_build: Arc::new(AtomicBool::new(true)),
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
            info!("Killing build container: {container_id}");
            if let Err(e) = self.docker_client.kill_container(&container_id).await {
                warn!("Failed to kill build container {container_id}: {e}");
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
            let executor = ParallelBuildExecutor::new(Arc::clone(&self), self.config.max_parallel);
            executor
                .build_optimized(component_specs, force_rebuild)
                .await
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
        if let Err(e) = self
            .event_bus
            .publish(Event::new(
                "build",
                ContainerEvent::BuildStarted {
                    component: "all".to_string(),
                    timestamp: Instant::now(),
                },
            ))
            .await
        {
            debug!("Failed to publish build started event: {e}");
        }

        let mut built_images = HashMap::new();
        let mut build_errors = Vec::new();
        let mut components_actually_rebuilt = Vec::new();

        // Separate ingress components from non-ingress components
        // Ingress needs to be built last and should always rebuild if any other component rebuilds
        let (ingress_specs, non_ingress_specs): (Vec<ComponentBuildSpec>, Vec<ComponentBuildSpec>) = component_specs
            .into_iter()
            .partition(|spec| matches!(spec.build_type, BuildType::Ingress { .. }));
        
        // Collect all specs for context (needed for artifact rendering)
        let all_specs: Vec<ComponentBuildSpec> = ingress_specs.iter().chain(non_ingress_specs.iter()).cloned().collect();

        // Build non-ingress components first
        for spec in &non_ingress_specs {
            // Check for cancellation
            if self.is_cancelling() || global_shutdown().is_shutting_down() {
                info!("Build cancelled by shutdown signal");
                return Err(rush_core::error::Error::Cancelled(
                    "Build cancelled by user".to_string(),
                ));
            }

            let component_span = info_span!(
                "build_component",
                component = %spec.component_name,
                build_type = ?spec.build_type
            );
            let _enter = component_span.enter();

            info!(
                "[BUILD DECISION] Component '{}': Evaluating build requirement",
                spec.component_name
            );
            let component_start = Instant::now();

            // Compute current tag
            let current_tag = self.tag_generator.compute_tag(spec).unwrap_or_else(|e| {
                warn!(
                    "Failed to compute tag for {}: {}, using timestamp",
                    spec.component_name, e
                );
                let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
                format!("{timestamp}")
            });

            let image_name = format!("{}/{}", self.config.product_name, spec.component_name);
            let full_image_name = format!("{image_name}:{current_tag}");

            // Check if image already exists with this tag (unless force rebuild)
            // Bazel components always rebuild - Bazel handles its own caching
            let is_bazel = matches!(spec.build_type, BuildType::Bazel { .. });
            if !force_rebuild && !is_bazel {
                info!(
                    "[BUILD DECISION] Component '{}': Checking if image '{}' exists",
                    spec.component_name, full_image_name
                );

                // Try to check if Docker image exists locally
                if let Ok(exists) = self.docker_client.image_exists(&full_image_name).await {
                    if exists {
                        info!("[BUILD DECISION] Component '{}': ✓ Image '{}' already exists, skipping build",
                            spec.component_name, full_image_name);

                        // Record skip in performance tracker
                        let skip_duration = component_start.elapsed();
                        perf_tracker
                            .record_with_component(
                                "component_skip",
                                &spec.component_name,
                                skip_duration,
                            )
                            .await;

                        built_images.insert(spec.component_name.clone(), full_image_name.clone());

                        // CRITICAL FIX: Always update cache with spec, even when skipping build
                        // This ensures cache invalidation can work on subsequent file changes
                        if self.config.enable_cache {
                            debug!("[BUILD DECISION] Component '{}': Adding to cache with spec for future invalidation",
                                spec.component_name);
                            let mut cache_guard = self.cache.lock().await;
                            cache_guard
                                .put_with_spec(
                                    spec.component_name.clone(),
                                    full_image_name.clone(),
                                    spec.clone(),
                                )
                                .await;
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
            } else if is_bazel {
                info!("[BUILD DECISION] Component '{}': Bazel component - always rebuilding (Bazel handles caching)",
                    spec.component_name);
            } else {
                info!("[BUILD DECISION] Component '{}': Force rebuild requested, ignoring existing images",
                    spec.component_name);
            }

            // Build the component
            info!(
                "[BUILD DECISION] Component '{}': ⚙️  Starting build",
                spec.component_name
            );
            match self.build_single(spec.clone(), &all_specs).await {
                Ok(image_name) => {
                    let component_duration = component_start.elapsed();
                    info!(
                        "[BUILD DECISION] Component '{}': ✓ Successfully built new image '{}'",
                        spec.component_name, image_name
                    );

                    // Record timing in performance tracker
                    perf_tracker
                        .record_with_component(
                            "component_build",
                            &spec.component_name,
                            component_duration,
                        )
                        .await;

                    built_images.insert(spec.component_name.clone(), image_name.clone());
                    
                    // Track that this component was actually rebuilt (not skipped)
                    components_actually_rebuilt.push(spec.component_name.clone());

                    // Update cache
                    if self.config.enable_cache {
                        let mut cache_guard = self.cache.lock().await;
                        cache_guard
                            .put_with_spec(
                                spec.component_name.clone(),
                                image_name.clone(),
                                spec.clone(),
                            )
                            .await;
                    }

                    // Update state
                    {
                        let mut state = self.state.write().await;
                        state.mark_component_built(&spec.component_name, image_name.clone());
                    }

                    // Publish success event
                    if let Err(e) = self
                        .event_bus
                        .publish(Event::new(
                            "build",
                            ContainerEvent::BuildCompleted {
                                component: spec.component_name.clone(),
                                success: true,
                                duration: start_time.elapsed(),
                                error: None,
                            },
                        ))
                        .await
                    {
                        debug!("Failed to publish build completed event: {e}");
                    }
                }
                Err(e) => {
                    error!(
                        "[BUILD DECISION] Component '{}': ✗ Build failed: {}",
                        spec.component_name, e
                    );
                    build_errors.push((spec.component_name.clone(), e.to_string()));

                    // Update state
                    {
                        let mut state = self.state.write().await;
                        state.record_component_error(&spec.component_name, e.to_string());
                    }

                    // Publish error event
                    if let Err(pub_err) = self
                        .event_bus
                        .publish(Event::error(
                            "build",
                            format!("Failed to build {}: {}", spec.component_name, e),
                            false,
                        ))
                        .await
                    {
                        debug!("Failed to publish error event: {pub_err}");
                    }
                }
            }
        }

        // Now build ingress components
        // Ingress always rebuilds on initial startup OR if any other component was rebuilt
        // This ensures the nginx configuration always reflects the current state of all components
        let is_initial = self.is_initial_build.swap(false, Ordering::SeqCst);
        
        for spec in &ingress_specs {
            // Check for cancellation
            if self.is_cancelling() || global_shutdown().is_shutting_down() {
                info!("Build cancelled by shutdown signal");
                return Err(rush_core::error::Error::Cancelled(
                    "Build cancelled by user".to_string(),
                ));
            }

            let component_span = info_span!(
                "build_component",
                component = %spec.component_name,
                build_type = ?spec.build_type
            );
            let _enter = component_span.enter();

            // Determine if we should force rebuild the ingress
            // Always rebuild if: force_rebuild is set, OR this is initial startup, OR any other component was rebuilt
            let force_ingress_rebuild = force_rebuild || is_initial || !components_actually_rebuilt.is_empty();
            
            if !components_actually_rebuilt.is_empty() {
                info!(
                    "[BUILD DECISION] Component '{}': Force rebuilding because {} component(s) were rebuilt: {:?}",
                    spec.component_name,
                    components_actually_rebuilt.len(),
                    components_actually_rebuilt
                );
            } else if is_initial {
                info!(
                    "[BUILD DECISION] Component '{}': Force rebuilding on initial startup",
                    spec.component_name
                );
            } else if force_rebuild {
                info!(
                    "[BUILD DECISION] Component '{}': Force rebuilding on startup",
                    spec.component_name
                );
            }

            info!(
                "[BUILD DECISION] Component '{}': Evaluating build requirement",
                spec.component_name
            );
            let component_start = Instant::now();

            // Compute current tag
            let current_tag = self.tag_generator.compute_tag(spec).unwrap_or_else(|e| {
                warn!(
                    "Failed to compute tag for {}: {}, using timestamp",
                    spec.component_name, e
                );
                let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
                format!("{timestamp}")
            });

            let image_name = format!("{}/{}", self.config.product_name, spec.component_name);
            let full_image_name = format!("{image_name}:{current_tag}");

            // For ingress, we always rebuild if force_ingress_rebuild is true
            // Bazel components always rebuild (Bazel handles its own caching)
            // Otherwise, check if image exists
            let is_bazel = matches!(spec.build_type, BuildType::Bazel { .. });
            let should_build = if force_ingress_rebuild {
                info!("[BUILD DECISION] Component '{}': Force rebuild - will rebuild regardless of existing image",
                    spec.component_name);
                true
            } else if is_bazel {
                info!("[BUILD DECISION] Component '{}': Bazel component - always rebuilding (Bazel handles caching)",
                    spec.component_name);
                true
            } else {
                // Check if image already exists with this tag
                info!(
                    "[BUILD DECISION] Component '{}': Checking if image '{}' exists",
                    spec.component_name, full_image_name
                );

                match self.docker_client.image_exists(&full_image_name).await {
                    Ok(exists) => {
                        if exists {
                            info!("[BUILD DECISION] Component '{}': ✓ Image '{}' already exists, skipping build",
                                spec.component_name, full_image_name);
                            
                            // Record skip in performance tracker
                            let skip_duration = component_start.elapsed();
                            perf_tracker
                                .record_with_component(
                                    "component_skip",
                                    &spec.component_name,
                                    skip_duration,
                                )
                                .await;

                            built_images.insert(spec.component_name.clone(), full_image_name.clone());

                            if self.config.enable_cache {
                                debug!("[BUILD DECISION] Component '{}': Adding to cache with spec for future invalidation",
                                    spec.component_name);
                                let mut cache_guard = self.cache.lock().await;
                                cache_guard
                                    .put_with_spec(
                                        spec.component_name.clone(),
                                        full_image_name.clone(),
                                        spec.clone(),
                                    )
                                    .await;
                            }

                            {
                                let mut state = self.state.write().await;
                                state.mark_component_built(&spec.component_name, full_image_name);
                            }

                            false // Don't build
                        } else {
                            info!("[BUILD DECISION] Component '{}': Image '{}' not found locally, will build",
                                spec.component_name, full_image_name);
                            true
                        }
                    }
                    Err(_) => {
                        info!("[BUILD DECISION] Component '{}': Unable to check image existence, will build",
                            spec.component_name);
                        true
                    }
                }
            };

            if should_build {
                info!(
                    "[BUILD DECISION] Component '{}': ⚙️  Starting build",
                    spec.component_name
                );
                match self.build_single(spec.clone(), &all_specs).await {
                    Ok(image_name) => {
                        let component_duration = component_start.elapsed();
                        info!(
                            "[BUILD DECISION] Component '{}': ✓ Successfully built new image '{}'",
                            spec.component_name, image_name
                        );

                        perf_tracker
                            .record_with_component(
                                "component_build",
                                &spec.component_name,
                                component_duration,
                            )
                            .await;

                        built_images.insert(spec.component_name.clone(), image_name.clone());

                        if self.config.enable_cache {
                            let mut cache_guard = self.cache.lock().await;
                            cache_guard
                                .put_with_spec(
                                    spec.component_name.clone(),
                                    image_name.clone(),
                                    spec.clone(),
                                )
                                .await;
                        }

                        {
                            let mut state = self.state.write().await;
                            state.mark_component_built(&spec.component_name, image_name.clone());
                        }

                        if let Err(e) = self
                            .event_bus
                            .publish(Event::new(
                                "build",
                                ContainerEvent::BuildCompleted {
                                    component: spec.component_name.clone(),
                                    success: true,
                                    duration: start_time.elapsed(),
                                    error: None,
                                },
                            ))
                            .await
                        {
                            debug!("Failed to publish build completed event: {e}");
                        }
                    }
                    Err(e) => {
                        error!(
                            "[BUILD DECISION] Component '{}': ✗ Build failed: {}",
                            spec.component_name, e
                        );
                        build_errors.push((spec.component_name.clone(), e.to_string()));

                        {
                            let mut state = self.state.write().await;
                            state.record_component_error(&spec.component_name, e.to_string());
                        }

                        if let Err(pub_err) = self
                            .event_bus
                            .publish(Event::error(
                                "build",
                                format!("Failed to build {}: {}", spec.component_name, e),
                                false,
                            ))
                            .await
                        {
                            debug!("Failed to publish error event: {pub_err}");
                        }
                    }
                }
            }
        }

        // Check if any builds failed
        if !build_errors.is_empty() {
            error!("Build failed for {} components", build_errors.len());
            for (component, error) in &build_errors {
                error!("  {component}: {error}");
            }
            return Err(rush_core::error::Error::Build(format!(
                "Failed to build {} components",
                build_errors.len()
            )));
        }

        let total_duration = start_time.elapsed();
        info!("All components built successfully in {:?}", total_duration);

        // Record total build time
        perf_tracker
            .record("build_all_components", total_duration, {
                let mut metadata = HashMap::new();
                metadata.insert(
                    "component_count".to_string(),
                    all_specs.len().to_string(),
                );
                metadata.insert("force_rebuild".to_string(), force_rebuild.to_string());
                metadata
            })
            .await;

        Ok(built_images)
    }

    /// Build a single component
    #[instrument(
        level = "debug",
        skip(self, all_specs),
        fields(component = %spec.component_name)
    )]
    pub async fn build_single(
        &self,
        spec: ComponentBuildSpec,
        all_specs: &[ComponentBuildSpec],
    ) -> Result<String> {
        debug!("Building component: {}", spec.component_name);
        let start_time = Instant::now();
        let _total_build_start = std::time::Instant::now();

        // Prepare build artifacts
        let artifacts_start = std::time::Instant::now();
        let _artifacts_dir = self.prepare_artifacts(&spec).await?;
        crate::profiling::global_tracker()
            .record_with_component(
                "build_single",
                "prepare_artifacts",
                artifacts_start.elapsed(),
            )
            .await;

        // Determine image name and tag
        let _tag_start = std::time::Instant::now();
        let image_name = format!("{}/{}", self.config.product_name, spec.component_name);
        let tag = self.tag_generator.compute_tag(&spec).unwrap_or_else(|e| {
            warn!(
                "Failed to compute tag for {}: {}, using timestamp",
                spec.component_name, e
            );
            let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            format!("{timestamp}")
        });
        let full_image_name = format!("{image_name}:{tag}");

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
                        error!(
                            "Failed to run build script for {}: {}",
                            spec.component_name, e
                        );
                        return Err(e);
                    }

                    // Determine the Docker build context directory
                    // Context is always relative to the component's location directory
                    // When context_dir is omitted, defaults to the component's location
                    let docker_context = match &spec.build_type {
                        BuildType::TrunkWasm {
                            context_dir,
                            location,
                            ..
                        } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                // context_dir is relative to the component's location
                                component_base.join(ctx)
                            } else {
                                // Default to the component's directory
                                component_base
                            }
                        }
                        BuildType::DixiousWasm {
                            context_dir,
                            location,
                            ..
                        } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                component_base.join(ctx)
                            } else {
                                component_base
                            }
                        }
                        BuildType::RustBinary {
                            context_dir,
                            location,
                            ..
                        } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                component_base.join(ctx)
                            } else {
                                component_base
                            }
                        }
                        BuildType::Book {
                            context_dir,
                            location,
                            ..
                        } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                component_base.join(ctx)
                            } else {
                                component_base
                            }
                        }
                        BuildType::Zola {
                            context_dir,
                            location,
                            ..
                        } => {
                            let component_base = self.config.product_dir.join(location);
                            if let Some(ctx) = context_dir {
                                component_base.join(ctx)
                            } else {
                                component_base
                            }
                        }
                        BuildType::Script {
                            context_dir,
                            location,
                            ..
                        } => {
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

                    debug!(
                        "Docker context for {}: {}",
                        spec.component_name,
                        docker_context.display()
                    );
                    debug!("Dockerfile path: {}", dockerfile_path.display());

                    // Render artifacts (e.g., nginx.conf from templates) to the Docker context
                    if let Err(e) = self
                        .render_artifacts_for_component(&spec, all_specs, &docker_context)
                        .await
                    {
                        error!(
                            "Failed to render artifacts for {}: {}",
                            spec.component_name, e
                        );
                        return Err(e);
                    }

                    // Build the image
                    self.docker_client
                        .build_image(
                            &full_image_name,
                            &dockerfile_path.to_string_lossy(),
                            &docker_context.to_string_lossy(),
                        )
                        .await?;

                    info!(
                        "Built {} in {:?}",
                        spec.component_name,
                        start_time.elapsed()
                    );
                    Ok(full_image_name)
                } else {
                    Err(rush_core::error::Error::Build(format!(
                        "No Dockerfile specified for {}",
                        spec.component_name
                    )))
                }
            }
            BuildType::PureDockerImage {
                image_name_with_tag,
                ..
            } => {
                // Use pre-built image
                info!(
                    "Using pre-built image for {}: {}",
                    spec.component_name, image_name_with_tag
                );
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
                debug!(
                    "Skipping build for kubernetes installation {}",
                    spec.component_name
                );
                Ok(String::new())
            }
            BuildType::Bazel {
                location,
                output_dir,
                targets,
                additional_args,
                base_image,
                oci_load_target,
                ..
            } => {
                info!("Building Bazel component: {}", spec.component_name);

                // Resolve workspace path
                let workspace_path = self.config.product_dir.join(location);

                // Validate workspace exists (check for MODULE.bazel for bzlmod, or WORKSPACE for legacy)
                if !workspace_path.join("MODULE.bazel").exists()
                    && !workspace_path.join("WORKSPACE").exists()
                    && !workspace_path.join("WORKSPACE.bazel").exists()
                {
                    return Err(rush_core::error::Error::Build(format!(
                        "No MODULE.bazel or WORKSPACE file found in {}",
                        workspace_path.display()
                    )));
                }

                // Ensure .bazelrc exists with correct toolchain configuration (macOS)
                Self::ensure_bazelrc(&workspace_path).await?;

                // Check if using rules_oci (oci_load_target is set)
                if let Some(load_target) = oci_load_target {
                    // Use Bazel to build and load OCI image directly into Docker
                    // Pass the expected Rush image name so it can be re-tagged after loading
                    self.run_bazel_oci_load(
                        &workspace_path,
                        load_target,
                        &full_image_name,
                        additional_args.as_ref(),
                    )
                    .await?;

                    info!(
                        "Built and loaded Bazel OCI image for {} in {:?}",
                        spec.component_name,
                        start_time.elapsed()
                    );

                    // The image is now tagged with the Rush image name
                    Ok(full_image_name)
                } else {
                    // Legacy path: Bazel build + Dockerfile generation
                    // Execute Bazel build
                    self.run_bazel_build(&workspace_path, targets.as_ref(), additional_args.as_ref())
                        .await?;

                    // Resolve output directory
                    let output_path = if std::path::Path::new(output_dir).is_absolute() {
                        std::path::PathBuf::from(output_dir)
                    } else {
                        workspace_path.join(output_dir)
                    };

                    // Create output directory if needed
                    tokio::fs::create_dir_all(&output_path)
                        .await
                        .map_err(rush_core::error::Error::Io)?;

                    // Generate Dockerfile for OCI image
                    let dockerfile_path = self
                        .generate_bazel_dockerfile(&output_path, &workspace_path, base_image.as_deref())
                        .await?;

                    // Build Docker image
                    self.docker_client
                        .build_image(
                            &full_image_name,
                            &dockerfile_path.to_string_lossy(),
                            &output_path.to_string_lossy(),
                        )
                        .await?;

                    info!(
                        "Built Bazel component {} in {:?}",
                        spec.component_name,
                        start_time.elapsed()
                    );

                    Ok(full_image_name)
                }
            }
        }
    }

    /// Prepare build artifacts for a component
    pub async fn prepare_artifacts(&self, spec: &ComponentBuildSpec) -> Result<PathBuf> {
        debug!("Preparing artifacts for {}", spec.component_name);
        let artifacts_start = std::time::Instant::now();

        // Create artifacts directory
        let dir_start = std::time::Instant::now();
        let artifacts_dir = self
            .config
            .product_dir
            .join(".rush")
            .join("artifacts")
            .join(&spec.component_name);

        tokio::fs::create_dir_all(&artifacts_dir)
            .await
            .map_err(rush_core::error::Error::Io)?;

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
            .record_with_component(
                "prepare_artifacts",
                "render_templates",
                render_start.elapsed(),
            )
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
        let source_dir = if let BuildType::RustBinary {
            context_dir,
            location,
            ..
        } = &spec.build_type
        {
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
            tokio::fs::copy(&cargo_src, &cargo_dst)
                .await
                .map_err(rush_core::error::Error::Io)?;
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
        let source_dir = if let BuildType::TrunkWasm {
            context_dir,
            location,
            ..
        } = &spec.build_type
        {
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
            tokio::fs::copy(&trunk_src, &trunk_dst)
                .await
                .map_err(rush_core::error::Error::Io)?;
        }

        // Copy index.html
        let index_src = source_dir.join("index.html");
        let index_dst = artifacts_dir.join("index.html");
        if index_src.exists() {
            tokio::fs::copy(&index_src, &index_dst)
                .await
                .map_err(rush_core::error::Error::Io)?;
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
    #[allow(clippy::only_used_in_recursion)]
    fn copy_dir_recursive<'a>(
        &'a self,
        src: &'a Path,
        dst: &'a Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            tokio::fs::create_dir_all(dst)
                .await
                .map_err(rush_core::error::Error::Io)?;

            let mut entries = tokio::fs::read_dir(src)
                .await
                .map_err(rush_core::error::Error::Io)?;

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(rush_core::error::Error::Io)?
            {
                let path = entry.path();
                let file_name = entry.file_name();
                let dst_path = dst.join(&file_name);

                if path.is_dir() {
                    self.copy_dir_recursive(&path, &dst_path).await?;
                } else {
                    tokio::fs::copy(&path, &dst_path)
                        .await
                        .map_err(rush_core::error::Error::Io)?;
                }
            }

            Ok(())
        })
    }

    /// Find the bazel/bazelisk binary path
    /// 
    /// Searches common installation locations for bazel or bazelisk
    fn find_bazel_binary() -> Result<String> {
        // First check if 'bazel' is in PATH
        if let Ok(path) = std::env::var("PATH") {
            for dir in path.split(':') {
                let bazel_path = std::path::Path::new(dir).join("bazel");
                if bazel_path.exists() {
                    return Ok(bazel_path.to_string_lossy().to_string());
                }
                let bazelisk_path = std::path::Path::new(dir).join("bazelisk");
                if bazelisk_path.exists() {
                    return Ok(bazelisk_path.to_string_lossy().to_string());
                }
            }
        }

        // Common locations for bazel/bazelisk on macOS and Linux
        let common_paths = [
            "/opt/homebrew/bin/bazel",           // Homebrew on Apple Silicon
            "/opt/homebrew/bin/bazelisk",        // Bazelisk on Apple Silicon
            "/usr/local/bin/bazel",              // Homebrew on Intel Mac / Linux
            "/usr/local/bin/bazelisk",           // Bazelisk on Intel Mac
            "/home/linuxbrew/.linuxbrew/bin/bazel", // Linuxbrew
            "/home/linuxbrew/.linuxbrew/bin/bazelisk",
            "/usr/bin/bazel",                    // System install
            "/usr/bin/bazelisk",
        ];

        for path in common_paths {
            if std::path::Path::new(path).exists() {
                return Ok(path.to_string());
            }
        }

        Err(rush_core::error::Error::Build(
            "Could not find bazel or bazelisk binary. Please install bazel and ensure it's in your PATH".to_string()
        ))
    }

    /// Ensures the Bazel workspace has a .bazelrc with correct toolchain configuration
    ///
    /// On macOS, if Homebrew LLVM is installed, Bazel may pick up clang-20 which
    /// doesn't know how to find the system linker (ld). This function creates a
    /// .bazelrc file that forces use of Apple's toolchain and ensures the correct
    /// PATH for docker-credential-desktop.
    async fn ensure_bazelrc(workspace_path: &Path) -> Result<()> {
        let bazelrc_path = workspace_path.join(".bazelrc");
        
        // Check if .bazelrc already exists
        if bazelrc_path.exists() {
            debug!(".bazelrc already exists at {}", bazelrc_path.display());
            return Ok(());
        }

        // Only create .bazelrc on macOS where the Homebrew LLVM issue occurs
        #[cfg(target_os = "macos")]
        {
            let bazelrc_content = r#"# Auto-generated by Rush to fix macOS toolchain issues
# This fixes "ld not found" errors when Homebrew LLVM is installed

# Force use of Apple's clang instead of Homebrew's clang
build --action_env=CC=/usr/bin/clang
build --action_env=CXX=/usr/bin/clang++

# Ensure the linker and docker-credential-desktop can be found
build --action_env=PATH=/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin

# Use the Xcode toolchain
build --apple_crosstool_top=@local_config_apple_cc//:toolchain
build --crosstool_top=@local_config_apple_cc//:toolchain
build --host_crosstool_top=@local_config_apple_cc//:toolchain
"#;

            info!("Creating .bazelrc at {} for macOS toolchain configuration", bazelrc_path.display());
            tokio::fs::write(&bazelrc_path, bazelrc_content)
                .await
                .map_err(|e| rush_core::error::Error::Build(format!(
                    "Failed to write .bazelrc: {}", e
                )))?;
        }

        Ok(())
    }

    /// Execute Bazel build command in the workspace
    async fn run_bazel_build(
        &self,
        workspace_path: &Path,
        targets: Option<&Vec<String>>,
        additional_args: Option<&Vec<String>>,
    ) -> Result<()> {
        use rush_utils::{CommandConfig, CommandRunner};

        let bazel_binary = Self::find_bazel_binary()?;
        
        let mut args = vec!["build".to_string()];

        // Add targets or default to all
        if let Some(t) = targets {
            args.extend(t.clone());
        } else {
            args.push("//...".to_string());
        }

        // Add compilation mode
        args.push("--compilation_mode=opt".to_string());

        // Add additional arguments
        if let Some(extra) = additional_args {
            args.extend(extra.clone());
        }

        info!(
            "Running Bazel build in {} with args: {:?}",
            workspace_path.display(),
            args
        );

        let config = CommandConfig::new(&bazel_binary)
            .args(args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .working_dir(workspace_path.to_str().unwrap())
            .capture(true);

        let output = CommandRunner::run(config)
            .await
            .map_err(|e| rush_core::error::Error::Build(format!("Bazel execution failed: {}", e)))?;

        if !output.success() {
            return Err(rush_core::error::Error::Build(format!(
                "Bazel build failed:\n{}",
                output.stderr
            )));
        }

        info!("Bazel build completed successfully");
        Ok(())
    }

    /// Parse repo_tags from a BUILD.bazel file for an oci_load target
    /// 
    /// This extracts the first tag from the repo_tags list to know what
    /// image name will be loaded into Docker.
    fn parse_oci_load_repo_tag(workspace_path: &Path, load_target: &str) -> Option<String> {
        // Determine the BUILD.bazel path from the load target
        // e.g., "//src:load" -> "src/BUILD.bazel"
        let target_path = load_target.trim_start_matches("//");
        let (package, _target_name) = target_path.split_once(':').unwrap_or((target_path, ""));
        
        let build_file = workspace_path.join(package).join("BUILD.bazel");
        let content = std::fs::read_to_string(&build_file).ok()?;
        
        // Look for repo_tags = ["name:tag"] pattern
        // This is a simple regex-free parser for the common case
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("repo_tags") {
                // Extract the first tag from repo_tags = ["tag"]
                if let Some(start) = trimmed.find('[') {
                    if let Some(end) = trimmed.find(']') {
                        let tags_str = &trimmed[start+1..end];
                        // Extract first quoted string
                        if let Some(q1) = tags_str.find('"') {
                            if let Some(q2) = tags_str[q1+1..].find('"') {
                                return Some(tags_str[q1+1..q1+1+q2].to_string());
                            }
                        }
                    }
                }
            }
        }
        
        None
    }

    /// Execute `bazel run` to build and load an OCI image into Docker
    ///
    /// This method is used when the Bazel workspace uses rules_oci to produce
    /// container images. The oci_load target builds the image and loads it
    /// directly into the local Docker daemon.
    /// 
    /// After loading, the image is re-tagged to the expected Rush image name.
    async fn run_bazel_oci_load(
        &self,
        workspace_path: &Path,
        oci_load_target: &str,
        expected_image_name: &str,
        additional_args: Option<&Vec<String>>,
    ) -> Result<()> {
        use rush_utils::{CommandConfig, CommandRunner};

        let bazel_binary = Self::find_bazel_binary()?;
        
        // Parse the repo_tags from BUILD.bazel to know what image bazel will load
        let bazel_image_tag = Self::parse_oci_load_repo_tag(workspace_path, oci_load_target);
        
        let mut args = vec!["run".to_string()];

        // Add the OCI load target
        args.push(oci_load_target.to_string());

        // Add compilation mode for optimized builds
        args.push("--compilation_mode=opt".to_string());

        // Add additional arguments
        if let Some(extra) = additional_args {
            args.extend(extra.clone());
        }

        info!(
            "Running Bazel OCI load in {} with target: {}",
            workspace_path.display(),
            oci_load_target
        );

        let config = CommandConfig::new(&bazel_binary)
            .args(args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .working_dir(workspace_path.to_str().unwrap())
            .capture(true);

        let output = CommandRunner::run(config)
            .await
            .map_err(|e| rush_core::error::Error::Build(format!("Bazel OCI load failed: {}", e)))?;

        if !output.success() {
            return Err(rush_core::error::Error::Build(format!(
                "Bazel OCI load failed:\n{}",
                output.stderr
            )));
        }

        info!("Bazel OCI image loaded successfully into Docker");

        // Re-tag the image to the expected Rush image name
        if let Some(bazel_tag) = bazel_image_tag {
            info!(
                "Re-tagging image from '{}' to '{}'",
                bazel_tag, expected_image_name
            );

            let tag_config = CommandConfig::new("docker")
                .args(["tag", &bazel_tag, expected_image_name])
                .capture(true);

            let tag_output = CommandRunner::run(tag_config)
                .await
                .map_err(|e| rush_core::error::Error::Build(format!("Failed to re-tag image: {}", e)))?;

            if !tag_output.success() {
                return Err(rush_core::error::Error::Build(format!(
                    "Failed to re-tag image from '{}' to '{}': {}",
                    bazel_tag, expected_image_name, tag_output.stderr
                )));
            }

            info!("Successfully re-tagged image to '{}'", expected_image_name);
        } else {
            warn!(
                "Could not parse repo_tags from BUILD.bazel for target '{}', image may have unexpected name",
                oci_load_target
            );
        }

        Ok(())
    }

    /// Generate a Dockerfile for Bazel build outputs
    async fn generate_bazel_dockerfile(
        &self,
        output_path: &Path,
        workspace_path: &Path,
        base_image: Option<&str>,
    ) -> Result<PathBuf> {
        let base = base_image.unwrap_or("python:3.11-slim");

        // For Python projects, we copy the source files directly rather than bazel-bin
        // because bazel-bin contains symlinks and read-only files that are problematic
        let src_dir = workspace_path.join("src");
        if src_dir.exists() {
            let target_src = output_path.join("src");
            tokio::fs::create_dir_all(&target_src)
                .await
                .map_err(rush_core::error::Error::Io)?;

            // Copy source files (not symlinks, just actual files)
            let mut entries = tokio::fs::read_dir(&src_dir)
                .await
                .map_err(rush_core::error::Error::Io)?;

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(rush_core::error::Error::Io)?
            {
                let path = entry.path();
                let file_name = entry.file_name();
                
                // Skip BUILD files and non-Python files for the Docker image
                let name_str = file_name.to_string_lossy();
                if name_str == "BUILD" || name_str == "BUILD.bazel" {
                    continue;
                }
                
                // Only copy regular files (not symlinks or directories for now)
                let metadata = tokio::fs::metadata(&path)
                    .await
                    .map_err(rush_core::error::Error::Io)?;
                    
                if metadata.is_file() {
                    let dst_path = target_src.join(&file_name);
                    tokio::fs::copy(&path, &dst_path)
                        .await
                        .map_err(rush_core::error::Error::Io)?;
                }
            }
        }

        let dockerfile_content = format!(
            r#"FROM {base}

WORKDIR /app
COPY src/ /app/

# Run the Python application
CMD ["python", "hello.py"]
"#
        );

        let dockerfile_path = output_path.join("Dockerfile.generated");
        tokio::fs::write(&dockerfile_path, dockerfile_content)
            .await
            .map_err(|e| {
                rush_core::error::Error::Build(format!("Failed to write Dockerfile: {}", e))
            })?;

        info!(
            "Generated Dockerfile at {} with base image {}",
            dockerfile_path.display(),
            base
        );

        Ok(dockerfile_path)
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
        info!(
            "Invalidated cache entries for {} changed files",
            changed_files.len()
        );
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

        // Create toolchain context - use native host platform
        let host_platform = Platform::default();
        let target_platform = Platform::for_docker(); // Docker containers always run Linux
        
        // Check if cross-compilation is needed (host OS != target OS)
        let needs_cross_compile = host_platform.os != target_platform.os;
        
        // Try to create toolchain context, falling back to native if cross-compilation not available
        let (toolchain, cross_compile_available) = match ToolchainContext::try_create_with_platforms(
            host_platform.clone(),
            target_platform.clone(),
        ) {
            Ok(tc) => {
                tc.setup_env();
                (tc, true)
            }
            Err(e) => {
                // Cross-compilation not available - check if we can use native toolchain
                // This is OK for build types that use Docker multi-stage builds
                debug!("Cross-compilation toolchain not available: {}. Using native toolchain.", e);
                (ToolchainContext::default(), false)
            }
        };

        // Get location from build type
        let location = spec.build_type.location().unwrap_or(".");
        
        // Determine if we should skip host cargo build
        // Skip when:
        // 1. Cross-compilation is needed but not available
        // 2. AND we have a multi-stage Dockerfile (detected by "multistage" in filename)
        let dockerfile_path = spec.build_type.dockerfile_path();
        let is_multistage_dockerfile = dockerfile_path
            .map(|d| d.to_lowercase().contains("multistage"))
            .unwrap_or(false);
        let skip_host_build = needs_cross_compile && !cross_compile_available && is_multistage_dockerfile;
        
        if skip_host_build {
            info!("Skipping host cargo build for {} - using multi-stage Dockerfile instead", spec.component_name);
        }

        // Create build context
        let context = BuildContext {
            build_type: spec.build_type.clone(),
            location: Some(location.to_string()),
            target: target_platform.clone(),
            host: host_platform.clone(),
            rust_target: target_platform.to_rust_target(),
            toolchain,
            services: Default::default(),
            environment: "local".to_string(),
            domain: spec.domain.clone(),
            product_name: spec.product_name.clone(),
            product_uri: format!("{}.local", spec.product_name),
            component: spec.component_name.clone(),
            docker_registry: String::new(),
            image_name: rush_core::naming::NamingConvention::image_name(
                &spec.product_name,
                &spec.component_name,
            ),
            domains: Default::default(),
            env: {
                // Merge dotenv and dotenv_secrets for build context
                let mut env = spec.dotenv.clone();
                env.extend(spec.dotenv_secrets.clone());
                env
            },
            secrets: Default::default(),
            cross_compile: spec.cross_compile.clone(),
            skip_host_build,
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
                vec![
                    "/bin/sh".to_string(),
                    script_path.to_string_lossy().to_string(),
                ],
                sink,
                Some(self.config.product_dir.clone()),
            )
            .await?;
        } else {
            // Fallback to direct execution without output capture
            let output = tokio::process::Command::new("/bin/sh")
                .arg(&script_path)
                .current_dir(&self.config.product_dir)
                .output()
                .await
                .map_err(|e| {
                    rush_core::error::Error::Build(format!("Failed to run build script: {e}"))
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(rush_core::error::Error::Build(format!(
                    "Build script failed for {}: {}",
                    spec.component_name, stderr
                )));
            }
        }

        info!("Build script completed for {}", spec.component_name);
        Ok(())
    }

    /// Renders artifacts for a component before Docker build
    async fn render_artifacts_for_component(
        &self,
        spec: &ComponentBuildSpec,
        all_specs: &[ComponentBuildSpec],
        docker_context: &Path,
    ) -> Result<()> {
        use std::collections::HashMap;

        use rush_build::{Artefact, BuildContext};
        use rush_toolchain::{Platform, ToolchainContext};

        // Check if this component has artifacts to render
        if spec.artefacts.is_none() {
            debug!("No artifacts to render for {}", spec.component_name);
            return Ok(());
        }

        let artifact_count = spec.artefacts.as_ref().map(|a| a.len()).unwrap_or(0);
        info!(
            "Rendering {} artifacts for {}",
            artifact_count, spec.component_name
        );

        // Create build context for rendering - use native platform
        let host_platform = Platform::default();
        let target_platform = Platform::for_docker(); // Docker containers always run Linux
        let toolchain = ToolchainContext::default();

        let location = spec.build_type.location().unwrap_or(".");

        // For Ingress, we need special handling for services
        let services = if let rush_build::BuildType::Ingress { components, .. } = &spec.build_type {
            // Build a services map using actual ports from component specs
            let mut services_map = HashMap::new();
            for component_name in components {
                // Find the actual component spec to get its resolved ports
                let component_spec = all_specs
                    .iter()
                    .find(|s| &s.component_name == component_name);

                if let Some(component_spec) = component_spec {
                    let service_spec = rush_build::ServiceSpec {
                        name: component_name.clone(),
                        host: rush_core::naming::NamingConvention::container_name(
                            &spec.product_name,
                            component_name,
                        ),
                        port: component_spec.port.unwrap_or(8080),
                        target_port: component_spec.target_port.unwrap_or(80),
                        mount_point: component_spec.mount_point.clone(),
                        domain: component_spec.domain.clone(),
                        docker_host: rush_core::naming::NamingConvention::container_name(
                            &spec.product_name,
                            component_name,
                        ),
                    };
                    services_map
                        .entry(component_spec.domain.clone())
                        .or_insert_with(Vec::new)
                        .push(service_spec);
                } else {
                    warn!("Component {component_name} referenced by ingress not found in specs");
                }
            }
            services_map
        } else {
            HashMap::new()
        };

        let context = BuildContext {
            build_type: spec.build_type.clone(),
            location: Some(location.to_string()),
            target: target_platform.clone(),
            host: host_platform,
            rust_target: target_platform.to_rust_target(),
            toolchain,
            services,
            environment: "local".to_string(),
            domain: spec.domain.clone(),
            product_name: spec.product_name.clone(),
            product_uri: format!("{}.local", spec.product_name),
            component: spec.component_name.clone(),
            docker_registry: String::new(),
            image_name: rush_core::naming::NamingConvention::image_name(
                &spec.product_name,
                &spec.component_name,
            ),
            domains: Default::default(),
            env: {
                // Merge dotenv and dotenv_secrets for build context
                let mut env = spec.dotenv.clone();
                env.extend(spec.dotenv_secrets.clone());
                env
            },
            secrets: Default::default(),
            cross_compile: spec.cross_compile.clone(),
            skip_host_build: false, // Artifact rendering doesn't need this
        };

        // Render each artifact from the spec
        if let Some(artefacts_map) = &spec.artefacts {
            for (input_path, output_path) in artefacts_map {
                // Create the artifact
                let full_input_path = self.config.product_dir.join(input_path);
                let artefact = Artefact::new(
                    full_input_path.to_string_lossy().to_string(),
                    output_path.clone(),
                )?;

                // Render the artifact
                match artefact.render(&context) {
                    Ok(content) => {
                        // 1. Write to .rush/artifacts for tracking and cache
                        let rush_output_path = self
                            .config
                            .product_dir
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
                        info!(
                            "Rendered artifact to component dist: {}",
                            dist_output_path.display()
                        );

                        // Verify the file was written
                        if dist_output_path.exists() {
                            debug!(
                                "Verified artifact exists at: {}",
                                dist_output_path.display()
                            );
                        } else {
                            error!(
                                "WARNING: Artifact was NOT written to: {}",
                                dist_output_path.display()
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Failed to render artifact {input_path}: {e}");
                    }
                }
            }
        }

        info!("Artifacts rendered for {}", spec.component_name);
        Ok(())
    }
}
