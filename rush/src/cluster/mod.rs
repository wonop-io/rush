mod infrastructure;
mod k8_encoder;
mod k8s;

use crate::toolchain::ToolchainContext;
use crate::utils::run_command;
use colored::Colorize;
use std::process::Command;
use std::sync::Arc;

pub use infrastructure::InfrastructureRepo;
pub use k8_encoder::{K8Encoder, NoopEncoder, SealedSecretsEncoder};
pub use k8s::K8ClusterManifests;

pub struct Minikube {
    toolchain: Arc<ToolchainContext>,
}

impl Minikube {
    pub fn new(toolchain: Arc<ToolchainContext>) -> Self {
        Minikube { toolchain }
    }

    pub async fn start(&self) -> Result<String, String> {
        let minikube_executable = self
            .toolchain
            .minikube()
            .ok_or_else(|| "Minikube executable not found. Please install it.".to_string())?;
        run_command(
            "minikube".white().bold(),
            &minikube_executable,
            vec!["start"],
        )
        .await
    }

    pub async fn stop(&self) -> Result<String, String> {
        let minikube_executable = self
            .toolchain
            .minikube()
            .ok_or_else(|| "Minikube executable not found. Please install it.".to_string())?;
        run_command(
            "minikube".white().bold(),
            &minikube_executable,
            vec!["stop"],
        )
        .await
    }

    pub async fn delete(&self) -> Result<String, String> {
        let minikube_executable = self
            .toolchain
            .minikube()
            .ok_or_else(|| "Minikube executable not found. Please install it.".to_string())?;
        run_command(
            "minikube".white().bold(),
            &minikube_executable,
            vec!["delete"],
        )
        .await
    }

    pub async fn get_ip(&self) -> Result<String, String> {
        let minikube_executable = match self.toolchain.minikube() {
            Some(minikube_executable) => minikube_executable,
            None => return Err("Minikube executable not found. Please install it.".to_string()),
        };

        let output = Command::new(minikube_executable)
            .arg("ip")
            .output()
            .expect("Failed to get minikube IP");

        if !output.status.success() {
            Err(format!(
                "Failed to get minikube IP: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        } else {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
    }
}
