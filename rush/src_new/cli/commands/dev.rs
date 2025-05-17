use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use log::{debug, error, info, trace};

use crate::container::{ContainerReactor, ContainerReactorConfig, DockerCliClient};
use crate::core::config::Config;
use crate::error::{Error, Result};
use crate::security::{Base64SecretsEncoder, FileVault, SecretsEncoder, SecretsProvider};
use crate::toolchain::ToolchainContext;

/// Command to run the development environment
pub struct DevCommand {
    product_name: String,
    config: Arc<Config>,
    toolchain: Arc<ToolchainContext>,
    vault: Arc<dyn SecretsProvider>,
    redirect_components: HashMap<String, (String, u16)>,
    silence_components: Vec<String>,
}

impl DevCommand {
    pub fn new(
        product_name: String,
        config: Arc<Config>,
        toolchain: Arc<ToolchainContext>,
        vault: Arc<dyn SecretsProvider>,
        redirect_components: HashMap<String, (String, u16)>,
        silence_components: Vec<String>,
    ) -> Self {
        DevCommand {
            product_name,
            config,
            toolchain,
            vault,
            redirect_components,
            silence_components,
        }
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
                    "latest".to_string()
                } else {
                    hash[..8].to_string()
                }
            }
            Err(_) => "latest".to_string(),
        };

        // Create vault adapter
        let vault_adapter = Arc::new(Mutex::new(FileVault::new(
            PathBuf::from("/tmp/vault"),
            None,
        )));
        let secrets_encoder = Arc::new(Base64SecretsEncoder);

        // Create docker client
        let docker_client = Arc::new(DockerCliClient::new(self.toolchain.docker().to_string()));

        // Create the container reactor config
        let reactor_config = ContainerReactorConfig {
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
        .map_err(|e| Error::Setup(format!("Failed to initialize container reactor: {}", e)))?;

        debug!("Container reactor initialized successfully");

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
) -> Result<()> {
    let cmd = DevCommand::new(
        product_name,
        config,
        toolchain,
        vault,
        redirect_components,
        silence_components,
    );
    cmd.execute().await
}
