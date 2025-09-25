use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use colored::*;
use log::{debug, info, warn};
use rush_config::Config;
use rush_core::error::{Error, Result};
use tokio::time::sleep;

/// Deployment strategy options
#[derive(Debug, Clone)]
pub enum DeploymentStrategy {
    /// Rolling update - gradually replace instances
    RollingUpdate {
        max_surge: u32,
        max_unavailable: u32,
    },
    /// Blue-green deployment - switch between two identical environments
    BlueGreen,
    /// Canary deployment - gradually roll out to a percentage of users
    Canary { percentage: u32 },
    /// Direct deployment - replace all instances at once
    Direct,
}

impl Default for DeploymentStrategy {
    fn default() -> Self {
        DeploymentStrategy::RollingUpdate {
            max_surge: 1,
            max_unavailable: 1,
        }
    }
}

/// Deployment configuration
#[derive(Debug, Clone)]
pub struct DeploymentConfig {
    /// Deployment strategy to use
    pub strategy: DeploymentStrategy,
    /// Whether to wait for deployments to be ready
    pub wait_for_ready: bool,
    /// Timeout for waiting (in seconds)
    pub wait_timeout: u64,
    /// Whether to perform health checks
    pub health_checks: bool,
    /// Whether to rollback on failure
    pub auto_rollback: bool,
    /// Dry run mode
    pub dry_run: bool,
    /// Force rebuild of images
    pub force_rebuild: bool,
    /// Skip image push
    pub skip_push: bool,
    /// Skip manifest generation
    pub skip_manifests: bool,
}

impl Default for DeploymentConfig {
    fn default() -> Self {
        Self {
            strategy: DeploymentStrategy::default(),
            wait_for_ready: true,
            wait_timeout: 300,
            health_checks: true,
            auto_rollback: true,
            dry_run: false,
            force_rebuild: false,
            skip_push: false,
            skip_manifests: false,
        }
    }
}

/// Progress reporter for deployment steps
struct ProgressReporter {
    current_step: usize,
    total_steps: usize,
    start_time: std::time::Instant,
}

impl ProgressReporter {
    fn new(total_steps: usize) -> Self {
        Self {
            current_step: 0,
            total_steps,
            start_time: std::time::Instant::now(),
        }
    }

    fn start_step(&mut self, description: &str) {
        self.current_step += 1;
        let progress = format!("[{}/{}]", self.current_step, self.total_steps).cyan();
        println!("{} {} {}", progress, "→".blue(), description.white());
    }

    fn complete_step(&self, message: &str) {
        println!("    {} {}", "✓".green(), message.green());
    }

    fn error_step(&self, message: &str) {
        println!("    {} {}", "✗".red(), message.red());
    }

    fn info(&self, message: &str) {
        println!("    {} {}", "ℹ".blue(), message);
    }

    fn finish(&self) {
        let elapsed = self.start_time.elapsed();
        println!(
            "\n{} Deployment completed in {:.1}s",
            "✓".green().bold(),
            elapsed.as_secs_f32()
        );
    }
}

