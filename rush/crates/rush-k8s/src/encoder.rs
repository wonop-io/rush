//! Kubernetes secret encoding
//!
//! This module provides functionality for encoding Kubernetes secrets to the
//! appropriate format for deployment in Kubernetes manifests.

use std::fs;
use std::process::Command;

use log::{info, trace, warn};
use rush_core::error::{Error, Result};

/// Trait defining operations for encoding Kubernetes secrets
pub trait K8sEncoder: Send + Sync {
    /// Encodes secrets in a Kubernetes manifest file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the manifest file
    fn encode_file(&self, path: &str) -> Result<()>;
}

/// Implementation that uses SealedSecrets for encoding
pub struct SealedSecretsEncoder;

impl K8sEncoder for SealedSecretsEncoder {
    fn encode_file(&self, path: &str) -> Result<()> {
        let content = fs::read_to_string(path)
            .map_err(|e| Error::Filesystem(format!("Failed to read manifest file: {e}")))?;
        trace!("Testing {path} if it contains 'kind: Secret'");

        // Check if this is a Secret resource that needs encoding
        let contains_kind_secret = content.lines().any(|line| line.trim() == "kind: Secret");
        let contains_data = content.lines().any(|line| line.trim().starts_with("data:"));

        if !contains_kind_secret || !contains_data {
            trace!("File does not contain 'kind: Secret' or has no data, skipping encoding");
            return Ok(());
        }

        // Convert 'data:' to 'stringData:' so kubeseal can handle plain text values
        // Kubernetes Secret 'data' requires base64-encoded values, but 'stringData' accepts plain text
        let preprocessed_content = content.replace("\ndata:", "\nstringData:");
        
        // Write preprocessed content to a temporary file
        let prep_file = format!("{path}.prep.yaml");
        fs::write(&prep_file, preprocessed_content)
            .map_err(|e| Error::Filesystem(format!("Failed to write preprocessed file: {e}")))?;
        
        trace!("Encoding file {path} (converted data -> stringData)");

        let temp_file = format!("{path}.tmp.yaml");

        // Run kubeseal command on the preprocessed file
        let output = Command::new("kubeseal")
            .arg("--format")
            .arg("yaml")
            .arg("-w")
            .arg(&temp_file)
            .arg("-f")
            .arg(&prep_file)
            .output()
            .map_err(|e| Error::External(format!("Failed to execute kubeseal: {e}")))?;

        // Clean up preprocessed file
        let _ = fs::remove_file(&prep_file);

        if !output.status.success() {
            info!("File attempted to be encoded: {path}");
            return Err(Error::External(format!(
                "kubeseal failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        // Replace original file with encoded file
        fs::rename(&temp_file, path)
            .map_err(|e| Error::Filesystem(format!("Failed to rename temporary file: {e}")))?;

        Ok(())
    }
}

/// No-operation encoder that doesn't modify files
pub struct NoopEncoder;

impl K8sEncoder for NoopEncoder {
    fn encode_file(&self, _path: &str) -> Result<()> {
        // No operation performed
        Ok(())
    }
}

/// Creates a K8sEncoder based on the specified type
///
/// # Arguments
///
/// * `encoder_type` - The type of encoder to create
///
/// # Returns
///
/// A boxed K8sEncoder implementation
pub fn create_encoder(encoder_type: &str) -> Box<dyn K8sEncoder> {
    match encoder_type {
        "kubeseal" => {
            info!("Using SealedSecrets for K8s secret encoding");
            Box::new(SealedSecretsEncoder)
        }
        "noop" => {
            warn!("Using no-op encoder - secrets will not be encrypted");
            Box::new(NoopEncoder)
        }
        _ => {
            warn!("Unknown encoder type '{encoder_type}', defaulting to no-op encoder");
            Box::new(NoopEncoder)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[test]
    fn test_noop_encoder() {
        let encoder = NoopEncoder;
        let result = encoder.encode_file("nonexistent.yaml");
        assert!(result.is_ok());
    }

    #[test]
    fn test_sealed_secrets_encoder_non_secret() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let content = "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: test-config\ndata:\n  key: value\n";
        temp_file.write_all(content.as_bytes()).unwrap();

        let encoder = SealedSecretsEncoder;
        let result = encoder.encode_file(temp_file.path().to_str().unwrap());

        assert!(result.is_ok());
        // Should not have modified the file since it's not a Secret
        let content_after = fs::read_to_string(temp_file.path()).unwrap();
        assert_eq!(content, content_after);
    }
}
