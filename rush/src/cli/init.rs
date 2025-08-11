use crate::core::config::{apply_rushd_config, RushdConfig};
use crate::core::environment::setup_environment;
use crate::error::Result;
use crate::utils::{check_version, find_project_root, Directory};
use log::{debug, trace};
use std::env;
use std::fs::File;
use std::io::Read;

pub async fn init_application() -> Result<()> {
    // Check for newer version
    check_version().await;

    // Setup the environment
    setup_environment();

    // Find and set the project root
    let current_dir = env::current_dir().map_err(|e| {
        crate::error::Error::Filesystem(format!("Failed to get current directory: {e}"))
    })?;

    let root_path = find_project_root(current_dir).ok_or_else(|| {
        crate::error::Error::Filesystem("Failed to find project root".to_string())
    })?;

    debug!("Found project root at: {}", root_path.display());
    env::set_var("RUSHD_ROOT", &root_path);

    // Change to root directory
    let _guard = Directory::chpath(&root_path);
    debug!("Changed directory to RUSHD_ROOT: {}", root_path.display());

    // Load rushd.yaml configuration if it exists
    let rushd_config_path = root_path.join("rushd.yaml");
    if rushd_config_path.exists() {
        debug!("Loading rushd.yaml from {}", rushd_config_path.display());
        let mut file = File::open(&rushd_config_path).map_err(|e| {
            crate::error::Error::Filesystem(format!("Failed to open rushd.yaml: {e}"))
        })?;

        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| {
            crate::error::Error::Filesystem(format!("Failed to read rushd.yaml: {e}"))
        })?;

        let rushd_config: RushdConfig = serde_yaml::from_str(&contents).map_err(|e| {
            crate::error::Error::Config(format!("Failed to parse rushd.yaml: {e}"))
        })?;

        apply_rushd_config(&rushd_config);
        debug!("Applied configuration from rushd.yaml");
    } else {
        debug!("No rushd.yaml found at {}", rushd_config_path.display());
    }

    // Load .env file if it exists
    dotenv::dotenv().ok();

    trace!("Application initialization complete");
    Ok(())
}
