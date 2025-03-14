#[macro_use]
extern crate tera;

mod builder;
mod cluster;
mod container;
mod dotenv_utils;
mod path_matcher;
mod public_env_defs;
mod toolchain;
mod utils;
mod vault;

use crate::builder::Config;
use crate::cluster::{K8Encoder, NoopEncoder, SealedSecretsEncoder};
use crate::cluster::{K8Validation, KubeconformValidator, KubevalValidator};
use crate::container::ContainerReactor;
use crate::public_env_defs::PublicEnvironmentDefinitions;
use crate::toolchain::Platform;
use crate::toolchain::ToolchainContext;
use crate::utils::Directory;
use crate::vault::Base64SecretsEncoder;
use crate::vault::SecretsDefinitions;
use clap::{arg, value_parser, Arg, Command};
use cluster::Minikube;
use colored::Colorize;
use log::warn;
use log::{debug, error, info, trace};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
use std::{path::Path, sync::Arc};
use tera::Context;
use tokio::io;
use vault::{DotenvVault, FileVault, OnePassword, Vault};
fn create_k8s_validator(config: &Config) -> Box<dyn K8Validation> {
    match config.k8s_validator() {
        "kubeconform" => Box::new(KubeconformValidator),
        "kubeval" => Box::new(KubevalValidator),
        _ => panic!("Invalid k8s validator"),
    }
}

fn setup_environment() {
    trace!("Setting up environment");

    // Set the RUSHD_ROOT environment variable
    let binding = env::current_dir().unwrap();
    let rushd_root = binding
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .expect("Unable to find git repository amounts ancestors");
    env::set_var("RUSHD_ROOT", rushd_root);
    debug!("RUSHD_ROOT set to: {:?}", rushd_root);

    // Set the HOME environment variable if not already set
    if env::var("HOME").is_err() {
        if let Some(home) = env::var_os("USERPROFILE") {
            env::set_var("HOME", home);
            debug!("HOME environment variable set from USERPROFILE");
        } else {
            error!("The HOME environment variable is not set.");
            panic!("The HOME environment variable is not set.");
        }
    }
    // Set the PATH environment variable
    let home_dir = env::var_os("HOME").unwrap();
    let cargo_bin = Path::new(&home_dir).join(".cargo/bin");
    let current_path = env::var_os("PATH").unwrap();
    // let new_path = env::join_paths([current_path, cargo_bin.into()].iter()).unwrap();
    // env::set_var("PATH", new_path);

    // Set toolchain environment variables for macOS ARM architecture
    if cfg!(target_os = "macos") && cfg!(target_arch = "arm") {
        trace!("Setting up toolchain for macOS ARM architecture");

        let toolchain_base = "/opt/homebrew/Cellar/x86_64-unknown-linux-gnu";
        let toolchain_path = std::fs::read_dir(toolchain_base)
            .expect("Failed to read toolchain directory")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .max_by_key(|entry| entry.file_name())
            .map(|entry| entry.path().join("bin").to_string_lossy().into_owned())
            .expect("No toolchain version found");

        let toolchain_path = format!("{}/", toolchain_path);
        debug!("Using toolchain path: {}", toolchain_path);

        env::set_var(
            "CC",
            format!("{}x86_64-unknown-linux-gnu-gcc", toolchain_path),
        );
        env::set_var(
            "CXX",
            format!("{}x86_64-unknown-linux-gnu-g++", toolchain_path),
        );
        env::set_var(
            "AR",
            format!("{}x86_64-unknown-linux-gnu-ar", toolchain_path),
        );
        env::set_var(
            "RANLIB",
            format!("{}x86_64-unknown-linux-gnu-ranlib", toolchain_path),
        );
        env::set_var(
            "NM",
            format!("{}x86_64-unknown-linux-gnu-nm", toolchain_path),
        );
        env::set_var(
            "STRIP",
            format!("{}x86_64-unknown-linux-gnu-strip", toolchain_path),
        );
        env::set_var(
            "OBJDUMP",
            format!("{}x86_64-unknown-linux-gnu-objdump", toolchain_path),
        );
        env::set_var(
            "OBJCOPY",
            format!("{}x86_64-unknown-linux-gnu-objcopy", toolchain_path),
        );
        env::set_var(
            "LD",
            format!("{}x86_64-unknown-linux-gnu-ld", toolchain_path),
        );
        debug!("Toolchain environment variables set for macOS ARM");
    }

    trace!("Environment setup complete");
}