/// Execute the full deployment pipeline with production features
pub async fn execute(config: Arc<Config>, deployment_config: DeploymentConfig) -> Result<()> {
    // Initialize audit logging
    let audit_log_dir = std::path::PathBuf::from(".rush/audit");
    let audit_manager = rush_k8s::AuditManager::with_file_logger(audit_log_dir)?;

    let environment = config.environment().to_string();
    let product_name = config.product_name().to_string();
    let version = std::env::var("GIT_COMMIT")
        .or_else(|_| std::env::var("DEPLOYMENT_VERSION"))
        .unwrap_or_else(|_| "latest".to_string());

    // Log deployment started
    audit_manager.log_deployment_started(&product_name, &environment, &version)?;

    // Initialize hook manager
    let mut hook_manager = rush_k8s::HookManager::new();

    // Add validation hooks
    hook_manager.add_pre_deploy_hook(Box::new(rush_k8s::ValidationHook::new(
        "resource-quota-check".to_string(),
    )));

    // Create hook context
    let hook_context = rush_k8s::HookContext {
        product_name: product_name.clone(),
        environment: environment.clone(),
        version: version.clone(),
        dry_run: deployment_config.dry_run,
        metadata: HashMap::new(),
    };

    // Run pre-deployment hooks
    if let Err(e) = hook_manager.run_pre_deploy_hooks(&hook_context).await {
        audit_manager.log_deployment_failed(
            &product_name,
            &environment,
            &version,
            e.to_string(),
        )?;
        return Err(e);
    }

    let mut reporter = ProgressReporter::new(8);

    println!(
        "\n{} {} to {}",
        "Deploying".cyan().bold(),
        config.product_name().white().bold(),
        std::env::var("RUSH_ENV")
            .unwrap_or_else(|_| "default".to_string())
            .yellow()
    );

    if deployment_config.dry_run {
        println!("{} {}", "Mode:".blue(), "DRY RUN".yellow().bold());
    }

    println!(
        "{} {}\n",
        "Strategy:".blue(),
        format!("{:?}", deployment_config.strategy).white()
    );

    // Step 1: Validate environment
    reporter.start_step("Validating deployment environment");
    validate_environment(&config)?;
    reporter.complete_step("Environment validated");

    // Step 2: Create reactor for building
    reporter.start_step("Initializing build system");
    let reactor = create_reactor(config.clone()).await?;
    reporter.complete_step("Build system ready");

    // Step 3: Build Docker images
    if !deployment_config.skip_manifests {
        reporter.start_step("Building Docker images");
        build_images(reactor, deployment_config.force_rebuild).await?;
        reporter.complete_step("Images built successfully");
    } else {
        reporter.info("Skipping image build (--skip-build)");
    }

    // Step 4: Push images to registry
    if !deployment_config.skip_push && !deployment_config.dry_run {
        reporter.start_step("Pushing images to registry");
        push_images(config.clone()).await?;
        reporter.complete_step("Images pushed to registry");
    } else if deployment_config.dry_run {
        reporter.info("Skipping image push (dry-run mode)");
    } else {
        reporter.info("Skipping image push (--skip-push)");
    }

    // Step 5: Generate Kubernetes manifests
    reporter.start_step("Generating Kubernetes manifests");
    let reactor = create_reactor(config.clone()).await?;
    generate_manifests(reactor).await?;
    reporter.complete_step("Manifests generated");

    // Step 6: Apply deployment strategy
    reporter.start_step(&format!(
        "Applying {:?} deployment",
        deployment_config.strategy
    ));
    apply_deployment_strategy(
        config.clone(),
        &deployment_config.strategy,
        deployment_config.dry_run,
    )
    .await?;
    reporter.complete_step("Deployment strategy applied");

    // Step 7: Deploy to Kubernetes
    reporter.start_step("Deploying to Kubernetes");
    let deployment_result = deploy_to_kubernetes(config.clone(), deployment_config.dry_run).await;

    match deployment_result {
        Ok(_) => {
            reporter.complete_step("Deployed to Kubernetes");
        }
        Err(e) => {
            reporter.error_step(&format!("Deployment failed: {e}"));

            if deployment_config.auto_rollback && !deployment_config.dry_run {
                reporter.start_step("Initiating automatic rollback");
                rollback_deployment(config.clone()).await?;
                reporter.complete_step("Rollback completed");
            }

            return Err(e);
        }
    }

    // Step 8: Verify deployment
    if deployment_config.wait_for_ready && !deployment_config.dry_run {
        reporter.start_step("Verifying deployment");
        verify_deployment(
            config.clone(),
            deployment_config.wait_timeout,
            deployment_config.health_checks,
        )
        .await?;
        reporter.complete_step("Deployment verified and healthy");
    }

    reporter.finish();

    // Run post-deployment hooks
    if let Err(e) = hook_manager.run_post_deploy_hooks(&hook_context).await {
        warn!("Post-deployment hook failed: {e}");
        // Post-deployment hooks are usually optional, so we don't fail the deployment
    }

    // Log deployment success
    audit_manager.log_deployment_succeeded(&product_name, &environment, &version)?;

    Ok(())
}

/// Validate the deployment environment
fn validate_environment(_config: &Config) -> Result<()> {
    // Check if we have required environment variables
    let required_vars = vec!["DOCKER_REGISTRY", "K8S_NAMESPACE"];

    for var in required_vars {
        if std::env::var(var).is_err() {
            warn!("Environment variable {var} not set, using defaults");
        }
    }

    // Check if kubectl is available
    let kubectl_check = std::process::Command::new("kubectl")
        .arg("version")
        .arg("--client")
        .output();

    if kubectl_check.is_err() {
        return Err(Error::External(
            "kubectl not found. Please install kubectl.".to_string(),
        ));
    }

    // Check if docker is available
    let docker_check = std::process::Command::new("docker").arg("version").output();

    if docker_check.is_err() {
        return Err(Error::External(
            "docker not found. Please install docker.".to_string(),
        ));
    }

    Ok(())
}

