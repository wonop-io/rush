//! Build script generation for different build types
//!
//! This module provides functionality for generating build scripts
//! based on component build types and contexts.

use std::error::Error;

use log::{debug, trace};
use rush_utils::TEMPLATES;
use tera::Context;

use crate::build_type::BuildType;
use crate::context::BuildContext;

/// Represents a build script that can be generated for a component
pub struct BuildScript {
    /// The type of build to perform
    build_type: BuildType,
}

impl BuildScript {
    /// Creates a new BuildScript for the specified build type
    ///
    /// # Arguments
    ///
    /// * `build_type` - The build type to generate a script for
    pub fn new(build_type: BuildType) -> Self {
        trace!("Creating new BuildScript for {build_type:?}");
        BuildScript { build_type }
    }

    /// Renders a build script based on the build context
    ///
    /// # Arguments
    ///
    /// * `context` - The build context containing variables for template rendering
    ///
    /// # Returns
    ///
    /// The rendered build script as a string
    pub fn render(&self, context: &BuildContext) -> String {
        debug!("Rendering build script for {:?}", self.build_type);

        let tera_context = Context::from_serialize(context)
            .expect("Could not create Tera context from build context");

        match &self.build_type {
            BuildType::TrunkWasm { .. } => {
                self.render_template("build/wasm_trunk.sh", &tera_context)
            }
            BuildType::DixiousWasm { .. } => {
                self.render_template("build/wasm_dixious.sh", &tera_context)
            }
            BuildType::RustBinary { .. } => {
                self.render_template("build/rust_binary.sh", &tera_context)
            }
            BuildType::Zola { .. } => self.render_template("build/zola.sh", &tera_context),
            BuildType::Book { .. } => self.render_template("build/mdbook.sh", &tera_context),
            BuildType::Script { .. }
            | BuildType::PureKubernetes
            | BuildType::KubernetesInstallation { .. }
            | BuildType::Ingress { .. }
            | BuildType::PureDockerImage { .. }
            | BuildType::LocalService { .. }
            | BuildType::Bazel { .. } => {
                // Bazel builds are handled directly in BuildOrchestrator, not via shell scripts
                trace!("No build script needed for {:?}", self.build_type);
                "".to_string()
            }
        }
    }

    /// Helper method to render a specific template with error handling
    ///
    /// # Arguments
    ///
    /// * `template_name` - The name of the template to render
    /// * `context` - The Tera context with template variables
    ///
    /// # Returns
    ///
    /// The rendered template as a string
    fn render_template(&self, template_name: &str, context: &Context) -> String {
        match TEMPLATES.render(template_name, context) {
            Ok(script) => script,
            Err(e) => {
                log::error!("Error rendering build script template: {e}");

                // Log detailed error information for debugging
                let mut cause = e.source();
                while let Some(err) = cause {
                    log::error!("Caused by: {err}");
                    cause = err.source();
                }

                panic!("Failed to render build script template '{template_name}'");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rush_toolchain::{Platform, ToolchainContext};

    use super::*;
    use crate::build_type::BuildType;
    use crate::context::BuildContext;

    fn create_test_context() -> BuildContext {
        let platform = Platform::default();
        let toolchain = ToolchainContext::default();

        BuildContext {
            build_type: BuildType::RustBinary {
                location: "test/location".to_string(),
                dockerfile_path: "test/Dockerfile".to_string(),
                context_dir: Some(".".to_string()),
                features: None,
                precompile_commands: None,
            },
            location: Some("test/location".to_string()),
            target: platform.clone(),
            host: platform,
            rust_target: "x86_64-unknown-linux-gnu".to_string(),
            toolchain,
            services: HashMap::new(),
            environment: "dev".to_string(),
            domain: "test.example.com".to_string(),
            product_name: "test-product".to_string(),
            product_uri: "test-product".to_string(),
            component: "test-component".to_string(),
            docker_registry: "registry.example.com".to_string(),
            image_name: "test-image:latest".to_string(),
            domains: HashMap::new(),
            env: HashMap::new(),
            secrets: HashMap::new(),
            cross_compile: "native".to_string(),
        }
    }

    #[test]
    fn test_rust_binary_script_generation() {
        let build_type = BuildType::RustBinary {
            location: "test/location".to_string(),
            dockerfile_path: "test/Dockerfile".to_string(),
            context_dir: Some(".".to_string()),
            features: None,
            precompile_commands: None,
        };

        let script = BuildScript::new(build_type);
        let context = create_test_context();

        let result = script.render(&context);

        // Basic checks that the script contains expected content
        assert!(result.contains("cd test/location"));
        assert!(result.contains("cargo build"));
    }

    #[test]
    fn test_no_script_for_pure_kubernetes() {
        let build_type = BuildType::PureKubernetes;
        let script = BuildScript::new(build_type);
        let context = create_test_context();

        let result = script.render(&context);

        assert_eq!(result, "");
    }
}
