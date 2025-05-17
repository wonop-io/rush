#[macro_use]
extern crate tera;

mod build;
mod cli;
mod cluster;
mod container;
mod core;
mod public_env_defs;
mod toolchain;
mod utils;
mod vault;

use crate::build::Config;
use crate::cluster::{K8Validation, KubeconformValidator, KubevalValidator};
use clap::value_parser;
use log::{debug, error, info, trace};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;
use std::{path::Path, sync::Arc};
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
    // Initialize the application
    cli::init_application().await?;

    // Parse command line arguments
    let matches = cli::args::parse_args();

    // Initialize CLI context with common resources
    let mut ctx = cli::context::create_context(&matches).await?;

    // Execute the appropriate command based on arguments
    cli::execute_command(&matches, &mut ctx).await
}
