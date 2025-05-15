//! Secret encoding utilities for Kubernetes and other systems
//!
//! This module provides implementations for encoding secrets in formats
//! required by different systems, such as base64 encoding for Kubernetes.

use std::collections::HashMap;

/// Trait for secret encoders that transform secret values for storage or transmission
pub trait SecretsEncoder {
    /// Encodes a map of secrets according to the encoder's strategy
    fn encode_secrets(&self, secrets: HashMap<String, String>) -> HashMap<String, String>;
}

/// An encoder that leaves secrets unmodified
pub struct NoopEncoder;

impl SecretsEncoder for NoopEncoder {
    fn encode_secrets(&self, secrets: HashMap<String, String>) -> HashMap<String, String> {
        secrets
    }
}

/// An encoder that encodes secret values as base64 for Kubernetes
pub struct Base64SecretsEncoder;

impl SecretsEncoder for Base64SecretsEncoder {
    fn encode_secrets(&self, secrets: HashMap<String, String>) -> HashMap<String, String> {
        secrets
            .into_iter()
            .map(|(key, value)| (key, base64::encode(value)))
            .collect()
    }
}

/// An encoder that encrypts secrets with a provided key
pub struct EncryptedSecretsEncoder {
    /// The encryption key
    encryption_key: Vec<u8>,
}

impl EncryptedSecretsEncoder {
    /// Creates a new EncryptedSecretsEncoder with the given key
    pub fn new(encryption_key: Vec<u8>) -> Self {
        Self { encryption_key }
    }
}

impl SecretsEncoder for EncryptedSecretsEncoder {
    fn encode_secrets(&self, secrets: HashMap<String, String>) -> HashMap<String, String> {
        // In a real implementation, this would use the encryption_key to encrypt each value
        // For now, we'll just use a placeholder implementation that prefixes values
        secrets
            .into_iter()
            .map(|(key, value)| {
                let encrypted = format!("ENCRYPTED:{}", value);
                (key, encrypted)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_encoder() {
        let encoder = NoopEncoder;
        let mut secrets = HashMap::new();
        secrets.insert("API_KEY".to_string(), "secret_value".to_string());
        secrets.insert("PASSWORD".to_string(), "p@ssw0rd".to_string());

        let encoded = encoder.encode_secrets(secrets.clone());

        assert_eq!(encoded.len(), 2);
        assert_eq!(encoded.get("API_KEY"), Some(&"secret_value".to_string()));
        assert_eq!(encoded.get("PASSWORD"), Some(&"p@ssw0rd".to_string()));
    }

    #[test]
    fn test_base64_encoder() {
        let encoder = Base64SecretsEncoder;
        let mut secrets = HashMap::new();
        secrets.insert("API_KEY".to_string(), "secret_value".to_string());

        let encoded = encoder.encode_secrets(secrets);

        assert_eq!(encoded.len(), 1);
        assert_eq!(
            encoded.get("API_KEY"),
            Some(&"c2VjcmV0X3ZhbHVl".to_string())
        );
    }

    #[test]
    fn test_encrypted_encoder() {
        let encoder = EncryptedSecretsEncoder::new(vec![1, 2, 3, 4]);
        let mut secrets = HashMap::new();
        secrets.insert("API_KEY".to_string(), "secret_value".to_string());

        let encoded = encoder.encode_secrets(secrets);

        assert_eq!(encoded.len(), 1);
        assert_eq!(
            encoded.get("API_KEY"),
            Some(&"ENCRYPTED:secret_value".to_string())
        );
    }
}