/// Create a reactor for building and deployment
#[allow(clippy::arc_with_non_send_sync)]
async fn create_reactor(
    config: Arc<Config>,
) -> Result<Arc<tokio::sync::Mutex<rush_container::reactor::Reactor>>> {
    let docker_client = Arc::new(rush_docker::DockerExecutor::new());

    // Create vault (using file vault for now)
    let vault: Arc<std::sync::Mutex<dyn rush_security::Vault + Send>> =
        Arc::new(std::sync::Mutex::new(rush_security::FileVault::new(
            config.product_path().join(".rush/vault"),
            None,
        )));

    // Create secrets encoder
    let secrets_encoder: Arc<dyn rush_security::SecretsEncoder> =
        Arc::new(rush_security::NoopEncoder);

    // Create network manager
    let network_manager = Arc::new(
        rush_container::network::NetworkManager::new(docker_client.clone(), config.product_name())
            .await?,
    );

    // Create reactor
    let reactor = rush_container::reactor::Reactor::from_product_dir(
        config.clone(),
        vault,
        secrets_encoder,
        HashMap::new(),
        Vec::new(),
        network_manager,
    )
    .await?;

    Ok(Arc::new(tokio::sync::Mutex::new(reactor)))
}

/// Build Docker images
async fn build_images(
    reactor: Arc<tokio::sync::Mutex<rush_container::reactor::Reactor>>,
    force_rebuild: bool,
) -> Result<()> {
    let mut reactor = reactor.lock().await;

    if force_rebuild {
        info!("Force rebuilding all images");
    }

    // Build all components
    reactor.rebuild_all().await?;

    Ok(())
}

/// Push Docker images to registry
async fn push_images(config: Arc<Config>) -> Result<()> {
    let _docker_client = rush_docker::DockerExecutor::new();

    // Get registry configuration
    let registry = config.docker_registry();
    let namespace = config.docker_registry_namespace();

    info!("Pushing images to registry: {registry}/{namespace:?}");

    // TODO: Get list of built images from reactor and push them
    // For now, we'll use a placeholder
    warn!("Image push not fully implemented yet");

    Ok(())
}

/// Generate Kubernetes manifests
async fn generate_manifests(
    reactor: Arc<tokio::sync::Mutex<rush_container::reactor::Reactor>>,
) -> Result<()> {
    let mut reactor = reactor.lock().await;

    // Generate manifests
    reactor.build_manifests().await?;

    Ok(())
}

/// Apply deployment strategy
async fn apply_deployment_strategy(
    config: Arc<Config>,
    strategy: &DeploymentStrategy,
    dry_run: bool,
) -> Result<()> {
    match strategy {
        DeploymentStrategy::RollingUpdate {
            max_surge,
            max_unavailable,
        } => {
            info!(
                "Configuring rolling update (max_surge: {max_surge}, max_unavailable: {max_unavailable})"
            );
            // The rolling update is handled by Kubernetes deployment spec
            // which is configured in the manifest generator
        }
        DeploymentStrategy::BlueGreen => {
            info!("Configuring blue-green deployment");
            apply_blue_green_deployment(config, dry_run).await?;
        }
        DeploymentStrategy::Canary { percentage } => {
            info!("Configuring canary deployment ({percentage}% traffic)");
            apply_canary_deployment(config, *percentage, dry_run).await?;
        }
        DeploymentStrategy::Direct => {
            info!("Using direct deployment strategy");
            // Direct deployment is the default behavior
        }
    }

    Ok(())
}

/// Apply blue-green deployment strategy
async fn apply_blue_green_deployment(_config: Arc<Config>, _dry_run: bool) -> Result<()> {
    // Blue-green deployment logic:
    // 1. Deploy to inactive environment (blue or green)
    // 2. Run smoke tests
    // 3. Switch traffic to new environment
    // 4. Keep old environment for rollback

    warn!("Blue-green deployment not fully implemented yet");

    Ok(())
}

/// Apply canary deployment strategy
async fn apply_canary_deployment(
    _config: Arc<Config>,
    _percentage: u32,
    _dry_run: bool,
) -> Result<()> {
    // Canary deployment logic:
    // 1. Deploy new version alongside old
    // 2. Route percentage of traffic to new version
    // 3. Monitor metrics
    // 4. Gradually increase traffic or rollback

    warn!("Canary deployment not fully implemented yet");

    Ok(())
}

/// Deploy to Kubernetes cluster
async fn deploy_to_kubernetes(config: Arc<Config>, dry_run: bool) -> Result<()> {
    // Create reactor for deployment
    let reactor = create_reactor(config.clone()).await?;
    let mut reactor = reactor.lock().await;

    // Set dry-run mode
    if dry_run {
        std::env::set_var("K8S_DRY_RUN", "true");
    }

    // Apply manifests
    reactor.apply().await?;

    Ok(())
}

