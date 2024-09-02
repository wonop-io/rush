mod container;
mod utils;
mod toolchain;
mod cluster;
mod builder;
mod gitignore;
mod vault;
mod dotenv_utils;

use crate::toolchain::Platform;
use clap::{arg, Command,Arg};
use tokio::io;
use std::{path::Path, sync::Arc};
use crate::container::ContainerReactor;
use crate::utils::Directory;
use crate::toolchain::ToolchainContext;
use cluster::Minikube;
use colored::Colorize;
use crate::builder::Config;
use std::env;
use std::fs::File;
use std::io::Read;
use serde::Deserialize;
use std::collections::HashMap;
use log::{info, error, debug};
use vault::{OnePassword, Vault, DotenvVault};
use std::sync::Mutex;
use crate::vault::SecretsDefinitions;

fn setup_environment() {
    info!("Setting up environment");
    
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
        info!("Setting up toolchain for macOS ARM architecture");
        
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

        env::set_var("CC", format!("{}x86_64-unknown-linux-gnu-gcc", toolchain_path));
        env::set_var("CXX", format!("{}x86_64-unknown-linux-gnu-g++", toolchain_path));
        env::set_var("AR", format!("{}x86_64-unknown-linux-gnu-ar", toolchain_path));
        env::set_var("RANLIB", format!("{}x86_64-unknown-linux-gnu-ranlib", toolchain_path));
        env::set_var("NM", format!("{}x86_64-unknown-linux-gnu-nm", toolchain_path));
        env::set_var("STRIP", format!("{}x86_64-unknown-linux-gnu-strip", toolchain_path));
        env::set_var("OBJDUMP", format!("{}x86_64-unknown-linux-gnu-objdump", toolchain_path));
        env::set_var("OBJCOPY", format!("{}x86_64-unknown-linux-gnu-objcopy", toolchain_path));
        env::set_var("LD", format!("{}x86_64-unknown-linux-gnu-ld", toolchain_path));
        debug!("Toolchain environment variables set for macOS ARM");
    }

    info!("Environment setup complete");
}



#[derive(Debug, Deserialize)]
struct RushdConfig {
    env: HashMap<String, String>,
}

fn load_config() {
    info!("Loading configuration");
    let config_path = "rushd.yaml";
    let mut file = File::open(config_path).expect("Unable to open the config file");
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect("Unable to read the config file");

    let config: RushdConfig = serde_yaml::from_str(&contents).expect("Error parsing the config file");

    for (key, value) in config.env {
        debug!("Set environment variable: {}={}", key.clone(), value.clone());
        std::env::set_var(key, &value);
    }
    info!("Configuration loaded successfully");
}





