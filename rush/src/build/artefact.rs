use crate::build::context::BuildContext;
use log::{debug, error, trace};
use std::fs;
use std::path::Path;
use tera::{Context, Tera};

/// Represents a file artifact that can be rendered using the BuildContext
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Artefact {
    /// Path to the input template file
    pub input_path: String,
    /// Path where the rendered output will be written
    pub output_path: String,
    /// Template content loaded from the input path
    pub template: String,
}

impl Artefact {
    /// Creates a new Artefact by loading a template from the given input path
    ///
    /// # Arguments
    ///
    /// * `input_path` - Path to the template file
    /// * `output_path` - Path where the rendered output will be written
    ///
    /// # Returns
    ///
    /// A new Artefact instance
    pub fn new(input_path: String, output_path: String) -> Self {
        trace!("Creating new artefact from {}", input_path);
        let template = fs::read_to_string(&input_path).unwrap_or_else(|e| {
            error!(
                "Failed to read template from input file {}: {}",
                input_path, e
            );
            panic!("Failed to read template from input file {}", input_path)
        });

        debug!(
            "Loaded template from {} ({} bytes)",
            input_path,
            template.len()
        );
        Artefact {
            input_path,
            output_path,
            template,
        }
    }

    /// Renders the template using the provided build context
    ///
    /// # Arguments
    ///
    /// * `context` - The build context containing variables for rendering
    ///
    /// # Returns
    ///
    /// The rendered template as a string
    pub fn render(&self, context: &BuildContext) -> String {
        trace!("Rendering template {} with context", self.input_path);
        let template = self.template.clone();

        let mut tera = Tera::default();
        tera.add_raw_templates(vec![(&self.input_path, template)])
            .unwrap_or_else(|e| {
                error!("Failed to add template to Tera: {}", e);
                panic!("Failed to add template to Tera")
            });

        let context = Context::from_serialize(context).expect("Could not create context");

        match tera.render(&self.input_path, &context) {
            Ok(rendered) => {
                debug!("Successfully rendered template {}", self.input_path);
                rendered
            }
            Err(e) => {
                error!("Failed to render template {}: {}", self.input_path, e);
                error!("Context: {:#?}", context);
                panic!("Failed to render template: {}", e);
            }
        }
    }

    /// Renders the template and writes the result to the output file
    ///
    /// # Arguments
    ///
    /// * `context` - The build context containing variables for rendering
    pub fn render_to_file(&self, context: &BuildContext) {
        trace!(
            "Rendering template {} to file {}",
            self.input_path,
            self.output_path
        );
        let rendered = self.render(context);

        // Ensure the output directory exists
        if let Some(parent) = Path::new(&self.output_path).parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).unwrap_or_else(|e| {
                    error!(
                        "Failed to create output directory {}: {}",
                        parent.display(),
                        e
                    );
                    panic!("Failed to create output directory {}", parent.display())
                });
            }
        }

        fs::write(&self.output_path, rendered).unwrap_or_else(|e| {
            error!("Failed to write to output file {}: {}", self.output_path, e);
            panic!("Failed to write to output file {}", self.output_path)
        });

        debug!(
            "Successfully wrote rendered template to {}",
            self.output_path
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::build_type::BuildType;
    use crate::core::types::ResourceRequirements;
    use std::sync::Arc;
    use crate::toolchain::Platform;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_context() -> BuildContext {
        let mut env = HashMap::new();
        env.insert("TEST_VAR".to_string(), "test_value".to_string());

        let mut domains = HashMap::new();
        domains.insert("test".to_string(), "test.example.com".to_string());

        BuildContext {
            build_type: BuildType::RustBinary {
                location: "src/test".to_string(),
                dockerfile_path: "Dockerfile".to_string(),
                context_dir: Some(".".to_string()),
                features: Some(vec!["test".to_string()]),
                precompile_commands: None,
            },
            location: Some("src/test".to_string()),
            target: Platform::new("linux", "x86_64"),
            host: Platform::new("linux", "x86_64"),
            rust_target: "x86_64-unknown-linux-gnu".to_string(),
            toolchain: crate::toolchain::ToolchainContext::new(
                Platform::new("linux", "x86_64"),
                Platform::new("linux", "x86_64")
            ),
            services: HashMap::new(),
            environment: "dev".to_string(),
            domain: "test.example.com".to_string(),
            product_name: "test-product".to_string(),
            product_uri: "test-product".to_string(),
            component: "test-component".to_string(),
            docker_registry: "test-registry".to_string(),
            image_name: "test-image:latest".to_string(),
            domains,
            env,
            secrets: HashMap::new(),
        }
    }

    #[test]
    fn test_artefact_render() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(
            temp_file,
            "Hello {{ component }}! Environment: {{ environment }}"
        )
        .unwrap();

        let artefact = Artefact::new(
            temp_file.path().to_string_lossy().to_string(),
            "output.txt".to_string(),
        );

        let context = create_test_context();
        let rendered = artefact.render(&context);

        assert_eq!(rendered, "Hello test-component! Environment: dev");
    }

    #[test]
    fn test_artefact_render_with_env_vars() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "Environment variable: {{ env.TEST_VAR }}").unwrap();

        let artefact = Artefact::new(
            temp_file.path().to_string_lossy().to_string(),
            "output.txt".to_string(),
        );

        let context = create_test_context();
        let rendered = artefact.render(&context);

        assert_eq!(rendered, "Environment variable: test_value");
    }
}
