use crate::rush_cli::vault::{Base64SecretsEncoder, EncodeSecrets, NoopEncoder};
use std::collections::HashMap;

#[test]
fn test_noop_encoder() {
    let encoder = NoopEncoder;
    let mut secrets = HashMap::new();
    secrets.insert("key1".to_string(), "value1".to_string());
    secrets.insert("key2".to_string(), "value2".to_string());
    
    let encoded = encoder.encode_secrets(secrets.clone());
    
    // Noop encoder should return the same values
    assert_eq!(encoded.len(), 2);
    assert_eq!(encoded.get("key1"), Some(&"value1".to_string()));
    assert_eq!(encoded.get("key2"), Some(&"value2".to_string()));
}

#[test]
fn test_base64_encoder() {
    let encoder = Base64SecretsEncoder;
    let mut secrets = HashMap::new();
    secrets.insert("key1".to_string(), "value1".to_string());
    secrets.insert("key2".to_string(), "value2".to_string());
    
    let encoded = encoder.encode_secrets(secrets.clone());
    
    // Base64 encoder should base64 encode the values
    assert_eq!(encoded.len(), 2);
    assert_eq!(encoded.get("key1"), Some(&base64::encode("value1")));
    assert_eq!(encoded.get("key2"), Some(&base64::encode("value2")));
    
    // Manually check base64 encoding
    assert_eq!(encoded.get("key1"), Some(&"dmFsdWUx".to_string()));
    assert_eq!(encoded.get("key2"), Some(&"dmFsdWUy".to_string()));
}

#[test]
fn test_base64_encoder_empty_values() {
    let encoder = Base64SecretsEncoder;
    let mut secrets = HashMap::new();
    secrets.insert("empty".to_string(), "".to_string());
    
    let encoded = encoder.encode_secrets(secrets.clone());
    
    // Empty string in base64 is an empty string
    assert_eq!(encoded.len(), 1);
    assert_eq!(encoded.get("empty"), Some(&"".to_string()));
}

#[test]
fn test_base64_encoder_special_chars() {
    let encoder = Base64SecretsEncoder;
    let mut secrets = HashMap::new();
    secrets.insert("special".to_string(), "!@#$%^&*()".to_string());
    secrets.insert("unicode".to_string(), "äöüÄÖÜß".to_string());
    
    let encoded = encoder.encode_secrets(secrets.clone());
    
    // Check that special characters are encoded correctly
    assert_eq!(encoded.len(), 2);
    assert_eq!(encoded.get("special"), Some(&base64::encode("!@#$%^&*()"))); 
    assert_eq!(encoded.get("unicode"), Some(&base64::encode("äöüÄÖÜß")));

    // Manual check for special characters
    assert_eq!(encoded.get("special"), Some(&"IUAjJCVeJiooKQ==".to_string()));
}