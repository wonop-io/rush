use rush_config::Config;
use rush_container::ContainerReactor;
use rush_output::simple::Sink;
use rush_security::{SecretsDefinitions, Vault};
use rush_toolchain::ToolchainContext;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

pub struct CliContext {
    pub config: Arc<Config>,
    pub environment: String,
    pub product_name: String,
    pub toolchain: Arc<ToolchainContext>,
    pub reactor: ContainerReactor,
    pub vault: Arc<Mutex<dyn Vault + Send>>,
    pub secrets_context: SecretsDefinitions,
    pub output_sink: Arc<TokioMutex<Box<dyn Sink>>>,
}

impl CliContext {
    pub fn new(
        config: Arc<Config>,
        environment: String,
        product_name: String,
        toolchain: Arc<ToolchainContext>,
        reactor: ContainerReactor,
        vault: Arc<Mutex<dyn Vault + Send>>,
        secrets_context: SecretsDefinitions,
        output_sink: Arc<TokioMutex<Box<dyn Sink>>>,
    ) -> Self {
        Self {
            config,
            environment,
            product_name,
            toolchain,
            reactor,
            vault,
            secrets_context,
            output_sink,
        }
    }
}
