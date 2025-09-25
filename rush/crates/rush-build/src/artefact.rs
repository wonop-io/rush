use std::error::Error;
use std::fs;
use std::path::Path;

use log::{debug, error, trace};
use tera::{Context, Tera};

use crate::context::BuildContext;

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
    /// A new Artefact instance or an error if the template cannot be loaded
    pub fn new(input_path: String, output_path: String) -> Result<Self, rush_core::error::Error> {
        trace!("Creating new artefact from {}", input_path);
        let template = match fs::read_to_string(&input_path) {
            Ok(content) => content,
            Err(e) => {
                error!("\n=== Failed to Load Template File ===");
                error!("Template path: {}", input_path);
                error!("Error: {}", e);

                // Check if file exists and provide helpful information
                let path = Path::new(&input_path);
                if !path.exists() {
                    error!("\n⚠️  File does not exist!");
                    if let Some(parent) = path.parent() {
                        error!("   Parent directory: {}", parent.display());
                        if parent.exists() {
                            error!("   Parent directory exists: ✓");
                            // Try to list files in parent directory
                            if let Ok(entries) = fs::read_dir(parent) {
                                error!("   Files in parent directory:");
                                for entry in entries.flatten().take(10) {
                                    if let Some(name) = entry.file_name().to_str() {
                                        error!("     - {}", name);
                                    }
                                }
                            }
                        } else {
                            error!("   Parent directory does not exist: ✗");
                        }
                    }
                } else if let Err(e) = fs::metadata(&input_path) {
                    error!("\n⚠️  File exists but cannot read metadata: {}", e);
                    error!("   Possible permission issue?");
                }

                error!("\nPossible causes:");
                error!("1. Template file path is incorrect");
                error!("2. File was not created or copied to the expected location");
                error!("3. Permission issues preventing file access");
                error!("4. Path contains invalid characters or symbolic links");

                return Err(rush_core::error::Error::FileSystem {
                    path: std::path::PathBuf::from(&input_path),
                    message: format!("Failed to read template file: {e}"),
                });
            }
        };

        debug!(
            "Loaded template from {} ({} bytes)",
            input_path,
            template.len()
        );
        Ok(Artefact {
            input_path,
            output_path,
            template,
        })
    }

    /// Renders the template using the provided build context
    ///
    /// # Arguments
    ///
    /// * `context` - The build context containing variables for rendering
    ///
    /// # Returns
    ///
    /// The rendered template as a string or an error
    pub fn render(&self, context: &BuildContext) -> Result<String, rush_core::error::Error> {
        trace!("Rendering template {} with context", self.input_path);
        let template = self.template.clone();

        let mut tera = Tera::default();
        tera.add_raw_templates(vec![(&self.input_path, template)])
            .unwrap_or_else(|e| {
                error!("Failed to add template to Tera: {}", e);
                panic!("Failed to add template to Tera")
            });

        let tera_context = Context::from_serialize(context).expect("Could not create context");

        match tera.render(&self.input_path, &tera_context) {
            Ok(rendered) => {
                debug!("Successfully rendered template {}", self.input_path);
                Ok(rendered)
            }
            Err(e) => {
                error!("\n=== Template Rendering Failed ===");
                error!("Template file: {}", self.input_path);
                error!("Output path: {}", self.output_path);
                // Show the actual Tera error details
                error!("\nTera Error: {}", e);

                // Show the error chain if available
                let mut source = e.source();
                let mut depth = 1;
                while let Some(err) = source {
                    error!("  Caused by ({}): {}", depth, err);
                    source = err.source();
                    depth += 1;
                }

                // Try to provide more specific error information
                let error_msg = e.to_string();
                if error_msg.contains("Variable") || error_msg.contains("not found") {
                    error!("\n💡 This appears to be a missing variable error.");
                    error!("   Check that all variables used in the template are defined in the context.");
                    error!("   Available variables in context:");
                    if let Ok(json) = serde_json::to_string_pretty(context) {
                        for line in json.lines().take(50) {
                            // Show first 50 lines of context
                            error!("     {}", line);
                        }
                        if json.lines().count() > 50 {
                            error!(
                                "     ... (context truncated, {} more lines)",
                                json.lines().count() - 50
                            );
                        }
                    }
                } else if error_msg.contains("Syntax") || error_msg.contains("parse") {
                    error!("\n💡 This appears to be a template syntax error.");
                    error!("   Check the Jinja2/Tera template syntax in your file.");
                    error!("   Common issues:");
                    error!(
                        "   - Unclosed tags: {{{{{{ variable }}}} should be {{{{{{ variable }}}}}}"
                    );
                    error!("   - Invalid filters: {{{{ var | unknown_filter }}}}");
                    error!("   - Missing endif/endfor for conditionals/loops");
                }

                error!("\nContext (debug format): {:#?}", tera_context);
                return Err(rush_core::error::Error::Template(
                    e.to_string(), // Use the actual Tera error message instead of wrapping it
                ));
            }
        }
    }

    /// Renders the template and writes the result to the output file
    ///
    /// # Arguments
    ///
    /// * `context` - The build context containing variables for rendering
    ///
    /// # Returns
    ///
    /// Result indicating success or failure
    pub fn render_to_file(&self, context: &BuildContext) -> Result<(), rush_core::error::Error> {
        trace!(
            "Rendering template {} to file {}",
            self.input_path,
            self.output_path
        );
        let rendered = self.render(context)?;

        // Ensure the output directory exists
        if let Some(parent) = Path::new(&self.output_path).parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| {
                    error!(
                        "Failed to create output directory {}: {}",
                        parent.display(),
                        e
                    );
                    rush_core::error::Error::FileSystem {
                        path: parent.to_path_buf(),
                        message: format!("Failed to create output directory: {e}"),
                    }
                })?;
            }
        }

        fs::write(&self.output_path, rendered).map_err(|e| {
            error!("Failed to write to output file {}: {}", self.output_path, e);
            rush_core::error::Error::FileSystem {
                path: std::path::PathBuf::from(&self.output_path),
                message: format!("Failed to write to output file: {e}"),
            }
        })?;

        debug!(
            "Successfully wrote rendered template to {}",
            self.output_path
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write;

    use rush_toolchain::Platform;
    use tempfile::NamedTempFile;

    use super::*;
    use crate::build_type::BuildType;

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
            toolchain: rush_toolchain::ToolchainContext::create_with_platforms(
                Platform::new("linux", "x86_64"),
                Platform::new("linux", "x86_64"),
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
            cross_compile: "native".to_string(),
        }
    }

    #[test]
    fn test_artefact_render() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(
            temp_file,
            "Hello {{{{ component }}}}! Environment: {{{{ environment }}}}"
        )
        .unwrap();
        temp_file.flush().unwrap();

        let artefact = Artefact::new(
            temp_file.path().to_string_lossy().to_string(),
            "output.txt".to_string(),
        )
        .unwrap();

        let context = create_test_context();
        let rendered = artefact.render(&context).unwrap();

        assert_eq!(rendered, "Hello test-component! Environment: dev");
    }

    #[test]
    fn test_artefact_render_with_env_vars() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "Environment variable: {{{{ env.TEST_VAR }}}}").unwrap();
        temp_file.flush().unwrap();

        let artefact = Artefact::new(
            temp_file.path().to_string_lossy().to_string(),
            "output.txt".to_string(),
        )
        .unwrap();

        let context = create_test_context();
        let rendered = artefact.render(&context).unwrap();

        assert_eq!(rendered, "Environment variable: test_value");
    }
}
