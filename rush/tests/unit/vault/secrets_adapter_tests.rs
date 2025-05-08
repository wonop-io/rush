use crate::rush_cli::vault::{Base64SecretsEncoder, EncodeSecrets, NoopEncoder};
use std::collections::HashMap;

#[test]
fn test_noop_encoder_empty() {
    let encoder = NoopEncoder;
    let secrets = HashMap::new();
    
    let encoded = encoder.encode_secrets(secrets);
    assert_eq!(encoded.len(), 0);
}

#[test]
fn test_noop_encoder_with_data() {
    let encoder = NoopEncoder;
    let mut secrets = HashMap::new();
    secrets.insert("key1".to_string(), "value1".to_string());
    secrets.insert("key2".to_string(), "value2".to_string());
    secrets.insert("key3".to_string(), "value3".to_string());
    
    let encoded = encoder.encode_secrets(secrets.clone());
    
    // Noop encoder should return the same values
    assert_eq!(encoded.len(), 3);
    assert_eq!(encoded.get("key1"), Some(&"value1".to_string()));
    assert_eq!(encoded.get("key2"), Some(&"value2".to_string()));
    assert_eq!(encoded.get("key3"), Some(&"value3".to_string()));
    
    // Clone the secrets to verify it was not modified
    let mut cloned_secrets = secrets.clone();
    cloned_secrets.insert("key4".to_string(), "value4".to_string());
    
    // Original secrets should be unchanged
    assert_eq!(secrets.len(), 3);
    assert!(!secrets.contains_key("key4"));
}

#[test]
fn test_base64_encoder_basic() {
    let encoder = Base64SecretsEncoder;
    let mut secrets = HashMap::new();
    secrets.insert("key1".to_string(), "value1".to_string());
    secrets.insert("key2".to_string(), "value2".to_string());
    
    let encoded = encoder.encode_secrets(secrets);
    
    // Base64 encoder should base64 encode the values
    assert_eq!(encoded.len(), 2);
    assert_eq!(encoded.get("key1"), Some(&base64::encode("value1")));
    assert_eq!(encoded.get("key2"), Some(&base64::encode("value2")));
}

#[test]
fn test_base64_encoder_empty() {
    let encoder = Base64SecretsEncoder;
    let secrets = HashMap::new();
    
    let encoded = encoder.encode_secrets(secrets);
    assert_eq!(encoded.len(), 0);
}

#[test]
fn test_base64_encoder_empty_values() {
    let encoder = Base64SecretsEncoder;
    let mut secrets = HashMap::new();
    secrets.insert("empty".to_string(), "".to_string());
    
    let encoded = encoder.encode_secrets(secrets);
    
    // Empty string in base64 is an empty string
    assert_eq!(encoded.len(), 1);
    assert_eq!(encoded.get("empty"), Some(&"".to_string()));
}

#[test]
fn test_base64_encoder_multiline() {
    let encoder = Base64SecretsEncoder;
    let mut secrets = HashMap::new();
    secrets.insert("multiline".to_string(), "line1\nline2\nline3".to_string());
    
    let encoded = encoder.encode_secrets(secrets);
    
    assert_eq!(encoded.len(), 1);
    assert_eq!(encoded.get("multiline"), Some(&base64::encode("line1\nline2\nline3")));
}

#[test]
fn test_base64_encoder_binary_data() {
    let encoder = Base64SecretsEncoder;
    let mut secrets = HashMap::new();
    
    // Create some binary data (using a Vec<u8> converted to a string)
    let binary_data = vec![0, 1, 2, 3, 4, 255].into_iter()
        .map(|b| b as char)
        .collect::<String>();
    
    secrets.insert("binary".to_string(), binary_data.clone());
    
    let encoded = encoder.encode_secrets(secrets);
    
    assert_eq!(encoded.len(), 1);
    assert_eq!(encoded.get("binary"), Some(&base64::encode(binary_data)));
}

#[test]
fn test_base64_encoder_special_chars() {
    let encoder = Base64SecretsEncoder;
    let mut secrets = HashMap::new();
    secrets.insert("special".to_string(), "!@#$%^&*()".to_string());
    secrets.insert("unicode".to_string(), "äöüÄÖÜß".to_string());
    secrets.insert("emoji".to_string(), "😀🚀🌍".to_string());
    
    let encoded = encoder.encode_secrets(secrets);
    
    // Check that special characters are encoded correctly
    assert_eq!(encoded.len(), 3);
    assert_eq!(encoded.get("special"), Some(&base64::encode("!@#$%^&*()")));
    assert_eq!(encoded.get("unicode"), Some(&base64::encode("äöüÄÖÜß")));
    assert_eq!(encoded.get("emoji"), Some(&base64::encode("😀🚀🌍")));
}

#[test]
fn test_encoders_chaining() {
    // Test chaining encoders (should just be the same as base64)
    let noop = NoopEncoder;
    let base64_encoder = Base64SecretsEncoder;
    
    let mut secrets = HashMap::new();
    secrets.insert("key".to_string(), "value".to_string());
    
    // First apply noop, then base64
    let noop_result = noop.encode_secrets(secrets.clone());
    let chained_result = base64_encoder.encode_secrets(noop_result);
    
    // Should be the same as direct base64
    let direct_result = base64_encoder.encode_secrets(secrets);
    
    assert_eq!(chained_result.get("key"), direct_result.get("key"));
    assert_eq!(chained_result.get("key"), Some(&base64::encode("value")));
}

#[test]
fn test_base64_decoder_verification() {
    // This test verifies that base64 encoded values can be decoded back to original
    let encoder = Base64SecretsEncoder;
    
    let mut secrets = HashMap::new();
    secrets.insert("key1".to_string(), "complex value with spaces".to_string());
    secrets.insert("key2".to_string(), "special chars: !@#$%^&*()".to_string());
    
    let encoded = encoder.encode_secrets(secrets.clone());
    
    // Manually decode and verify
    for (key, original_value) in secrets.iter() {
        let encoded_value = encoded.get(key).unwrap();
        let decoded_bytes = base64::decode(encoded_value).unwrap();
        let decoded_string = String::from_utf8(decoded_bytes).unwrap();
        
        assert_eq!(&decoded_string, original_value);
    }
}