#!/bin/bash
set -e

echo "Setting up all crate lib.rs files..."

# rush-config lib.rs
cat > crates/rush-config/src/lib.rs << 'EOF'
//! Rush Config - Configuration management

pub mod config;
pub mod dotenv;
pub mod environment;
pub mod product;
pub mod types;

pub use config::{Config, ConfigLoader};
pub use environment::Environment;
pub use product::Product;
EOF

# rush-security lib.rs  
cat > crates/rush-security/src/lib.rs << 'EOF'
//! Rush Security - Security and secrets management

pub mod env_defs;
pub mod secrets;
pub mod vault;

pub use secrets::{SecretsEncoder, SecretsProvider};
pub use vault::{Vault, FileVault, DotenvVault};

// Re-export common types
pub use secrets::definitions::*;
pub use secrets::encoder::{Base64SecretsEncoder, NoopEncoder};
EOF

# rush-build lib.rs
cat > crates/rush-build/src/lib.rs << 'EOF'
//! Rush Build - Build system and artifact generation

pub mod artefact;
pub mod script;
pub mod spec;
pub mod template;
pub mod types;

pub use artefact::Artefact;
pub use script::BuildScript;
pub use spec::{BuildType, ComponentBuildSpec, Variables};
pub use types::BuildContext;
EOF

# rush-output lib.rs
cat > crates/rush-output/src/lib.rs << 'EOF'
//! Rush Output - Terminal output and logging

pub mod director;
pub mod interactive;
pub mod json;
pub mod plain;
pub mod stream;

pub use director::{OutputDirector, OutputDirectorConfig, OutputDirectorFactory};
pub use stream::{OutputStream, OutputSource};

#[cfg(feature = "interactive")]
pub use interactive::InteractiveOutputDirector;

pub type SharedOutputDirector = std::sync::Arc<Box<dyn OutputDirector>>;
EOF

# rush-container lib.rs
cat > crates/rush-container/src/lib.rs << 'EOF'
//! Rush Container - Docker and container orchestration

pub mod build;
pub mod docker;
pub mod image_builder;
pub mod lifecycle;
pub mod network;
pub mod reactor;
pub mod watcher;

pub use docker::{DockerClient, DockerCliClient, DockerService};
pub use image_builder::ImageBuilder;
pub use reactor::{ContainerReactor, ContainerReactorConfig};

// Re-export build types
pub use build::processor::BuildProcessor;

pub type ContainerService = docker::ContainerService;
pub type ServiceCollection = std::collections::HashMap<String, Vec<std::sync::Arc<ContainerService>>>;
EOF

# rush-k8s lib.rs
cat > crates/rush-k8s/src/lib.rs << 'EOF'
//! Rush K8s - Kubernetes deployment and management

pub mod context;
pub mod deployment;
pub mod infrastructure;
pub mod manifests;
pub mod types;
pub mod validation;

pub use context::K8sContext;
pub use deployment::K8sDeployment;
pub use manifests::ManifestGenerator;
pub use validation::K8sValidator;
EOF

# rush-cli lib.rs
cat > crates/rush-cli/src/lib.rs << 'EOF'
//! Rush CLI - Command-line interface

pub mod cli;
pub mod commands;
pub mod context_builder;
pub mod execute;

pub use cli::Cli;
pub use context_builder::ContextBuilder;
pub use execute::execute_command;
EOF

echo "Creating module structure for rush-security/vault..."
mkdir -p crates/rush-security/src/vault
mkdir -p crates/rush-security/src/secrets

# Move vault files if they exist
if [ -d "src/security/vault" ]; then
    cp -r src/security/vault/* crates/rush-security/src/vault/ 2>/dev/null || true
fi

if [ -d "src/security/secrets" ]; then
    cp -r src/security/secrets/* crates/rush-security/src/secrets/ 2>/dev/null || true
fi

# Create mod.rs files
echo "pub mod dotenv;" > crates/rush-security/src/vault/mod.rs
echo "pub mod file;" >> crates/rush-security/src/vault/mod.rs
echo "" >> crates/rush-security/src/vault/mod.rs
echo "pub use dotenv::DotenvVault;" >> crates/rush-security/src/vault/mod.rs
echo "pub use file::FileVault;" >> crates/rush-security/src/vault/mod.rs
echo "" >> crates/rush-security/src/vault/mod.rs
echo "use async_trait::async_trait;" >> crates/rush-security/src/vault/mod.rs
echo "use std::collections::HashMap;" >> crates/rush-security/src/vault/mod.rs
echo "" >> crates/rush-security/src/vault/mod.rs
echo "#[async_trait]" >> crates/rush-security/src/vault/mod.rs
echo "pub trait Vault: Send + Sync {" >> crates/rush-security/src/vault/mod.rs
echo "    async fn get(&self, product: &str, component: &str, environment: &str) -> Result<HashMap<String, String>, anyhow::Error>;" >> crates/rush-security/src/vault/mod.rs
echo "    async fn set(&mut self, product: &str, component: &str, environment: &str, secrets: HashMap<String, String>) -> Result<(), anyhow::Error>;" >> crates/rush-security/src/vault/mod.rs
echo "}" >> crates/rush-security/src/vault/mod.rs

echo "pub mod adapter;" > crates/rush-security/src/secrets/mod.rs
echo "pub mod definitions;" >> crates/rush-security/src/secrets/mod.rs
echo "pub mod encoder;" >> crates/rush-security/src/secrets/mod.rs
echo "" >> crates/rush-security/src/secrets/mod.rs
echo "pub use definitions::*;" >> crates/rush-security/src/secrets/mod.rs
echo "pub use encoder::{SecretsEncoder, Base64SecretsEncoder, NoopEncoder};" >> crates/rush-security/src/secrets/mod.rs
echo "" >> crates/rush-security/src/secrets/mod.rs
echo "use async_trait::async_trait;" >> crates/rush-security/src/secrets/mod.rs
echo "use std::collections::HashMap;" >> crates/rush-security/src/secrets/mod.rs
echo "" >> crates/rush-security/src/secrets/mod.rs
echo "#[async_trait]" >> crates/rush-security/src/secrets/mod.rs
echo "pub trait SecretsProvider: Send + Sync {" >> crates/rush-security/src/secrets/mod.rs
echo "    async fn get_secrets(&self, context: &str) -> Result<HashMap<String, String>, anyhow::Error>;" >> crates/rush-security/src/secrets/mod.rs
echo "}" >> crates/rush-security/src/secrets/mod.rs

echo "All lib.rs files created!"