#[derive(Debug, Deserialize)]
struct RushdConfig {
    env: HashMap<String, String>,
}

fn load_config() {
    trace!("Loading configuration");
    let config_path = "rushd.yaml";
    let mut file = File::open(config_path).expect("Unable to open the config file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Unable to read the config file");

    let config: RushdConfig =
        serde_yaml::from_str(&contents).expect("Error parsing the config file");

    for (key, value) in config.env {
        debug!(
            "Set environment variable: {}={}",
            key.clone(),
            value.clone()
        );
        std::env::set_var(key, &value);
    }
    trace!("Configuration loaded successfully");
}

#[derive(Deserialize)]
struct Release {
    url: String,
    tag_name: String,
    name: String,
    draft: bool,
    prerelease: bool,
}

async fn check_version() {
    let version = env!("CARGO_PKG_VERSION");
    let url = format!("https://api.github.com/repos/wonop-io/rush/releases/latest");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", "rush")
        .send()
        .await
        .unwrap();

    let release: Release = match resp.json().await {
        Ok(release) => release,
        Err(e) => {
            panic!("Failed to get release: {}", e);
        }
    };

    let latest_version = release
        .tag_name
        .replace("v.", "")
        .replace("v", "")
        .replace(" ", "");
    let current_version = semver::Version::parse(version).expect("Failed to parse current version");
    let latest_version =
        semver::Version::parse(&latest_version).expect("Failed to parse latest version");

    if latest_version > current_version {
        println!("============================================================");
        println!("* A new version of Rush is available: {}", release.tag_name);
        println!("* Please update it by running:");
        println!("* ");
        println!("* cargo install rush-cli --force");
        println!("* ");
        println!("============================================================");
        println!();
        std::process::exit(1);
    }
}

fn create_vault(
    product_path: &PathBuf,
    config: &Config,
    name: &str,
) -> Arc<Mutex<dyn Vault + Send>> {
    let vault = match name {
        ".env" => {
            info!("Vault: .env");
            Arc::new(Mutex::new(DotenvVault::new(product_path.clone())))
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
            let json_path = std::path::PathBuf::from(
                config
                    .json_vault_dir()
                    .expect("JSON path not found. Please set this in rushd.yaml"),
            );
            info!("JSON Vault: {}", json_path.display());
            Arc::new(Mutex::new(FileVault::new(json_path, None))) as Arc<Mutex<dyn Vault + Send>>
        }
        _ => panic!("Invalid vault"),
    };
    vault
}

