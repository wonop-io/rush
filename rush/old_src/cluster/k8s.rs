pub use super::k8_encoder::{K8Encoder, NoopEncoder, SealedSecretsEncoder};
use crate::builder::Artefact;
use crate::builder::BuildContext;
use crate::builder::BuildType;
use crate::builder::ComponentBuildSpec;
use crate::cluster::run_command;
use crate::toolchain::ToolchainContext;
use colored::Colorize;
use log::{error, trace};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

pub struct K8ManifestArtefact {
    pub artefact: Artefact,
    encoder: Arc<dyn K8Encoder>,
}

impl K8ManifestArtefact {
    pub fn new(input_path: String, output_path: String, encoder: Arc<dyn K8Encoder>) -> Self {
        K8ManifestArtefact {
            artefact: Artefact::new(input_path, output_path),
            encoder,
        }
    }

    pub fn from_artefact(artefact: Artefact, encoder: Arc<dyn K8Encoder>) -> Self {
        K8ManifestArtefact { artefact, encoder }
    }

    pub fn render(&self, context: &BuildContext) -> String {
        self.artefact.render(context)
    }

    pub fn render_to_file(&self, context: &BuildContext) {
        self.artefact.render_to_file(context);
        match self.encoder.encode_file(&self.artefact.output_path) {
            Ok(_) => trace!("Encoded file {}", self.artefact.output_path),
            Err(e) => {
                error!("Failed to encode file {}: {}", self.artefact.output_path, e);
                panic!("Encoding failed");
            }
        }
    }

    pub fn update_encoder(&mut self, encoder: Arc<dyn K8Encoder>) {
        self.encoder = encoder;
    }
}

pub struct K8ClusterManifests {
    components: Vec<K8ComponentManifests>,
    toolchain: Option<Arc<ToolchainContext>>,
    output_directory: PathBuf,
    encoder: Arc<dyn K8Encoder>,
}

impl K8ClusterManifests {
    pub fn new(
        output_directory: PathBuf,
        toolchain: Option<Arc<ToolchainContext>>,
        encoder: Arc<dyn K8Encoder>,
    ) -> Self {
        K8ClusterManifests {
            components: Vec::new(),
            toolchain,
            output_directory,
            encoder,
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
            self.encoder.clone(),
        ));
    }

    pub fn output_directory(&self) -> &PathBuf {
        &self.output_directory
    }

    pub fn components(&self) -> &Vec<K8ComponentManifests> {
        &self.components
    }

    pub fn update_encoder(&mut self, encoder: Arc<dyn K8Encoder>) {
        self.encoder = encoder.clone();
        for component in &mut self.components {
            component.update_encoder(encoder.clone());
        }
    }
}

pub struct K8ComponentManifests {
    name: String,
    spec: Arc<Mutex<ComponentBuildSpec>>,
    is_installation: bool,
    manifests: Vec<K8ManifestArtefact>,
    input_directory: PathBuf,
    output_directory: PathBuf,
    toolchain: Option<Arc<ToolchainContext>>,
    namespace: String,
    encoder: Arc<dyn K8Encoder>,
}

impl K8ComponentManifests {
    pub fn new(
        name: &str,
        spec: Arc<Mutex<ComponentBuildSpec>>,
        input_directory: PathBuf,
        output_directory: PathBuf,
        toolchain: Option<Arc<ToolchainContext>>,
        encoder: Arc<dyn K8Encoder>,
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
            encoder: encoder.clone(),
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
                ret.manifests
                    .push(K8ManifestArtefact::from_artefact(artefact, encoder.clone()));
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

    pub fn manifests(&self) -> &Vec<K8ManifestArtefact> {
        &self.manifests
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn add_manifest(&mut self, manifest: Artefact) {
        self.manifests.push(K8ManifestArtefact::from_artefact(
            manifest,
            self.encoder.clone(),
        ));
    }

    pub fn update_encoder(&mut self, encoder: Arc<dyn K8Encoder>) {
        self.encoder = encoder.clone();
        for manifest in &mut self.manifests {
            manifest.update_encoder(encoder.clone());
        }
    }

    pub async fn apply(&self) -> Result<(), String> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };

        for manifest in &self.manifests {
            let output_path = manifest.artefact.output_path.to_string();
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
            let output_path = manifest.artefact.output_path.to_string();
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
