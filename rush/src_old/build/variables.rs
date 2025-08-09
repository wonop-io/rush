use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct VariablesFile {
    pub dev: HashMap<String, String>,
    pub staging: HashMap<String, String>,
    pub prod: HashMap<String, String>,
    pub local: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Variables {
    pub values: VariablesFile,
    pub env: String,
}

impl Variables {
    pub fn new(path: &str, env: &str) -> Arc<Self> {
        let contents = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => {
                return Arc::new(Variables {
                    values: VariablesFile {
                        dev: HashMap::new(),
                        staging: HashMap::new(),
                        prod: HashMap::new(),
                        local: HashMap::new(),
                    },
                    env: env.to_lowercase(),
                })
            }
        };

        let variables =
            serde_yaml::from_str(&contents).expect("Could not parse variables YAML file");

        Arc::new(Variables {
            values: variables,
            env: env.to_lowercase(),
        })
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match self.env.as_str() {
            "dev" => self.values.dev.get(key).cloned(),
            "staging" => self.values.staging.get(key).cloned(),
            "prod" => self.values.prod.get(key).cloned(),
            "local" => self.values.local.get(key).cloned(),
            _ => None,
        }
    }
}
