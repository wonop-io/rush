use colored::Colorize;
use log::{debug, error, info, trace};
use rush_cli::cli::{self, parse_args, DescribeCommand};
use rush_cli::core::config::{apply_rushd_config, ConfigLoader, RushdConfig};
use rush_cli::core::environment::setup_environment;
use rush_cli::error::{Error, Result};
use rush_cli::k8s;
use rush_cli::security::{FileVault, SecretsProvider, VaultAdapter};
use rush_cli::toolchain::ToolchainContext;
use rush_cli::utils::{find_project_root, Directory};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let matches = parse_args();

    // Initialize logging
    let log_level = matches
        .get_one::<String>("log_level")
        .unwrap_or(&"info".to_string())
        .to_string();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    // Get product name from arguments
    let product_name = matches
        .get_one::<String>("product_name")
        .ok_or_else(|| Error::InvalidInput("Product name is required".to_string()))?;

    // Set up environment and find project root
    setup_environment();

    let current_dir = std::env::current_dir()
        .map_err(|e| Error::Filesystem(format!("Failed to get current directory: {}", e)))?;

    let root_path = find_project_root(current_dir)
        .ok_or_else(|| Error::Filesystem("Failed to find project root".to_string()))?;

    debug!("Found project root at: {}", root_path.display());
    let _root_guard = Directory::chpath(&root_path);

    // Load rushd.yaml configuration if it exists
    let rushd_config_path = root_path.join("rushd.yaml");
    if rushd_config_path.exists() {
        debug!("Loading rushd.yaml from {}", rushd_config_path.display());
        let mut file = File::open(&rushd_config_path)
            .map_err(|e| Error::Filesystem(format!("Failed to open rushd.yaml: {}", e)))?;

        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|e| Error::Filesystem(format!("Failed to read rushd.yaml: {}", e)))?;

        let rushd_config: RushdConfig = serde_yaml::from_str(&contents)
            .map_err(|e| Error::Config(format!("Failed to parse rushd.yaml: {}", e)))?;

        apply_rushd_config(&rushd_config);
        info!("Applied configuration from rushd.yaml");
    } else {
        debug!("No rushd.yaml found at {}", rushd_config_path.display());
    }

    info!("Starting rush for product: {}", product_name);

    // Load configuration
    let config_loader = ConfigLoader::new(root_path);
    let environment = matches
        .get_one::<String>("environment")
        .map(|s| s.as_str())
        .unwrap_or("local");
    let docker_registry = matches
        .get_one::<String>("docker_registry")
        .map(|s| s.as_str())
        .unwrap_or("docker.io");
    let start_port = matches
        .get_one::<u16>("start_port")
        .copied()
        .unwrap_or(8080);

    let config =
        config_loader.load_config(product_name, environment, docker_registry, start_port)?;

    trace!("Configuration loaded: {:?}", config);

    // Set up toolchain
    let toolchain = Arc::new(ToolchainContext::default());

    // Set up vault
    let vault_path = PathBuf::from("/tmp/vault");
    let _vault = Arc::new(Mutex::new(FileVault::new(vault_path, None)));

    // Handle subcommands
    if let Some(("build", sub_matches)) = matches.subcommand() {
        info!("Executing build command");
        cli::execute_build(config.clone(), sub_matches).await?;
    } else if let Some(("dev", sub_m)) = matches.subcommand() {
        info!("Executing dev command");
        let redirect_components = cli::parse_redirected_components(&matches);
        let silence_components = cli::parse_silenced_components(&matches);

        let secrets_provider: Arc<dyn SecretsProvider> = Arc::new(VaultAdapter::new(
            FileVault::new(PathBuf::from("/tmp/vault"), None),
        ));

        cli::execute_dev(
            product_name.clone(),
            config.clone(),
            toolchain.clone(),
            secrets_provider,
            redirect_components,
            silence_components,
        )
        .await?;
    } else if let Some(("deploy", _)) = matches.subcommand() {
        info!("Executing deploy command");
        let context_manager = Arc::new(Mutex::new(k8s::create_context_manager("kubectl").unwrap()));
        let services = Vec::new(); // Initialize with empty service list as placeholder
        cli::execute_deploy(config.clone(), context_manager, &services).await?;
    } else if let Some(("describe", describe_matches)) = matches.subcommand() {
        // Handle describe subcommands
        let services = Vec::new(); // Empty service list as placeholder
        let secrets_provider: Arc<dyn SecretsProvider> = Arc::new(VaultAdapter::new(
            FileVault::new(PathBuf::from("/tmp/vault"), None),
        ));

        if let Some(("toolchain", _)) = describe_matches.subcommand() {
            cli::execute_describe(
                DescribeCommand::Toolchain,
                &config,
                &services,
                &toolchain,
                &secrets_provider,
            )
            .await?;
        } else if let Some(("images", _)) = describe_matches.subcommand() {
            cli::execute_describe(
                DescribeCommand::Images,
                &config,
                &services,
                &toolchain,
                &secrets_provider,
            )
            .await?;
        } else {
            error!("Unknown describe subcommand");
            process::exit(1);
        }
    } else {
        println!("{} {}", "Error:".red().bold(), "No subcommand specified");
        process::exit(1);
    }

    info!("rush completed successfully");
    Ok(())
}
