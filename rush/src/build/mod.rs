//! Build management functionality for Rush CLI
//!
//! This module handles the core building capabilities, including build context management,
//! script generation, artifact rendering, and various build types.

mod artefact;
mod build_type;
mod context;
mod script;
mod spec;
mod types;
mod variables;

// Re-export key components
pub use artefact::Artefact;
pub use build_type::BuildType;
pub use context::BuildContext;
pub use script::BuildScript;
pub use spec::{ComponentBuildSpec, ServiceSpec};
//pub use types::BuildStatus;
pub use variables::Variables;

// Re-export templates for use in other modules
//pub(crate) use templates::TEMPLATES;
