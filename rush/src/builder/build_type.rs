use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum BuildType {
    TrunkWasm {
        location: String,
        dockerfile_path: String,
        context_dir: Option<String>,
    },
    RustBinary {
        location: String,
        dockerfile_path: String,
        context_dir: Option<String>,
    },
    Script {
        location: String,
        dockerfile_path: String,
        context_dir: Option<String>,
    },
    Book {
        location: String,
        dockerfile_path: String,
        context_dir: Option<String>,
    },
    Ingress {
        components: Vec<String>,
        dockerfile_path: String,
        context_dir: Option<String>,
    },
    PureDockerImage {
        image_name_with_tag: String,
        command: Option<String>,
        entrypoint: Option<String>,
    },
    PureKubernetes,
    KubernetesInstallation {
        namespace: String,
    },
}
