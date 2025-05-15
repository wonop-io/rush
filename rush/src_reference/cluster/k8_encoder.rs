use log::{info, trace};
use std::fs;
use std::process::Command;

pub trait K8Encoder {
    fn encode_file(&self, path: &str) -> Result<(), String>;
}

// Implementation of the K8Encoder trait
pub struct SealedSecretsEncoder;

impl K8Encoder for SealedSecretsEncoder {
    fn encode_file(&self, path: &str) -> Result<(), String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
        trace!("Testing {} if it contains 'kind: Secret'", path);

        // Make the search for data more robust
        let contains_kind_secret = content.lines().any(|line| line.trim() == "kind: Secret");
        let contains_data = content.lines().any(|line| line.trim() == "data:");

        if !contains_kind_secret || !contains_data {
            trace!("File does not contain 'kind: Secret' or has no data, skipping encoding");
            return Ok(());
        }

        let temp_file = format!("{}.tmp.yaml", path);
        trace!("Encoding file {}", path);

        // Run kubeseal command
        // TODO: Add certitiicate path as an argument
        let output = Command::new("kubeseal")
            .arg("--format")
            .arg("yaml")
            .arg("-w")
            .arg(&temp_file)
            .arg("-f")
            .arg(path)
            .output()
            .map_err(|e| format!("Failed to execute kubeseal: {}", e))?;

        if !output.status.success() {
            info!("File attempted to be encoded: {}", path);
            return Err(format!(
                "kubeseal failed with status: {}\nstderr: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Replace original file with encoded file
        fs::rename(&temp_file, path).map_err(|e| format!("Failed to rename file: {}", e))?;

        Ok(())
    }
}

// NoopEncoder implementation of the K8Encoder trait
pub struct NoopEncoder;

impl K8Encoder for NoopEncoder {
    fn encode_file(&self, _path: &str) -> Result<(), String> {
        // No operation performed
        Ok(())
    }
}
