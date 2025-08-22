use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use log::{debug, error, info, trace};
use rush_output::simple::Sink as OutputSink;
use tokio::sync::Mutex as TokioMutex;

use rush_config::Config;
use rush_container::{ContainerReactor, DevEnvironment, DockerCliClient};
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
        let _git_hash = match self
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
        let docker_client = Arc::new(DockerCliClient::new(self.toolchain.docker().to_string()));

        // Create the container reactor from product directory
        let reactor = ContainerReactor::from_product_dir(
            self.config.clone(),
            vault_adapter,
            secrets_encoder,
            self.redirect_components.clone(),
            self.silence_components.clone(),
        )
        .map_err(|e| Error::Setup(format!("Failed to initialize container reactor: {e}")))?;
        
        // Create development environment with both local services and reactor
        let data_dir = self.config.product_path().join("target").join("local-services");
        let mut dev_env = DevEnvironment::new(
            reactor,
            docker_client,
            self.config.network_name().to_string(),
            data_dir,
        );
        
        // Get component specs from the product directory to register local services
        let stack_spec_path = self.config.product_path().join("stack.spec.yaml");
        if stack_spec_path.exists() {
            // Load component specs and register local services
            let spec_content = std::fs::read_to_string(&stack_spec_path)
                .map_err(|e| Error::Setup(format!("Failed to read stack spec: {e}")))?;
            let yaml: serde_yaml::Value = serde_yaml::from_str(&spec_content)
                .map_err(|e| Error::Setup(format!("Failed to parse stack spec: {e}")))?;
            
            // Parse component specs
            let mut component_specs = Vec::new();
            let variables = rush_build::Variables::empty();
            if let Some(components) = yaml.as_mapping() {
                for (name, spec_yaml) in components {
                    if let (Some(_name_str), Some(spec_obj)) = (name.as_str(), spec_yaml.as_mapping()) {
                        // Create a minimal ComponentBuildSpec for local services
                        if let Some(build_type) = spec_obj.get(&serde_yaml::Value::String("build_type".to_string())) {
                            if let Some(build_type_str) = build_type.as_str() {
                                if build_type_str == "LocalService" {
                                    // Parse the component spec
                                    let spec = rush_build::ComponentBuildSpec::from_yaml(
                                        self.config.clone(),
                                        variables.clone(),
                                        spec_yaml,
                                    );
                                    component_specs.push(spec);
                                }
                            }
                        }
                    }
                }
            }
            
            // Register local services
            dev_env.register_local_services(&component_specs)
                .map_err(|e| Error::Setup(format!("Failed to register local services: {e}")))?;
        }

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
        let result = dev_env.start().await;
        
        // Always try to stop the dev environment after it completes
        // This is crucial for cleanup
        info!("Stopping development environment and local services...");
        if let Err(e) = dev_env.stop().await {
            error!("Error during development environment stop: {}", e);
        }
        
        // Return the original result
        match result {
            Ok(_) => {
                info!("Development environment completed successfully");
                Ok(())
            }
            Err(e) => {
                error!("Development environment failed: {}", e);
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
