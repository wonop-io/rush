use crate::context::CliContext;
use clap::ArgMatches;
use log::{debug, error, info, trace, warn};
use rush_config::environment::EnvironmentGenerator;
use rush_config::{Config, ConfigLoader};
use rush_container::ContainerReactor;
use rush_core::constants::*;
use rush_core::error::Result;
use rush_k8s::encoder::{K8sEncoder, NoopEncoder, SealedSecretsEncoder};
use rush_output::simple::Sink;
use rush_output::sink_proxy::SinkProxy;
use rush_security::EnvironmentDefinitions;
use rush_security::{SecretsDefinitions, SecretsEncoder, Vault};
use rush_toolchain::{Platform, ToolchainContext};
use rush_utils::Directory;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

pub async fn create_context(
    matches: &ArgMatches,
    output_sink: Arc<TokioMutex<Box<dyn Sink>>>,
) -> Result<CliContext> {
    let start_port = *matches.get_one::<u16>("start_port").unwrap();
    let redirected_components = parse_redirected_components(matches);
    let silence_components = parse_silence_components(matches);

    let target_arch = get_target_arch(matches);
    let target_os = get_target_os(matches);
    let environment = get_environment(matches);
    let docker_registry = get_docker_registry(matches);
    let product_name = matches
        .get_one::<String>("product_name")
        .ok_or("Product name is required. Usage: rush <product_name> <command>")
        .map_err(|e| rush_core::error::Error::Config(e.to_string()))?
        .clone();

    info!("Product name: {}", product_name);

    let root_dir = env::var("RUSHD_ROOT").unwrap();
    let config = create_config(
        &root_dir,
        &product_name,
        &environment,
        &docker_registry,
        start_port,
    )?;

    // Start local services BEFORE creating .env files (if this is for the dev command)
    let local_service_env_vars = if matches.subcommand_matches("dev").is_some() {
        info!("Starting local services before environment file generation...");
        let docker_command = env::var("DOCKER_COMMAND").unwrap_or_else(|_| "docker".to_string());
        match crate::local_services_startup::start_local_services(&config, &product_name, &docker_command).await {
            Ok(env_vars) => {
                info!("Local services started with {} environment variables", env_vars.len());
                env_vars
            }
            Err(e) => {
                warn!("Failed to start local services: {}", e);
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };

    // Inject local service environment variables into the process environment
    // so they're available during .env generation
    for (key, value) in &local_service_env_vars {
        env::set_var(key, value);
    }

    // Create secrets and vault
    let (secrets_context, vault) = setup_secrets(&config, &product_name)?;

    // Setup environment files (now with local service env vars available)
    setup_environment_files(&config, &product_name, &environment)?;

    // Create toolchain
    let toolchain = create_toolchain(&target_os, &target_arch);

    // Create reactor
    let mut reactor = create_reactor(
        config.clone(),
        toolchain.clone(),
        vault.clone(),
        redirected_components,
        silence_components,
        local_service_env_vars,
    )?;
    
    // Set the output sink on the reactor
    {
        // Clone the Arc to get a reference we can use
        let sink_clone = output_sink.clone();
        // We need to extract the inner sink to pass to the reactor
        // This is a temporary solution - ideally the reactor would accept Arc<Mutex<Box<dyn Sink>>>
        let sink_for_reactor = Box::new(SinkProxy::new(sink_clone));
        reactor.set_output_sink(sink_for_reactor);
    }

    Ok(CliContext::new(
        config,
        environment,
        product_name,
        toolchain,
        reactor,
        vault,
        secrets_context,
        output_sink,
    ))
}

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

pub fn setup_logging(matches: &ArgMatches) {
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
        let registry = env::var("DOCKER_REGISTRY").unwrap_or_else(|_| {
            warn!("DOCKER_REGISTRY environment variable not found, using empty registry");
            DEFAULT_DOCKER_REGISTRY.to_string()
        });

        // If registry is "not_set", treat it as empty for local development
        let registry = if registry == "not_set" {
            debug!("DOCKER_REGISTRY is 'not_set', using empty registry for local development");
            "".to_string()
        } else {
            registry
        };

        info!(
            "Docker registry: {}",
            if registry.is_empty() {
                "(local)"
            } else {
                &registry
            }
        );
        registry
    }
}

fn create_config(
    root_dir: &str,
    product_name: &str,
    environment: &str,
    docker_registry: &str,
    start_port: u16,
) -> Result<Arc<Config>> {
    let _root_guard = Directory::chdir(root_dir);

    let config_loader = ConfigLoader::new(PathBuf::from(root_dir));
    match config_loader.load_config(product_name, environment, docker_registry, start_port) {
        Ok(config) => Ok(config),
        Err(e) => {
            error!("Failed to create config: {}", e);
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

fn setup_secrets(
    config: &Config,
    product_name: &str,
) -> Result<(SecretsDefinitions, Arc<Mutex<dyn Vault + Send>>)> {
    let secrets_context = SecretsDefinitions::new(
        product_name.to_string(),
        &format!("{}/stack.env.secrets.yaml", config.product_path().display()),
    );
    let product_path = PathBuf::from(config.product_path());

    let vault = create_vault(&product_path, config, config.vault_name());

    Ok((secrets_context, vault))
}

fn create_vault(product_path: &Path, config: &Config, name: &str) -> Arc<Mutex<dyn Vault + Send>> {
    use rush_security::vault::OnePassword;
    use rush_security::{DotenvVault, FileVault};

    match name {
        ".env" => {
            info!("Vault: .env");
            Arc::new(Mutex::new(DotenvVault::new(product_path.to_path_buf())))
                as Arc<Mutex<dyn Vault + Send>>
        }
        "1Password" => {
            let account_name = config
                .one_password_account()
                .expect("1Password account not found. Please set this in rushd.yaml");
            info!("Vault: {}", account_name);
            Arc::new(Mutex::new(OnePassword::new(account_name))) as Arc<Mutex<dyn Vault + Send>>
        }
        "json" => {
            let json_path = PathBuf::from(
                config
                    .json_vault_dir()
                    .expect("JSON path not found. Please set this in rushd.yaml"),
            );
            info!("JSON Vault: {}", json_path.display());
            Arc::new(Mutex::new(FileVault::new(json_path, None))) as Arc<Mutex<dyn Vault + Send>>
        }
        _ => panic!("Invalid vault"),
    }
}

fn setup_environment_files(config: &Config, product_name: &str, environment: &str) -> Result<()> {
    // Use EnvironmentDefinitions for component-level .env generation
    let env_definitions = EnvironmentDefinitions::new(
        product_name.to_string(),
        &format!("{}/stack.env.base.yaml", config.product_path().display()),
        &format!(
            "{}/stack.env.{}.yaml",
            config.product_path().display(),
            environment
        ),
    );

    match env_definitions.generate_dotenv_files() {
        Ok(_) => {
            debug!("Generated component-level .env files");
            Ok(())
        }
        Err(e) => {
            // Fallback to simple generator if component-level fails
            warn!(
                "Component-level env generation failed, trying simple generator: {}",
                e
            );
            let simple_generator = EnvironmentGenerator::new(
                product_name.to_string(),
                &format!("{}/stack.env.base.yaml", config.product_path().display()),
                &format!(
                    "{}/stack.env.{}.yaml",
                    config.product_path().display(),
                    environment
                ),
            );

            match simple_generator.generate_dotenv_files() {
                Ok(_) => Ok(()),
                Err(e) => {
                    error!("Unable to generate dotenv files: {}", e);
                    eprintln!("{e:#?}");
                    process::exit(1);
                }
            }
        }
    }
}

fn create_toolchain(target_os: &str, target_arch: &str) -> Arc<ToolchainContext> {
    let toolchain = Arc::new(ToolchainContext::create_with_platforms(
        Platform::default(),
        Platform::new(target_os, target_arch),
    ));
    toolchain.setup_env();
    debug!("Toolchain set up");
    toolchain
}

fn create_reactor(
    config: Arc<Config>,
    _toolchain: Arc<ToolchainContext>,
    vault: Arc<Mutex<dyn Vault + Send>>,
    redirected_components: HashMap<String, (String, u16)>,
    silence_components: Vec<String>,
    local_service_env_vars: HashMap<String, String>,
) -> Result<ContainerReactor> {
    // TODO: Resolve conflicting name for NoopEncoder
    let secrets_encoder: Arc<dyn SecretsEncoder> = Arc::new(rush_security::NoopEncoder);
    // TODO: Fix k8s encoding - also this seems redudant or at least very similar to dev
    let _k8s_encoder = match config.k8s_encoder() {
        "kubeseal" => {
            info!("Encrypting K8s secrets with kubeseal");
            Arc::new(SealedSecretsEncoder) as Arc<dyn K8sEncoder>
        }
        "noop" => {
            warn!("No secret encryption of secrets for K8s");
            Arc::new(NoopEncoder) as Arc<dyn K8sEncoder>
        }
        _ => panic!("Invalid k8s encoder"),
    };

    println!("\n\n");
    match ContainerReactor::from_product_dir(
        config,
        vault,
        secrets_encoder,
        redirected_components,
        silence_components,
    ) {
        Ok(mut reactor) => {
            // Add local service environment variables to the reactor
            for (key, value) in local_service_env_vars {
                reactor.add_env_var(key, value);
            }
            Ok(reactor)
        }
        Err(e) => {
            error!("Failed to create Reactor: {}", e);
            eprintln!("{e}");
            process::exit(1);
        }
    }
}