#[tokio::main]
async fn main() -> io::Result<()> {
 
    setup_environment();

    // TODO: Get the rushd root by go levels up until you find ".git" directory
    let root_dir = std::env::var("RUSHD_ROOT").unwrap();
    let _guard = Directory::chdir(&root_dir);
    debug!("Changed directory to RUSHD_ROOT: {}", root_dir);
    load_config();

    dotenv::dotenv().ok();    
    
    let matches = Command::new("rush")
        .version("0.1.0")
        .author("Your Name <your_email@example.com>")
        .about("Rush is designed as an all-around support unit for developers, transforming the development workflow with its versatile capabilities. It offers a suite of tools for building, deploying, and managing applications, adapting to the diverse needs of projects with ease.")
        .arg(arg!(target_arch : --arch <TARGET_ARCH> "Target architecture"))
        .arg(arg!(target_os : --os <TARGET_OS> "Target OS"))
        .arg(arg!(environment : --env <ENVIRONMENT> "Environment"))
        .arg(arg!(docker_registry : --registry <DOCKER_REGISTRY> "Docker Registry"))
        .arg(arg!(log_level : -l --loglevel <LOG_LEVEL> "Log level (trace, debug, info, warn, error)").default_value("info"))
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
        .subcommand(Command::new("dev"))
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
            .subcommand(Command::new("generate"))
        )
        .subcommand(Command::new("secrets")
            .about("Manages secrets")
            .subcommand(Command::new("init")
                .about("Initializes secrets")
            )
        )
        .get_matches();

    debug!("Command line arguments parsed");

    // Set log level based on command line argument
    if let Some(log_level) = matches.get_one::<String>("log_level") {
        env::set_var("RUST_LOG", log_level);
        env_logger::builder().parse_env("RUST_LOG").init();
        info!("Log level set to: {}", log_level);
    } else {

        // Initialize env_logger
        env_logger::init();
    }
    // Log the start of the application
    info!("Starting Rush application");


    let target_arch = if let Some(target_arch) = matches.get_one::<String>("target_arch") {
        target_arch.clone()
    } else {
        "x86_64".to_string()
    };
    debug!("Target architecture: {}", target_arch);

    let target_os = if let Some(target_os) = matches.get_one::<String>("target_os") {
        target_os.clone()
    } else {
        "linux".to_string()
    };
    debug!("Target OS: {}", target_os);

    let environment = if let Some(environment) = matches.get_one::<String>("environment") {
        environment.clone()
    } else {
        "dev".to_string()
    };
    debug!("Environment: {}", environment);

    


    let docker_registry = if let Some(docker_registry) = matches.get_one::<String>("docker_registry") {
        docker_registry.clone()
    } else {
        std::env::var("DOCKER_REGISTRY").expect("DOCKER_REGISTRY environment variable not found")
    };
    debug!("Docker registry: {}", docker_registry);


    let product_name = matches.get_one::<String>("product_name").unwrap();
    info!("Product name: {}", product_name);

    let config = match Config::new(&root_dir, product_name, &environment, &docker_registry) {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to create config: {}", e);
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let secrets_context = SecretsDefinitions::new(product_name.clone(), &format!("{}/secrets.yaml", config.product_path()));
    let product_path = std::path::PathBuf::from(config.product_path());
    let vault =  match config.vault_name() {
        ".env" => Arc::new(Mutex::new(DotenvVault::new(product_path))) as Arc<Mutex<dyn Vault + Send>>,
        "1Password" => Arc::new(Mutex::new(OnePassword::new())) as Arc<Mutex<dyn Vault + Send>>,
        _ => panic!("Invalid vault"),
    };


    let toolchain = Arc::new(ToolchainContext::new(Platform::default(), Platform::new(&target_os, &target_arch)));
    toolchain.setup_env();
    debug!("Toolchain set up");

    
    let mut reactor = match ContainerReactor::from_product_dir(config.clone(), toolchain.clone(), vault.clone()) {
        Ok(reactor) => reactor,
        Err(e) => {
            error!("Failed to create ContainerReactor: {}", e);
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };   

    let minikube = Minikube::new(toolchain.clone());     

    if let Some(matches) = matches.subcommand_matches("describe") {
        info!("Executing 'describe' subcommand");
        if matches.subcommand_matches("toolchain").is_some() {
            println!("{:#?}",toolchain);
            debug!("Described toolchain");
            std::process::exit(0);
        }

        if matches.subcommand_matches("images").is_some() {
            println!("{:#?}", reactor.images());
            debug!("Described images");
            std::process::exit(0);
        }

        if matches.subcommand_matches("services").is_some() {
            println!("{:#?}", reactor.services());
            debug!("Described services");
            std::process::exit(0);
        }



        if matches.subcommand_matches("build-script").is_some() {
            let component_name = matches.get_one::<String>("component_name").unwrap();
            info!("Describing build script for component: {}", component_name);
            let image = reactor.get_image(component_name).expect("Component not found");
            let secrets = vault.lock().unwrap().get(&product_name, &component_name, &environment).await.unwrap_or_default();
            let ctx = image.generate_build_context(secrets);

            println!("{}", image.build_script(&ctx).unwrap());
            debug!("Described build script for component: {}", component_name);
            std::process::exit(0);
        }

        if matches.subcommand_matches("build-context").is_some() {
            let component_name = matches.get_one::<String>("component_name").unwrap();
            info!("Describing build context for component: {}", component_name);
            let image = reactor.get_image(component_name).expect("Component not found");
            let secrets = vault.lock().unwrap().get(&product_name, &component_name, &environment).await.unwrap_or_default();
            let ctx = image.generate_build_context(secrets);
            println!("{:#?}", ctx);
            debug!("Described build context for component: {}", component_name);
            std::process::exit(0);
        }

        if matches.subcommand_matches("artefacts").is_some() {
            let _pop_dir = Directory::chdir(reactor.product_directory());
            let component_name = matches.get_one::<String>("component_name").unwrap();
            info!("Describing artefacts for component: {}", component_name);
            let image = reactor.get_image(component_name).expect("Component not found");
            let secrets = vault.lock().unwrap().get(&product_name, &component_name, &environment).await.unwrap_or_default();
            let ctx = image.generate_build_context(secrets);
            for (k,v) in image.spec().build_artefacts() {
                let message = format!("{} {}", "Artefact".green(), k.white());
                println!("{}\n",&message.bold());
                
                println!("{}\n", v.render(&ctx));
            }
            debug!("Described artefacts for component: {}", component_name);
            std::process::exit(0);
        }

        if matches.subcommand_matches("k8s").is_some() {
            info!("Describing Kubernetes manifests");
            let manifests = reactor.cluster_manifests();
            for component in manifests.components() {
                println!("{} -> {}", component.input_directory().display(), component.output_directory().display());                
                let spec = component.spec();
                let secrets = vault.lock().unwrap().get(&product_name, &component.name(), &environment).await.unwrap_or_default();
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


    if let Some(matches) = matches.subcommand_matches("minikube") {
        info!("Executing 'minikube' subcommand");
        if matches.subcommand_matches("start").is_some() {
            info!("Starting Minikube");
            match minikube.start().await {
                Ok(_) => {
                    info!("Minikube started successfully");
                    return Ok(());
                },
                Err(e) => {
                    error!("Failed to start Minikube: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }
        if matches.subcommand_matches("stop").is_some() {
            info!("Stopping Minikube");
            match minikube.stop().await {
                Ok(_) => {
                    info!("Minikube stopped successfully");
                    return Ok(());
                },
                Err(e) => {
                    error!("Failed to stop Minikube: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }        
        if matches.subcommand_matches("delete").is_some() {
            info!("Deleting Minikube");
            match minikube.delete().await {
                Ok(_) => {
                    info!("Minikube deleted successfully");
                    return Ok(());
                },
                Err(e) => {
                    error!("Failed to delete Minikube: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }               
    }

    if matches.subcommand_matches("dev").is_some() {        
        info!("Launching development environment");
        match reactor.launch().await {
            Ok(_) => {
                info!("Development environment launched successfully");
                return Ok(());
            },
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
            },
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
            },
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

    match reactor.select_kubernetes_context(config.kube_context()).await {
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
            },
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
            },
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
            },
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
            },
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
            },
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
            },
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    }

    if let Some(matches) = matches.subcommand_matches("vault") {
        info!("Executing 'vault' subcommand");

        if matches.subcommand_matches("create").is_some() {
            info!("Creating vault");
            match vault.lock().unwrap().create_vault(product_name).await {
                Ok(_) => {
                    info!("Vault created successfully");
                    return Ok(());
                },
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
            info!("Adding: {}", secrets);
            let secrets: HashMap<String, String> = serde_json::from_str(secrets).expect("Invalid secrets format");

            info!("Adding secrets to vault");
            match vault.lock().unwrap().set(product_name, component_name, &environment, secrets).await {
                Ok(_) => {
                    info!("Secrets added successfully");
                    return Ok(());
                },
                Err(e) => {
                    error!("Failed to add secrets: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }

        if let Some(matches) = matches.subcommand_matches("remove") {
            let component_name = matches.get_one::<String>("component_name").unwrap();

            info!("Removing secrets from vault");
            
            match vault.lock().unwrap().remove(product_name, component_name, &environment).await {
                Ok(_) => {
                    info!("Secrets removed successfully");
                    return Ok(());
                },
                Err(e) => {
                    error!("Failed to remove secrets: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
            
        }
    }

    if let Some(matches) = matches.subcommand_matches("secrets") {
        info!("Executing 'secrets' subcommand");

        if matches.subcommand_matches("init").is_some() {
            match vault.lock().unwrap().create_vault(product_name).await {
                Ok(_) => (),
            Err(e) => {
                error!("Failed to create vault: {}", e);
                eprintln!("{}", e);
                std::process::exit(1);
                }
            }
            info!("Initializing secrets");
            match secrets_context.populate(vault.clone(), &environment).await {
                Ok(_) => {
                    info!("Secrets initialized successfully");
                    return Ok(());
                },
                Err(e) => {
                    error!("Failed to initialize secrets: {}", e);
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}