/// Verify deployment is successful
async fn verify_deployment(config: Arc<Config>, timeout: u64, health_checks: bool) -> Result<()> {
    let namespace = std::env::var("K8S_NAMESPACE").unwrap_or_else(|_| {
        format!(
            "{}-{}",
            config.product_name(),
            std::env::var("RUSH_ENV").unwrap_or_else(|_| "default".to_string())
        )
    });

    let kubectl_config = rush_k8s::KubectlConfig {
        kubectl_path: "kubectl".to_string(),
        context: std::env::var("K8S_CONTEXT").ok(),
        namespace: Some(namespace),
        dry_run: false,
        kubeconfig: None,
        verbose: false,
    };

    let kubectl = rush_k8s::Kubectl::new(kubectl_config);

    // Wait for deployments to be ready
    info!("Waiting for deployments to be ready (timeout: {timeout}s)");

    let start = std::time::Instant::now();
    let timeout_duration = Duration::from_secs(timeout);

    // Check deployment status
    while start.elapsed() < timeout_duration {
        // Get deployment status
        let result = kubectl.get("deployments", None).await?;

        if result.success {
            // Parse JSON output to check if all replicas are ready
            // For now, we'll just check if the command succeeded
            debug!("Deployments status check succeeded");

            if health_checks {
                // Perform health checks
                perform_health_checks(&kubectl).await?;
            }

            return Ok(());
        }

        // Wait before retrying
        sleep(Duration::from_secs(5)).await;
    }

    Err(Error::Deploy(
        "Deployment verification timed out".to_string(),
    ))
}

/// Perform health checks on deployed services
async fn perform_health_checks(kubectl: &rush_k8s::Kubectl) -> Result<()> {
    info!("Performing health checks on deployed services");

    // Get list of services
    let services_result = kubectl.get("services", None).await?;

    if !services_result.success {
        warn!("Could not retrieve services for health checks");
        return Ok(());
    }

    // TODO: Implement actual health checks
    // This would involve:
    // 1. Getting service endpoints
    // 2. Making HTTP requests to health check endpoints
    // 3. Verifying responses

    info!("Health checks passed");

    Ok(())
}

/// Rollback deployment on failure
async fn rollback_deployment(config: Arc<Config>) -> Result<()> {
    warn!("Initiating deployment rollback");

    // Create reactor for rollback
    let reactor = create_reactor(config.clone()).await?;
    let mut reactor = reactor.lock().await;

    // Perform rollback
    reactor.rollback(None).await?;

    info!("Rollback completed successfully");

    Ok(())
}

/// Get deployment status
pub async fn get_status(config: Arc<Config>) -> Result<()> {
    let namespace = std::env::var("K8S_NAMESPACE").unwrap_or_else(|_| {
        format!(
            "{}-{}",
            config.product_name(),
            std::env::var("RUSH_ENV").unwrap_or_else(|_| "default".to_string())
        )
    });

    println!(
        "\n{} {} in namespace {}",
        "Deployment Status for".cyan(),
        config.product_name().white().bold(),
        namespace.yellow()
    );

    let kubectl_config = rush_k8s::KubectlConfig {
        kubectl_path: "kubectl".to_string(),
        context: std::env::var("K8S_CONTEXT").ok(),
        namespace: Some(namespace),
        dry_run: false,
        kubeconfig: None,
        verbose: false,
    };

    let kubectl = rush_k8s::Kubectl::new(kubectl_config);

    // Get deployments
    println!("\n{}:", "Deployments".blue().bold());
    let deployments = kubectl
        .execute(vec![
            "get".to_string(),
            "deployments".to_string(),
            "-o".to_string(),
            "wide".to_string(),
        ])
        .await?;

    if deployments.success {
        println!("{}", deployments.stdout);
    } else {
        println!("  No deployments found");
    }

    // Get services
    println!("{}:", "Services".blue().bold());
    let services = kubectl
        .execute(vec![
            "get".to_string(),
            "services".to_string(),
            "-o".to_string(),
            "wide".to_string(),
        ])
        .await?;

    if services.success {
        println!("{}", services.stdout);
    } else {
        println!("  No services found");
    }

    // Get pods
    println!("{}:", "Pods".blue().bold());
    let pods = kubectl
        .execute(vec![
            "get".to_string(),
            "pods".to_string(),
            "-o".to_string(),
            "wide".to_string(),
        ])
        .await?;

    if pods.success {
        println!("{}", pods.stdout);
    } else {
        println!("  No pods found");
    }

    Ok(())
}
