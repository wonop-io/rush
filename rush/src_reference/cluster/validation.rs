use std::process::Command;

pub trait K8Validation {
    fn validate(&self, path: &str, version: &str) -> Result<(), String>;
}

pub struct KubeconformValidator;

impl K8Validation for KubeconformValidator {
    fn validate(&self, path: &str, version: &str) -> Result<(), String> {
        println!(
            "Executing: kubeconform -kubernetes-version {} -strict {}",
            version, path
        );
        let output = Command::new("kubeconform")
            .arg("-kubernetes-version")
            .arg(version)
            .arg("-strict")
            .arg(path)
            .output()
            .map_err(|e| format!("Failed to execute kubeconform: {}", e))?;

        if !output.status.success() {
            Err(format!(
                "kubeconform validation failed:\nstderr:\n{}\nstdout:\n{}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            ))
        } else {
            Ok(())
        }
    }
}

pub struct KubevalValidator;

impl K8Validation for KubevalValidator {
    fn validate(&self, path: &str, version: &str) -> Result<(), String> {
        let output = Command::new("kubeval")
            .arg("--strict")
            .arg("--kubernetes-version")
            .arg(version)
            .arg(path)
            .output()
            .map_err(|e| format!("Failed to execute kubeval: {}", e))?;

        if !output.status.success() {
            Err(format!(
                "kubeval validation failed:\nstderr:\n{}\nstdout:\n{}",
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&output.stdout)
            ))
        } else {
            Ok(())
        }
    }
}
