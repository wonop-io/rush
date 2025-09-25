use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use log::{debug, error, trace};

/// Loads environment variables from a .env file
///
/// # Arguments
///
/// * `path` - Path to the .env file to load
///
/// # Returns
///
/// A HashMap of environment variable names to values, or an error
pub fn load_dotenv(path: &Path) -> Result<HashMap<String, String>, std::io::Error> {
    trace!("Loading dotenv file: {}", path.display());
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut env_map = HashMap::new();
    let mut lines_iter = reader.lines().peekable();

    while let Some(Ok(line)) = lines_iter.next() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split the line into key and value
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value_str = value.trim().to_string();

            // Handle quoted values
            if value_str.starts_with('"') && !value_str.ends_with('"') {
                // This is a multi-line value
                let mut full_value = value_str[1..].to_string();

                // Keep reading lines until we find the closing quote
                while let Some(Ok(next_line)) = lines_iter.next() {
                    full_value.push('\n');
                    full_value.push_str(&next_line);
                    if next_line.trim().ends_with('"') {
                        break;
                    }
                }

                // Remove the last quote
                if full_value.ends_with('"') {
                    full_value.pop();
                }

                env_map.insert(key, full_value);
            } else if value_str.starts_with('"') && value_str.ends_with('"') {
                // Single-line quoted value
                env_map.insert(key, value_str[1..value_str.len() - 1].to_string());
            } else {
                // Unquoted value
                env_map.insert(key, value_str);
            }
        }
    }

    debug!(
        "Loaded {} environment variables from {}",
        env_map.len(),
        path.display()
    );
    Ok(env_map)
}

/// Saves environment variables to a .env file
///
/// # Arguments
///
/// * `path` - Path to the .env file to write
/// * `env_map` - HashMap of environment variable names to values
///
/// # Returns
///
/// A Result indicating success or failure
pub fn save_dotenv(path: &Path, env_map: HashMap<String, String>) -> Result<(), std::io::Error> {
    trace!("Saving dotenv file: {}", path.display());
    let mut file = File::create(path)?;

    for (key, value) in &env_map {
        writeln!(file, "{key}=\"{value}\"")?;
    }

    debug!(
        "Saved {} environment variables to {}",
        env_map.len(),
        path.display()
    );
    Ok(())
}

/// Loads environment variables from a .env file and sets them in the process environment
///
/// # Arguments
///
/// * `path` - Path to the .env file to load
///
/// # Returns
///
/// A Result indicating success or failure
pub fn load_and_set_dotenv(path: &Path) -> Result<(), std::io::Error> {
    let env_map = load_dotenv(path)?;

    for (key, value) in env_map {
        trace!("Setting environment variable: {}={}", key, value);
        std::env::set_var(key, value);
    }

    Ok(())
}

/// Merges multiple .env files, with later files taking precedence
///
/// # Arguments
///
/// * `paths` - A slice of paths to .env files to merge
///
/// # Returns
///
/// A HashMap of environment variable names to values, or an error
pub fn merge_dotenv_files(paths: &[&Path]) -> Result<HashMap<String, String>, std::io::Error> {
    let mut merged_env = HashMap::new();

    for path in paths {
        if path.exists() {
            match load_dotenv(path) {
                Ok(env_map) => {
                    merged_env.extend(env_map);
                }
                Err(e) => {
                    error!("Error loading dotenv file {}: {}", path.display(), e);
                    return Err(e);
                }
            }
        }
    }

    Ok(merged_env)
}
