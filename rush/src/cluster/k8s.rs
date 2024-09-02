use crate::builder::Artefact;
use crate::builder::BuildContext;
use crate::builder::BuildType;
use crate::builder::ComponentBuildSpec;
use crate::cluster::run_command;
use crate::toolchain::ToolchainContext;
use colored::Colorize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

pub struct K8ClusterManifests {
    components: Vec<K8ComponentManifests>,
    toolchain: Option<Arc<ToolchainContext>>,
    output_directory: PathBuf,
}

impl K8ClusterManifests {
    pub fn new(output_directory: PathBuf, toolchain: Option<Arc<ToolchainContext>>) -> Self {
        K8ClusterManifests {
            components: Vec::new(),
            toolchain,
            output_directory,
        }
    }

    pub fn add_component(
        &mut self,
        name: &str,
        spec: Arc<Mutex<ComponentBuildSpec>>,
        input_directory: PathBuf,
    ) {
        let output_directory = self.output_directory.join(name);
        self.components.push(K8ComponentManifests::new(
            name,
            spec,
            input_directory,
            output_directory,
            self.toolchain.clone(),
        ));
    }

    pub fn output_directory(&self) -> &PathBuf {
        &self.output_directory
    }

    pub fn components(&self) -> &Vec<K8ComponentManifests> {
        &self.components
    }
}

pub struct K8ComponentManifests {
    name: String,
    spec: Arc<Mutex<ComponentBuildSpec>>,
    is_installation: bool,
    manifests: Vec<Artefact>,
    input_directory: PathBuf,
    output_directory: PathBuf,
    toolchain: Option<Arc<ToolchainContext>>,
    namespace: String,
}

impl K8ComponentManifests {
    pub fn new(
        name: &str,
        spec: Arc<Mutex<ComponentBuildSpec>>,
        input_directory: PathBuf,
        output_directory: PathBuf,
        toolchain: Option<Arc<ToolchainContext>>,
    ) -> Self {
        let (is_installation, namespace) = if let BuildType::KubernetesInstallation { namespace } =
            &spec.lock().unwrap().build_type
        {
            (true, namespace.clone())
        } else {
            (false, "default".to_string())
        };
        let mut ret = K8ComponentManifests {
            name: name.to_string(),
            manifests: Vec::new(),
            input_directory: input_directory.clone(),
            output_directory: output_directory.clone(),
            toolchain,
            is_installation,
            spec,
            namespace,
        };

        let paths = std::fs::read_dir(&input_directory)
            .unwrap_or_else(|_| {
                panic!(
                    "Failed to read input directory: {}",
                    input_directory.display()
                )
            })
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() || path.extension().map_or(false, |ext| ext == "yaml"))
            .collect::<Vec<_>>();

        for path in paths {
            if !path.is_dir() {
                let output_path = output_directory.join(path.file_name().unwrap());
                let artefact = Artefact::new(
                    path.clone().display().to_string(),
                    output_path.display().to_string(),
                );
                ret.manifests.push(artefact);
            }
        }

        ret
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn is_installation(&self) -> bool {
        self.is_installation
    }

    pub fn spec(&self) -> ComponentBuildSpec {
        self.spec.lock().unwrap().clone()
    }

    pub fn input_directory(&self) -> &PathBuf {
        &self.input_directory
    }

    pub fn output_directory(&self) -> &PathBuf {
        &self.output_directory
    }

    pub fn manifests(&self) -> &Vec<Artefact> {
        &self.manifests
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn add_manifest(&mut self, manifest: Artefact) {
        self.manifests.push(manifest);
    }

    pub fn render_all(&self, context: &BuildContext) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.output_directory)?;
        for manifest in &self.manifests {
            manifest.render_to_file(context);
        }
        Ok(())
    }

    pub async fn apply(&self) -> Result<(), String> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };

        for manifest in &self.manifests {
            let output_path = manifest.output_path.to_string();
            run_command(
                "kubectl apply".white(),
                toolchain.kubectl(),
                vec!["apply", "-f", &output_path],
            )
            .await?;
        }
        Ok(())
    }

    pub async fn unapply(&self) -> Result<(), String> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };

        for manifest in self.manifests.iter().rev() {
            let output_path = manifest.output_path.to_string();
            run_command(
                "kubectl delete".white(),
                toolchain.kubectl(),
                vec!["delete", "-f", &output_path],
            )
            .await?;
        }
        Ok(())
    }
}
