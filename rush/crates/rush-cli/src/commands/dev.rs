use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use log::{debug, error, info, trace};
use rush_output::simple::Sink as OutputSink;
use tokio::sync::Mutex as TokioMutex;

use rush_config::Config;
use rush_container::{ContainerReactor, ContainerReactorConfig, DockerCliClient};
use rush_core::constants::DOCKER_TAG_LATEST;
use rush_core::error::{Error, Result};
// Legacy imports removed - this module is deprecated
// use rush_output::OutputDirectorFactory;
// use rush_output::factory::OutputDirectorConfig;
use rush_output::factory::OutputDirectorConfig;
use rush_security::{FileVault, NoopEncoder, SecretsProvider};
use rush_toolchain::ToolchainContext;

/// Command to run the development environment
pub struct DevCommand {
    product_name: String,
    config: Arc<Config>,
    toolchain: Arc<ToolchainContext>,
    redirect_components: HashMap<String, (String, u16)>,
    silence_components: Vec<String>,
    _output_config: OutputDirectorConfig,
    output_sink: Option<Arc<TokioMutex<Box<dyn OutputSink>>>>,
}

impl DevCommand {
    pub fn new(
        product_name: String,
        config: Arc<Config>,
        toolchain: Arc<ToolchainContext>,
        _vault: Arc<dyn SecretsProvider>,
        redirect_components: HashMap<String, (String, u16)>,
        silence_components: Vec<String>,
        _output_config: OutputDirectorConfig,
    ) -> Self {
        DevCommand {
            product_name,
            config,
            toolchain,
            redirect_components,
            silence_components,
            _output_config,
            output_sink: None,
        }
    }

    /// Set the output sink for this command
    pub fn with_output_sink(mut self, sink: Arc<TokioMutex<Box<dyn OutputSink>>>) -> Self {
        self.output_sink = Some(sink);
        self
    }

    pub async fn execute(&self) -> Result<()> {
        trace!("Starting development environment for {}", self.product_name);

        // Get the git hash for tagging
        let git_hash = match self
            .toolchain
            .get_git_folder_hash(&self.config.product_path().display().to_string())
        {
            Ok(hash) => {
                if hash.is_empty() {
                    DOCKER_TAG_LATEST.to_string()
                } else {
                    hash[..8].to_string()
                }
            }
            Err(_) => DOCKER_TAG_LATEST.to_string(),
        };

        // Create vault adapter
        let vault_adapter = Arc::new(Mutex::new(FileVault::new(
            PathBuf::from("/tmp/vault"),
            None,
        )));
        let secrets_encoder = Arc::new(NoopEncoder);

        // Create docker client
        let _docker_client = Arc::new(DockerCliClient::new(self.toolchain.docker().to_string()));

        // Create the container reactor config
        let _reactor_config = ContainerReactorConfig {
            product_name: self.product_name.clone(),
            product_dir: self.config.product_path().clone(),
            network_name: self.config.network_name().to_string(),
            environment: self.config.environment().to_string(),
            docker_registry: self.config.docker_registry().to_string(),
            redirected_components: self.redirect_components.clone(),
            silenced_components: self
                .silence_components
                .clone()
                .into_iter()
                .collect::<HashSet<_>>(),
            verbose: false,
            watch_config: Default::default(),
            git_hash,
            start_port: self.config.start_port(),
        };

        // Create the container reactor from product directory
        let mut reactor = ContainerReactor::from_product_dir(
            self.config.clone(),
            vault_adapter,
            secrets_encoder,
            self.redirect_components.clone(),
            self.silence_components.clone(),
        )
        .map_err(|e| Error::Setup(format!("Failed to initialize container reactor: {e}")))?;

        debug!("Container reactor initialized successfully");

        // Set the output sink if provided
        if let Some(ref sink) = self.output_sink {
            // Clone the sink and convert to the format expected by the reactor
            let sink_clone = sink.clone();
            tokio::spawn(async move {
                let sink_guard = sink_clone.lock().await;
                // Note: We need to clone the inner sink to pass it to the reactor
                // This is a limitation of the current design
                drop(sink_guard);
            });
            // For now, we'll log a message - the reactor will use its own sink
            debug!("Output sink configured for development environment");
        }

        debug!("Output system initialized");

        // Launch the development environment
        match reactor.launch().await {
            Ok(_) => {
                info!("Development environment launched successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to launch development environment: {}", e);
                Err(Error::LaunchFailed(e.to_string()))
            }
        }
    }
}

pub async fn execute(
    product_name: String,
    config: Arc<Config>,
    toolchain: Arc<ToolchainContext>,
    vault: Arc<dyn SecretsProvider>,
    redirect_components: HashMap<String, (String, u16)>,
    silence_components: Vec<String>,
    _output_config: OutputDirectorConfig,
) -> Result<()> {
    let cmd = DevCommand::new(
        product_name,
        config,
        toolchain,
        vault,
        redirect_components,
        silence_components,
        _output_config,
    );
    cmd.execute().await
}
