use clap::{arg, value_parser, Arg, Command};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct CommandArgs {
    pub product_name: String,
    // Other fields as needed
}

#[derive(Debug, Clone)]
pub struct CommonCliArgs {
    pub product_name: String,
    pub environment: String,
    // Other fields as needed
}

#[derive(Debug, Clone)]
pub struct DeployArgs {
    pub product_name: String,
    // Other fields as needed
}

#[derive(Debug, Clone)]
pub enum DescribeCommand {
    Toolchain,
    Images,
    Services,
    BuildScript { component_name: String },
    BuildContext { component_name: String },
    Artefacts { component_name: String },
    K8s,
}

/// Parse command line arguments and return clap matches
pub fn parse_args() -> clap::ArgMatches {
    let version = env!("CARGO_PKG_VERSION");

    Command::new("rush")
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
            .arg(arg!(--redirect <COMPONENTS> ... "Disables component and redirects the ingress. Format: component@host:port").num_args(1..))
            .arg(arg!(--silence <COMPONENTS> ... "Silence output for specific components").num_args(1..))
            .arg(arg!(--output <TYPE> "Output type: stdout, files, or both").default_value("stdout"))
            .arg(arg!(--"output-dir" <DIR> "Directory for log files when using files or both output types").default_value("logs"))
            .arg(arg!(--"no-color" "Disable colored output"))
            .arg(arg!(--"no-timestamps" "Disable timestamps in file logs"))
            .arg(arg!(--"no-source-names" "Disable source names in logs"))
            .arg(arg!(--"no-buffering" "Disable output buffering"))
        )
        .subcommand(Command::new("build"))
        .subcommand(Command::new("push"))
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
        .get_matches()
}

/// Parse redirected components from command line arguments
pub fn parse_redirected_components(matches: &clap::ArgMatches) -> HashMap<String, (String, u16)> {
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

/// Parse silenced components from command line arguments
pub fn parse_silenced_components(matches: &clap::ArgMatches) -> Vec<String> {
    matches
        .subcommand_matches("dev")
        .and_then(|dev_matches| dev_matches.get_many::<String>("silence"))
        .map(|values| values.cloned().collect())
        .unwrap_or_default()
}
