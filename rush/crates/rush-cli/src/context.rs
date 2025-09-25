use std::sync::{Arc, Mutex};

use rush_config::Config;
use rush_container::Reactor;
use rush_local_services::LocalServiceManager;
use rush_output::simple::Sink;
use rush_security::{SecretsDefinitions, Vault};
use rush_toolchain::ToolchainContext;
use tokio::sync::Mutex as TokioMutex;

pub struct CliContext {
    pub config: Arc<Config>,
    pub environment: String,
    pub product_name: String,
    pub toolchain: Arc<ToolchainContext>,
    pub reactor: Reactor,
    pub vault: Arc<Mutex<dyn Vault + Send>>,
    pub secrets_context: SecretsDefinitions,
    pub output_sink: Arc<TokioMutex<Box<dyn Sink>>>,
    pub local_services: Option<LocalServiceManager>,
}

impl CliContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Arc<Config>,
        environment: String,
        product_name: String,
        toolchain: Arc<ToolchainContext>,
        reactor: Reactor,
        vault: Arc<Mutex<dyn Vault + Send>>,
        secrets_context: SecretsDefinitions,
        output_sink: Arc<TokioMutex<Box<dyn Sink>>>,
        local_services: Option<LocalServiceManager>,
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
            local_services,
        }
    }

    /// Stop local services if they are running
    pub async fn stop_local_services(&mut self) {
        if let Some(ref mut local_services) = self.local_services {
            log::info!("Stopping local services from context...");
            if let Err(e) = local_services.stop_all().await {
                log::error!("Failed to stop local services: {e}");
            }
        }
    }
}
