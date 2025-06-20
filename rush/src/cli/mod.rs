pub mod args;
pub mod commands;
pub mod context;

use crate::build::Config;
use crate::check_version;
use crate::cli::context::CliContext;
use crate::cluster::{K8Encoder, NoopEncoder, SealedSecretsEncoder};
use crate::container::Reactor;
use crate::core::environment::PublicEnvironmentDefinitions;
use crate::create_vault;
use crate::load_config;
use crate::setup_environment;
use crate::toolchain::Platform;
use crate::toolchain::ToolchainContext;
use crate::utils::Directory;
use crate::vault::Base64SecretsEncoder;
use crate::vault::SecretsDefinitions;
use crate::vault::Vault;
use clap::ArgMatches;
use log::warn;
use log::{debug, error, info, trace};
use std::collections::HashMap;
use std::env;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;

pub async fn init_application() -> Result<(), std::io::Error> {
    check_version().await;
    setup_environment();

    let root_dir = std::env::var("RUSHD_ROOT").unwrap();
    let _guard = Directory::chdir(&root_dir);
    debug!("Changed directory to RUSHD_ROOT: {}", root_dir);
    load_config();
    dotenv::dotenv().ok();

    Ok(())
}

// Execute the appropriate command based on command line arguments
pub async fn execute_command(
    matches: &ArgMatches,
    ctx: &mut CliContext,
) -> Result<(), std::io::Error> {
    // Validate secrets before executing commands
    if let Err(e) = ctx
        .secrets_context
        .validate_vault(ctx.vault.clone(), &ctx.environment)
        .await
    {
        error!("Missing secrets in vault: {}", e);
        eprintln!("{}", e);
        process::exit(1);
    }

    // Route to appropriate command handlers
    if let Some(validate_matches) = matches.subcommand_matches("validate") {
        commands::validate::execute(validate_matches, ctx).await
    } else if let Some(describe_matches) = matches.subcommand_matches("describe") {
        commands::describe::execute(describe_matches, ctx).await
    } else if let Some(vault_matches) = matches.subcommand_matches("vault") {
        commands::vault::execute(vault_matches, ctx).await
    } else if let Some(secrets_matches) = matches.subcommand_matches("secrets") {
        commands::secrets::execute(secrets_matches, ctx).await
    } else if matches.subcommand_matches("dev").is_some() {
        commands::dev::execute(ctx).await
    } else if matches.subcommand_matches("build").is_some() {
        commands::build::execute(ctx).await
    } else if matches.subcommand_matches("push").is_some() {
        commands::build::push(ctx).await
    } else if matches.subcommand_matches("rollout").is_some() {
        commands::rollout::execute(ctx).await
    } else if matches.subcommand_matches("install").is_some() {
        commands::install::execute(ctx).await
    } else if matches.subcommand_matches("uninstall").is_some() {
        commands::install::uninstall(ctx).await
    } else if matches.subcommand_matches("deploy").is_some() {
        commands::deploy::execute(ctx).await
    } else if matches.subcommand_matches("apply").is_some() {
        commands::apply::execute(ctx).await
    } else if matches.subcommand_matches("unapply").is_some() {
        commands::apply::unapply(ctx).await
    } else {
        Ok(())
    }
}

