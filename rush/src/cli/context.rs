use super::*;

pub struct CliContext {
    pub config: Arc<Config>,
    pub environment: String,
    pub product_name: String,
    pub toolchain: Arc<ToolchainContext>,
    pub reactor: ContainerReactor,
    pub vault: Arc<Mutex<dyn Vault + Send>>,
    pub secrets_context: SecretsDefinitions,
}

pub async fn create_context(matches: &ArgMatches) -> Result<CliContext, std::io::Error> {
    let start_port = *matches.get_one::<u16>("start_port").unwrap();
    let redirected_components = parse_redirected_components(matches);
    let silence_components = parse_silence_components(matches);

    setup_logging(matches);

    let target_arch = get_target_arch(matches);
    let target_os = get_target_os(matches);
    let environment = get_environment(matches);
    let docker_registry = get_docker_registry(matches);
    let product_name = matches.get_one::<String>("product_name").unwrap().clone();

    info!("Product name: {}", product_name);

    let root_dir = std::env::var("RUSHD_ROOT").unwrap();
    let config = create_config(
        &root_dir,
        &product_name,
        &environment,
        &docker_registry,
        start_port,
    )?;

    // Create secrets and vault
    let (secrets_context, vault) = setup_secrets(&config, &product_name)?;

    // Setup environment files
    setup_environment_files(&config, &product_name, &environment)?;

    // Create toolchain
    let toolchain = create_toolchain(&target_os, &target_arch);

    // Create reactor
    let reactor = create_reactor(
        config.clone(),
        toolchain.clone(),
        vault.clone(),
        redirected_components,
        silence_components,
    )?;

    Ok(CliContext {
        config,
        environment,
        product_name,
        toolchain,
        reactor,
        vault,
        secrets_context,
    })
}
