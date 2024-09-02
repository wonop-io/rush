use std::collections::HashMap;
use base64;

pub trait EncodeSecrets {
    fn encode_secrets(&self, secrets: HashMap<String, String>) -> HashMap<String, String>;
}


pub struct NoopEncoder;

impl EncodeSecrets for NoopEncoder {
    fn encode_secrets(&self, secrets: HashMap<String, String>) -> HashMap<String, String> {
        secrets
    }
}

pub struct Base64SecretsEncoder;

impl EncodeSecrets for Base64SecretsEncoder {
    fn encode_secrets(&self, secrets: HashMap<String, String>) -> HashMap<String, String> {
        secrets.into_iter()
            .map(|(key, value)| (key, base64::encode(value)))
            .collect()
    }
}