#[tokio::main]
async fn main() -> io::Result<()> {
    check_version().await;

    // Add for debugging console_subscriber::init();
    setup_environment();

    // TODO: Get the rushd root by go levels up until you find ".git" directory
    let root_dir = std::env::var("RUSHD_ROOT").unwrap();
    let _guard = Directory::chdir(&root_dir);
    debug!("Changed directory to RUSHD_ROOT: {}", root_dir);
    load_config();

    dotenv::dotenv().ok();

    let version = env!("CARGO_PKG_VERSION");
    // https://api.github.com/repos/wonop-io/rush/releases
    let matches = Command::new("rush")
        .version(version)
        .author("Troels F. Rønnow <troels@wonop.com>")
        .about("Rush is designed as an all-around support unit for developers, transforming the development workflow with its versatile capabilities. It offers a suite of tools for building, deploying, and managing applications, adapting to the diverse needs of projects with ease.")
        .arg(arg!(target_arch : --arch <TARGET_ARCH> "Target architecture"))
        .arg(arg!(target_os : --os <TARGET_OS> "Target OS"))
        .arg(arg!(environment : --env <ENVIRONMENT> "Environment"))
        .arg(arg!(docker_registry : --registry <DOCKER_REGISTRY> "Docker Registry"))
        .arg(arg!(log_level : -l --loglevel <LOG_LEVEL> "Log level (trace, debug, info, warn, error)").default_value("info"))
        .arg(arg!(start_port: --port <START_PORT> "Starting port for services").value_parser(value_parser!(u16)).default_value("8129"))
        .arg(Arg::new("product_name").required(true))
        .subcommand(Command::new("describe")
            .about("Describes the current configuration")
            .subcommand(Command::new("toolchain")
                .about("Describes the current toolchain")
            )
            .subcommand(Command::new("images")
                .about("Describes the current images")
            )
            .subcommand(Command::new("services")
                .about("Describes the current services")
            )
            .subcommand(Command::new("build-script")
                .about("Describes the current build script")
                .arg(Arg::new("component_name").required(true))
            )
            .subcommand(Command::new("build-context")
                .about("Describes the current build context")
                .arg(Arg::new("component_name").required(true))
            )
            .subcommand(Command::new("artefacts")
                .about("Describes the current artefacts")
                .arg(Arg::new("component_name").required(true))
            )
            .subcommand(Command::new("k8s")
                .about("Describes the current k8s")
            )
        )
        .subcommand(Command::new("dev")
            .arg(arg!(redirect : --redirect <COMPONENTS> ... "Disables component and redirects the ingress. Format: component@host:port").num_args(1..))
            .arg(arg!(silence : --silence <COMPONENTS> ... "Silence output for specific components").num_args(1..))
        )
        .subcommand(Command::new("build"))
        .subcommand(Command::new("push"))
        .subcommand(Command::new("minikube")
            .about("Runs tasks on minikube")
            .subcommand(Command::new("dev"))
            .subcommand(Command::new("start"))
            .subcommand(Command::new("stop"))
            .subcommand(Command::new("delete"))
        )
        .subcommand(Command::new("rollout")
            .about("Rolls out the product into staging or production")
        )
        .subcommand(Command::new("deploy"))
        .subcommand(Command::new("install"))
        .subcommand(Command::new("uninstall"))
        .subcommand(Command::new("apply"))
        .subcommand(Command::new("unapply"))
        .subcommand(Command::new("validate")
            .about("Validates Kubernetes manifests")
            .subcommand(Command::new("manifests")
                .about("Validates Kubernetes manifests with schema validation")
                .arg(arg!(target_version : --version <K8S_VERSION> "Target Kubernetes version"))
            )
            .subcommand(Command::new("deprecations")
                .about("Checks for deprecated APIs in Kubernetes manifests")
                .arg(arg!(target_version : --version <K8S_VERSION> "Target Kubernetes version"))
            )
        )
        .subcommand(Command::new("vault")
            .about("Manages vault operations")
            .subcommand(Command::new("create"))
            .subcommand(Command::new("add")
                .arg(Arg::new("component_name").required(true))
                .arg(Arg::new("secrets").required(true))
            )
            .subcommand(Command::new("remove")
                .arg(Arg::new("component_name").required(true))
            )
            .subcommand(Command::new("migrate")
                .about("Migrates secrets")
                .arg(Arg::new("dest").required(true))
            )
        )
        .subcommand(Command::new("secrets")
            .about("Manages secrets")
            .subcommand(Command::new("init")
                .about("Initializes secrets")
            )
        )
        .get_matches();

    let start_port = *matches.get_one::<u16>("start_port").unwrap();
    let redirected_components: HashMap<String, (String, u16)> = matches
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
        .unwrap_or_default();

    let silence_components: Vec<String> = matches
        .subcommand_matches("dev")
        .and_then(|dev_matches| dev_matches.get_many::<String>("silence"))
        .map(|values| values.cloned().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    log::info!("Redirecting components: {:#?}", redirected_components);

    debug!("Command line arguments parsed");

    // Set log level based on command line argument
    if let Some(log_level) = matches.get_one::<String>("log_level") {
        env::set_var("RUST_LOG", log_level);
        env_logger::builder().parse_env("RUST_LOG").init();
        trace!("Log level set to: {}", log_level);
    } else {
        // Initialize env_logger
        env_logger::init();
    }
    // Log the start of the application
    trace!("Starting Rush application");

    let target_arch = if let Some(target_arch) = matches.get_one::<String>("target_arch") {
        target_arch.clone()
    } else {
        "x86_64".to_string()
    };
    info!("Target architecture: {}", target_arch);

    let target_os = if let Some(target_os) = matches.get_one::<String>("target_os") {
        target_os.clone()
    } else {
        "linux".to_string()
    };
    info!("Target OS: {}", target_os);

    let environment = if let Some(environment) = matches.get_one::<String>("environment") {
        environment.clone()
    } else {
        "local".to_string()
    };
    info!("Environment: {}", environment);

    let docker_registry = if let Some(docker_registry) =
        matches.get_one::<String>("docker_registry")
    {
        docker_registry.clone()
    } else {
        std::env::var("DOCKER_REGISTRY").expect("DOCKER_REGISTRY environment variable not found")
    };
    info!("Docker registry: {}", docker_registry);

    let product_name = matches.get_one::<String>("product_name").unwrap();
    info!("Product name: {}", product_name);

    let config = match Config::new(
        &root_dir,
        product_name,
        &environment,
        &docker_registry,
        start_port,
    ) {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to create config: {}", e);
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    // Loading secrets definitions and creating the vault
    let secrets_context = SecretsDefinitions::new(
        product_name.clone(),
        &format!("{}/stack.env.secrets.yaml", config.product_path()),
    );
    let product_path = std::path::PathBuf::from(config.product_path());

    let vault = create_vault(&product_path, &config, config.vault_name());

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

    // Creating environment
    let public_environment = PublicEnvironmentDefinitions::new(
        product_name.clone(),
        &format!("{}/stack.env.base.yaml", config.product_path()),
        &format!("{}/stack.env.{}.yaml", config.product_path(), environment),
    );
    match public_environment.generate_dotenv_files() {
        Ok(_) => (),
        Err(e) => {
            error!("Unable to generate dotenv files: {}", e);
            eprintln!("{:#?}", e);
            std::process::exit(1);
        }
    }

    let toolchain = Arc::new(ToolchainContext::new(
        Platform::default(),
        Platform::new(&target_os, &target_arch),
    ));
    toolchain.setup_env();
    debug!("Toolchain set up");

    println!("\n\n");
    let mut reactor = match ContainerReactor::from_product_dir(
        config.clone(),
        toolchain.clone(),
        vault.clone(),
        secrets_encoder,
        k8s_encoder,
        redirected_components,
        silence_components,
    ) {
        Ok(reactor) => reactor,
        Err(e) => {
            error!("Failed to create ContainerReactor: {}", e);
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let minikube = Minikube::new(toolchain.clone());

    if let Some(validate_matches) = matches.subcommand_matches("validate") {
        let _pop_dir = Directory::chdir(reactor.product_directory());
        if let Some(manifest_matches) = validate_matches.subcommand_matches("manifests") {
            let target_version = manifest_matches
                .get_one::<String>("version")
                .map(|v| v.as_str())
                .unwrap_or_else(|| config.k8s_version());
            let validator = create_k8s_validator(&config);

            let mut validation_failed = false;
            for component in reactor.cluster_manifests().components() {
                trace!(
                    "Validating manifests for component: {}",
                    component.spec().component_name
                );
                if let Err(e) = validator.validate(
                    component.output_directory().to_str().unwrap(),
                    target_version,
                ) {
                    error!(
                        "Validation failed for {}: {}",
                        component.spec().component_name,
                        e
                    );
                    validation_failed = true;
                }
            }

            if validation_failed {
                println!("One or more components failed validation!");
                std::process::exit(1);
            } else {
                println!("All manifests validated successfully!");
                std::process::exit(0);
            }
        }
    }

    if let Some(describe_matches) = matches.subcommand_matches("describe") {
        trace!("Executing 'describe' subcommand");
        if let Some(_) = describe_matches.subcommand_matches("toolchain") {
            println!("{:#?}", toolchain);
            debug!("Described toolchain");
            std::process::exit(0);
        }

        if let Some(_) = describe_matches.subcommand_matches("images") {
            println!("{:#?}", reactor.images());
            debug!("Described images");
            std::process::exit(0);
        }

        if let Some(_) = describe_matches.subcommand_matches("services") {
            println!("{:#?}", reactor.services());
            debug!("Described services");
            std::process::exit(0);
        }

        if let Some(build_script_matches) = describe_matches.subcommand_matches("build-script") {
            trace!("Describing build script");
            if let Some(component_name) = build_script_matches.get_one::<String>("component_name") {
                trace!("Describing build script for component: {}", component_name);
                let image = reactor
                    .get_image(component_name)
                    .expect("Component not found");
                let secrets = vault
                    .lock()
                    .unwrap()
                    .get(&product_name, &component_name, &environment)
                    .await
                    .unwrap_or_default();
                let ctx = image.generate_build_context(secrets);

                println!("{}", image.build_script(&ctx).unwrap());
                debug!("Described build script for component: {}", component_name);
                std::process::exit(0);
            }
            error!("No component name provided");
            std::process::exit(1);
        }

        if let Some(build_context_matches) = describe_matches.subcommand_matches("build-context") {
            if let Some(component_name) = build_context_matches.get_one::<String>("component_name")
            {
                trace!("Describing build context for component: {}", component_name);
                let image = reactor
                    .get_image(component_name)
                    .expect("Component not found");
                let secrets = vault
                    .lock()
                    .unwrap()
                    .get(&product_name, &component_name, &environment)
                    .await
                    .unwrap_or_default();
                let ctx = image.generate_build_context(secrets);
                let ctx = Context::from_serialize(ctx).expect("Could not create context");
                println!("{:#?}", ctx);
                debug!("Described build context for component: {}", component_name);
                std::process::exit(0);
            }
            error!("No component name provided");
            std::process::exit(1);
        }

        if let Some(artefacts_matches) = describe_matches.subcommand_matches("artefacts") {
            if let Some(component_name) = artefacts_matches.get_one::<String>("component_name") {
                let _pop_dir = Directory::chdir(reactor.product_directory());
                trace!("Describing artefacts for component: {}", component_name);
                let image = reactor
                    .get_image(component_name)
                    .expect("Component not found");
                let secrets = vault
                    .lock()
                    .unwrap()
                    .get(&product_name, &component_name, &environment)
                    .await
                    .unwrap_or_default();
                let ctx = image.generate_build_context(secrets);
                for (k, v) in image.spec().build_artefacts() {
                    let message = format!("{} {}", "Artefact".green(), k.white());
                    println!("{}\n", &message.bold());

                    println!("{}\n", v.render(&ctx));
                }
                debug!("Described artefacts for component: {}", component_name);
                std::process::exit(0);
            }
            error!("No component name provided");
            std::process::exit(1);
        }

        if let Some(_) = describe_matches.subcommand_matches("k8s") {
            trace!("Describing Kubernetes manifests");
            let manifests = reactor.cluster_manifests();
            for component in manifests.components() {
                println!(
                    "{} -> {}",
                    component.input_directory().display(),
                    component.output_directory().display()
                );
                let spec = component.spec();
                let secrets = vault
                    .lock()
                    .unwrap()
                    .get(&product_name, &spec.component_name, &environment)
                    .await
                    .unwrap_or_default();
                let ctx = spec.generate_build_context(Some(toolchain.clone()), secrets);
                for manifest in component.manifests() {
                    println!("{}", manifest.render(&ctx));
                }
                println!();
            }
            debug!("Described Kubernetes manifests");
            std::process::exit(0);
        }
    }

    if let Some(matches) = matches.subcommand_matches("vault") {
        trace!("Executing 'vault' subcommand");
        if let Some(matches) = matches.subcommand_matches("migrate") {
            let dest = matches.get_one::<String>("dest").unwrap();
            let dest_vault = create_vault(&product_path, &config, dest.as_str());
            trace!("Migrating secrets to: {}", dest);

            let mut dest_vault = dest_vault.lock().unwrap();
            dest_vault.create_vault(product_name);

            let vault = vault.lock().unwrap();
            let manifests = reactor.cluster_manifests();

            println!("Migrating:");
            for component_name in reactor.available_components() {
                println!(" - {}", component_name);
                let secrets = vault
                    .get(&product_name, &component_name, &environment)
                    .await
                    .unwrap_or_default();

                if !secrets.is_empty() {
                    dest_vault
                        .set(&product_name, &component_name, &environment, secrets)
                        .await
                        .unwrap();
                }
            }
        }

        if matches.subcommand_matches("create").is_some() {
            trace!("Creating vault");
            match vault.lock().unwrap().create_vault(product_name).await {
                Ok(_) => {
                    trace!("Vault created successfully");
                    return Ok(());
                }
                Err(e) => {
                    error!("Failed to create vault: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }

        if let Some(matches) = matches.subcommand_matches("add") {
            let component_name = matches.get_one::<String>("component_name").unwrap();
            let secrets = matches.get_one::<String>("secrets").unwrap();
            trace!("Adding: {}", secrets);
            let secrets: HashMap<String, String> =
                serde_json::from_str(secrets).expect("Invalid secrets format");

            trace!("Adding secrets to vault");
            match vault
                .lock()
                .unwrap()
                .set(product_name, component_name, &environment, secrets)
                .await
            {
                Ok(_) => {
                    trace!("Secrets added successfully");
                    return Ok(());
                }
                Err(e) => {
                    error!("Failed to add secrets: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }

        if let Some(matches) = matches.subcommand_matches("remove") {
            let component_name = matches.get_one::<String>("component_name").unwrap();

            trace!("Removing secrets from vault");

            match vault
                .lock()
                .unwrap()
                .remove(product_name, component_name, &environment)
                .await
            {
                Ok(_) => {
                    trace!("Secrets removed successfully");
                    return Ok(());
                }
                Err(e) => {
                    error!("Failed to remove secrets: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }
        return Ok(());
    }

    if let Some(matches) = matches.subcommand_matches("secrets") {
        trace!("Executing 'secrets' subcommand");

        if matches.subcommand_matches("init").is_some() {
            match vault.lock().unwrap().create_vault(product_name).await {
                Ok(_) => (),
                Err(e) => {
                    error!("Failed to create vault: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            trace!("Initializing secrets");
            match secrets_context.populate(vault.clone(), &environment).await {
                Ok(_) => {
                    trace!("Secrets initialized successfully");
                    return Ok(());
                }
                Err(e) => {
                    error!("Failed to initialize secrets: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    // Validate secrets
    if let Err(e) = secrets_context
        .validate_vault(vault.clone(), &environment)
        .await
    {
        error!("Missing secrets in vault: {}", e);
        eprintln!("{}", e);
        std::process::exit(1);
    }

    // Run and deploy Operations
    if matches.subcommand_matches("dev").is_some() {
        trace!("Launching development environment");
        match reactor.launch().await {
            Ok(_) => {
                trace!("Development environment launched successfully");
                return Ok(());
            }
            Err(e) => {
                error!("Failed to launch development environment: {}", e);
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    if matches.subcommand_matches("build").is_some() {
        match reactor.build().await {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    if matches.subcommand_matches("push").is_some() {
        match reactor.build_and_push().await {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    // Setting the context
    if !toolchain.has_kubectl() {
        eprintln!("kubectl not found");
        std::process::exit(1);
    }

    match reactor
        .select_kubernetes_context(config.kube_context())
        .await
    {
        Ok(_) => (),
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }

    if matches.subcommand_matches("rollout").is_some() {
        match reactor.rollout().await {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    if matches.subcommand_matches("install").is_some() {
        match reactor.install_manifests().await {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    if matches.subcommand_matches("uninstall").is_some() {
        match reactor.uninstall_manifests().await {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    if matches.subcommand_matches("deploy").is_some() {
        match reactor.deploy().await {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    if matches.subcommand_matches("apply").is_some() {
        match reactor.apply().await {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    if matches.subcommand_matches("unapply").is_some() {
        match reactor.unapply().await {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
