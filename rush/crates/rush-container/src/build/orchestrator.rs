//! Build orchestration for container images
//!
//! This module coordinates the building of multiple components,
//! handling dependencies and parallel builds where possible.

use crate::{
    build::{BuildProcessor, BuildCache, CacheEntry, CacheStats},
    docker::DockerClient,
    events::{Event, EventBus, ContainerEvent},
    reactor::state::SharedReactorState,
};
use rush_build::{BuildType, ComponentBuildSpec};
use rush_core::error::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use log::{debug, error, info};
use tokio::sync::Mutex;

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
    cache: Arc<Mutex<BuildCache>>,
    build_processor: Arc<BuildProcessor>,
}

impl BuildOrchestrator {
    /// Create a new build orchestrator
    pub fn new(
        config: BuildOrchestratorConfig,
        docker_client: Arc<dyn DockerClient>,
        event_bus: EventBus,
        state: SharedReactorState,
    ) -> Self {
        let cache = Arc::new(Mutex::new(BuildCache::new(&config.cache_dir)));
        let build_processor = Arc::new(BuildProcessor::new(false));
        
        Self {
            config,
            docker_client,
            event_bus,
            state,
            cache,
            build_processor,
        }
    }

    /// Build all components
    pub async fn build_components(
        &self,
        component_specs: Vec<ComponentBuildSpec>,
        force_rebuild: bool,
    ) -> Result<HashMap<String, String>> {
        info!("Building {} components", component_specs.len());
        let start_time = Instant::now();
        
        // Update state
        {
            let mut state = self.state.write().await;
            state.transition_to(crate::reactor::state::ReactorPhase::Building)?;
        }
        
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
        for spec in component_specs {
            // Check cache if enabled
            if self.config.enable_cache && !force_rebuild {
                let cache_guard = self.cache.lock().await;
                if let Some(cached_image) = cache_guard.get(&spec.component_name).await {
                    if !cache_guard.is_expired(&spec.component_name).await {
                        info!("Using cached image for {}: {}", spec.component_name, cached_image);
                        built_images.insert(spec.component_name.clone(), cached_image.clone());
                        
                        // Update state
                        {
                            let mut state = self.state.write().await;
                            state.mark_component_built(&spec.component_name, cached_image);
                        }
                        
                        continue;
                    }
                }
            }
            
            // Build the component
            match self.build_single(spec.clone()).await {
                Ok(image_name) => {
                    info!("Successfully built {}: {}", spec.component_name, image_name);
                    built_images.insert(spec.component_name.clone(), image_name.clone());
                    
                    // Update cache
                    if self.config.enable_cache {
                        let mut cache_guard = self.cache.lock().await;
                        cache_guard.put(
                            spec.component_name.clone(),
                            CacheEntry::new(image_name.clone(), spec.clone()),
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
                    error!("Failed to build {}: {}", spec.component_name, e);
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
        
        info!("All components built successfully in {:?}", start_time.elapsed());
        Ok(built_images)
    }

    /// Build a single component
    pub async fn build_single(&self, spec: ComponentBuildSpec) -> Result<String> {
        debug!("Building component: {}", spec.component_name);
        let start_time = Instant::now();
        
        // Prepare build artifacts
        let artifacts_dir = self.prepare_artifacts(&spec).await?;
        
        // Determine image name and tag
        let image_name = format!(
            "{}/{}",
            self.config.product_name,
            spec.component_name
        );
        let tag = self.generate_tag(&spec);
        let full_image_name = format!("{}:{}", image_name, tag);
        
        // Build based on type
        match &spec.build_type {
            BuildType::RustBinary { .. } 
            | BuildType::TrunkWasm { .. }
            | BuildType::DixiousWasm { .. }
            | BuildType::Script { .. }
            | BuildType::Zola { .. }
            | BuildType::Book { .. } => {
                // Build using Dockerfile
                if let Some(dockerfile) = spec.build_type.dockerfile_path() {
                    let dockerfile_path = self.config.product_dir.join(dockerfile);
                    
                    // Build the image
                    self.docker_client.build_image(
                        &full_image_name,
                        &dockerfile_path.to_string_lossy(),
                        &artifacts_dir.to_string_lossy(),
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
            BuildType::Ingress { .. } => {
                // Ingress doesn't need a container image
                debug!("Skipping build for ingress {}", spec.component_name);
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
        
        // Create artifacts directory
        let artifacts_dir = self.config.product_dir
            .join(".rush")
            .join("artifacts")
            .join(&spec.component_name);
        
        tokio::fs::create_dir_all(&artifacts_dir).await
            .map_err(|e| rush_core::error::Error::Io(e))?;
        
        // Render templates based on build type
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
        // Get source directory from build type
        let source_dir = if let BuildType::RustBinary { context_dir, .. } = &spec.build_type {
            if let Some(ctx) = context_dir {
                self.config.product_dir.join(ctx)
            } else {
                self.config.product_dir.join(&spec.component_name)
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
        // Get source directory from build type
        let source_dir = if let BuildType::TrunkWasm { context_dir, .. } = &spec.build_type {
            if let Some(ctx) = context_dir {
                self.config.product_dir.join(ctx)
            } else {
                self.config.product_dir.join(&spec.component_name)
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

    /// Generate a tag for the image
    fn generate_tag(&self, _spec: &ComponentBuildSpec) -> String {
        // For now, use a timestamp-based tag
        // TODO: Use git commit hash or version from Cargo.toml
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        format!("{}", timestamp)
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
}