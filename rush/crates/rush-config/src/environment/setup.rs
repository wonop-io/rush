use log::{debug, info, trace, warn};
use std::env;
use std::path::Path;

/// Sets up the environment for Rush CLI
///
/// This function configures environment variables and paths needed
/// for Rush CLI to operate correctly across different platforms.
pub fn setup_environment() {
    trace!("Setting up environment");

    // Set the RUSHD_ROOT environment variable
    let binding = env::current_dir().unwrap();
    let rushd_root = binding
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .expect("Unable to find git repository amongst ancestors");
    env::set_var("RUSHD_ROOT", rushd_root);
    debug!("RUSHD_ROOT set to: {:?}", rushd_root);

    // Set the HOME environment variable if not already set
    if env::var("HOME").is_err() {
        if let Some(home) = env::var_os("USERPROFILE") {
            env::set_var("HOME", home);
            debug!("HOME environment variable set from USERPROFILE");
        } else {
            warn!("The HOME environment variable is not set.");
        }
    }

    // Set the PATH environment variable
    let home_dir = env::var_os("HOME").unwrap();
    let cargo_bin = Path::new(&home_dir).join(".cargo/bin");

    // Only add cargo_bin to PATH if it exists
    if cargo_bin.exists() {
        if let Some(current_path) = env::var_os("PATH") {
            let mut paths = env::split_paths(&current_path).collect::<Vec<_>>();
            // Only add if not already in PATH
            if !paths.contains(&cargo_bin) {
                paths.push(cargo_bin);
                if let Ok(new_path) = env::join_paths(paths) {
                    env::set_var("PATH", new_path);
                    debug!("Added cargo bin to PATH");
                }
            }
        }
    }

    trace!("Environment setup complete");
}

/// Loads environment-specific variables
///
/// # Arguments
///
/// * `environment` - The environment name (e.g., "local", "dev", "prod")
///
/// # Returns
///
/// A Result indicating success or error
pub fn load_environment_variables(environment: &str) -> Result<(), String> {
    trace!("Loading environment variables for: {}", environment);

    // Set environment-specific variables
    let variable_names = [
        "CTX",
        "VAULT",
        "DOMAIN",
        "K8S_ENCODER",
        "K8S_VALIDATOR",
        "K8S_VERSION",
    ];

    for var_name in &variable_names {
        let env_var_name = format!("{}_{}", var_name, environment.to_uppercase());
        let fallback_var_name = format!("{}_CTX", environment.to_uppercase());

        if env::var(&env_var_name).is_err() {
            if let Ok(value) = env::var(&fallback_var_name) {
                debug!("Using fallback value for {}: {}", env_var_name, value);
                env::set_var(&env_var_name, value);
            } else {
                let msg = format!("{env_var_name} environment variable not found");
                warn!("{}", msg);
                return Err(msg);
            }
        } else {
            debug!("Found environment variable: {}", env_var_name);
        }
    }

    // Check for infrastructure repository
    if env::var("INFRASTRUCTURE_REPOSITORY").is_err() {
        let msg = "INFRASTRUCTURE_REPOSITORY environment variable not found";
        warn!("{}", msg);
        return Err(msg.to_string());
    }

    info!("Environment variables loaded for: {}", environment);
    Ok(())
}
