//! Kubernetes integration module
//!
//! This module provides functionality for working with Kubernetes resources,
//! including generating manifests, validating configurations, and deploying services.

mod context;
mod deployment;
mod encoder;
mod infrastructure;
mod manifests;
mod validation;

pub use context::{
    create_context_manager, create_context_manager_with_config, ContextManager, KubernetesContext,
};
pub use deployment::{Manifest, ManifestCollection};
pub use encoder::{create_encoder, K8sEncoder, NoopEncoder, SealedSecretsEncoder};
pub use infrastructure::InfrastructureRepo;
pub use manifests::ManifestCollection as K8sManifestCollection;
pub use validation::{K8sValidator, KubeconformValidator, KubevalValidator};