// Helper functions for CLI context creation - src/cli/context.rs
fn parse_redirected_components(matches: &ArgMatches) -> HashMap<String, (String, u16)> {
    matches
        .subcommand_matches("dev")
        .and_then(|dev_matches| dev_matches.get_many::<String>("redirect"))
        .map(|values| {
            values
                .cloned()
                .filter_map(|value| {
                    let parts: Vec<&str> = value.split('@').collect();
                    if parts.len() == 2 {
                        let component = parts[0].to_string();
                        let host_port: Vec<&str> = parts[1].split(':').collect();
                        if host_port.len() == 2 {
                            let mut host = host_port[0].to_string();
                            if host == "localhost" || host == "127.0.0.1" {
                                host = "host.docker.internal".to_string();
                            }
                            if let Ok(port) = host_port[1].parse::<u16>() {
                                return Some((component, (host, port)));
                            }
                        }
                    }
                    None
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_silence_components(matches: &ArgMatches) -> Vec<String> {
    matches
        .subcommand_matches("dev")
        .and_then(|dev_matches| dev_matches.get_many::<String>("silence"))
        .map(|values| values.cloned().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

fn setup_logging(matches: &ArgMatches) {
    if let Some(log_level) = matches.get_one::<String>("log_level") {
        env::set_var("RUST_LOG", log_level);
        env_logger::builder().parse_env("RUST_LOG").init();
        trace!("Log level set to: {}", log_level);
    } else {
        env_logger::init();
    }
    trace!("Starting Rush application");
}

fn get_target_arch(matches: &ArgMatches) -> String {
    if let Some(target_arch) = matches.get_one::<String>("target_arch") {
        let arch = target_arch.clone();
        info!("Target architecture: {}", arch);
        arch
    } else {
        let default_arch = "x86_64".to_string();
        info!("Target architecture: {}", default_arch);
        default_arch
    }
}

fn get_target_os(matches: &ArgMatches) -> String {
    if let Some(target_os) = matches.get_one::<String>("target_os") {
        let os = target_os.clone();
        info!("Target OS: {}", os);
        os
    } else {
        let default_os = "linux".to_string();
        info!("Target OS: {}", default_os);
        default_os
    }
}

fn get_environment(matches: &ArgMatches) -> String {
    if let Some(environment) = matches.get_one::<String>("environment") {
        let env = environment.clone();
        info!("Environment: {}", env);
        env
    } else {
        let default_env = "local".to_string();
        info!("Environment: {}", default_env);
        default_env
    }
}

fn get_docker_registry(matches: &ArgMatches) -> String {
    if let Some(docker_registry) = matches.get_one::<String>("docker_registry") {
        let registry = docker_registry.clone();
        info!("Docker registry: {}", registry);
        registry
    } else {
        let registry = std::env::var("DOCKER_REGISTRY")
            .expect("DOCKER_REGISTRY environment variable not found");
        info!("Docker registry: {}", registry);
        registry
    }
}

fn create_config(
    root_dir: &str,
    product_name: &str,
    environment: &str,
    docker_registry: &str,
    start_port: u16,
) -> Result<Arc<Config>, std::io::Error> {
    let _root_guard = Directory::chdir(root_dir);

    match Config::new(
        root_dir,
        product_name,
        environment,
        docker_registry,
        start_port,
    ) {
        Ok(config) => Ok(config),
        Err(e) => {
            error!("Failed to create config: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

fn setup_secrets(
    config: &Config,
    product_name: &str,
) -> Result<(SecretsDefinitions, Arc<Mutex<dyn Vault + Send>>), std::io::Error> {
    let secrets_context = SecretsDefinitions::new(
        product_name.to_string(),
        &format!("{}/stack.env.secrets.yaml", config.product_path()),
    );
    let product_path = std::path::PathBuf::from(config.product_path());

    let vault = create_vault(&product_path, config, config.vault_name());

    let secrets_encoder = Arc::new(Base64SecretsEncoder);
    let k8s_encoder = match config.k8s_encoder() {
        "kubeseal" => {
            info!("Encrypting K8s secrets with kubeseal");
            Arc::new(SealedSecretsEncoder) as Arc<dyn K8Encoder>
        }
        "noop" => {
            warn!("No secret encryption of secrets for K8s");
            Arc::new(NoopEncoder) as Arc<dyn K8Encoder>
        }
        _ => panic!("Invalid k8s encoder"),
    };

    Ok((secrets_context, vault))
}

fn setup_environment_files(
    config: &Config,
    product_name: &str,
    environment: &str,
) -> Result<(), std::io::Error> {
    let public_environment = PublicEnvironmentDefinitions::new(
        product_name.to_string(),
        &format!("{}/stack.env.base.yaml", config.product_path()),
        &format!("{}/stack.env.{}.yaml", config.product_path(), environment),
    );

    match public_environment.generate_dotenv_files() {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Unable to generate dotenv files: {}", e);
            eprintln!("{:#?}", e);
            std::process::exit(1);
        }
    }
}

fn create_toolchain(target_os: &str, target_arch: &str) -> Arc<ToolchainContext> {
    let toolchain = Arc::new(ToolchainContext::new(
        Platform::default(),
        Platform::new(target_os, target_arch),
    ));
    toolchain.setup_env();
    debug!("Toolchain set up");
    toolchain
}

fn create_reactor(
    config: Arc<Config>,
    toolchain: Arc<ToolchainContext>,
    vault: Arc<Mutex<dyn Vault + Send>>,
    redirected_components: HashMap<String, (String, u16)>,
    silence_components: Vec<String>,
) -> Result<Reactor, std::io::Error> {
    let secrets_encoder = Arc::new(Base64SecretsEncoder);
    let k8s_encoder = match config.k8s_encoder() {
        "kubeseal" => {
            info!("Encrypting K8s secrets with kubeseal");
            Arc::new(SealedSecretsEncoder) as Arc<dyn K8Encoder>
        }
        "noop" => {
            warn!("No secret encryption of secrets for K8s");
            Arc::new(NoopEncoder) as Arc<dyn K8Encoder>
        }
        _ => panic!("Invalid k8s encoder"),
    };

    println!("\n\n");
    match Reactor::from_product_dir(
        config,
        toolchain,
        vault,
        secrets_encoder,
        k8s_encoder,
        redirected_components,
        silence_components,
    ) {
        Ok(reactor) => Ok(reactor),
        Err(e) => {
            error!("Failed to create Reactor: {}", e);
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
