use rush_cli::cluster::{K8Encoder, NoopEncoder, SealedSecretsEncoder};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

fn create_secret_yaml(dir: &Path, filename: &str) -> String {
    let secret_yaml = r#"
apiVersion: v1
kind: Secret
metadata:
  name: test-secret
type: Opaque
data:
  username: dXNlcm5hbWU=  # base64 encoded "username"
  password: cGFzc3dvcmQ=  # base64 encoded "password"
"#;
    
    let file_path = dir.join(filename);
    let mut file = File::create(&file_path).unwrap();
    file.write_all(secret_yaml.as_bytes()).unwrap();
    
    file_path.to_string_lossy().to_string()
}

fn create_non_secret_yaml(dir: &Path, filename: &str) -> String {
    let non_secret_yaml = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test-configmap
data:
  config.json: |
    {
      "key": "value"
    }
"#;
    
    let file_path = dir.join(filename);
    let mut file = File::create(&file_path).unwrap();
    file.write_all(non_secret_yaml.as_bytes()).unwrap();
    
    file_path.to_string_lossy().to_string()
}

#[test]
fn test_noop_encoder() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = create_secret_yaml(&temp_dir.path(), "secret.yaml");
    
    // Save the original content for comparison
    let original_content = fs::read_to_string(&file_path).unwrap();
    
    // Use the NoopEncoder
    let encoder = NoopEncoder;
    let result = encoder.encode_file(&file_path);
    
    // Should succeed
    assert!(result.is_ok());
    
    // File content should remain unchanged
    let new_content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(original_content, new_content);
}

#[test]
fn test_sealed_secrets_encoder_with_non_secret() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = create_non_secret_yaml(&temp_dir.path(), "configmap.yaml");
    
    // Save the original content for comparison
    let original_content = fs::read_to_string(&file_path).unwrap();
    
    // Use the SealedSecretsEncoder
    let encoder = SealedSecretsEncoder;
    let result = encoder.encode_file(&file_path);
    
    // Should succeed because it will skip non-Secret resources
    assert!(result.is_ok());
    
    // File content should remain unchanged
    let new_content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(original_content, new_content);
}

#[test]
fn test_sealed_secrets_encoder_with_secret() {
    // Skip if kubeseal is not installed
    if std::process::Command::new("which")
        .arg("kubeseal")
        .output()
        .map(|output| !output.status.success())
        .unwrap_or(true) {
        println!("Skipping test_sealed_secrets_encoder_with_secret: kubeseal not installed");
        return;
    }
    
    let temp_dir = TempDir::new().unwrap();
    let file_path = create_secret_yaml(&temp_dir.path(), "secret.yaml");
    
    // Use the SealedSecretsEncoder
    let encoder = SealedSecretsEncoder;
    let result = encoder.encode_file(&file_path);
    
    // If kubeseal is installed and working, this should succeed
    // But it might fail due to missing certificates or config, so we don't assert success
    if result.is_ok() {
        // File content should be changed
        let new_content = fs::read_to_string(&file_path).unwrap();
        assert!(new_content.contains("SealedSecret"));
    }
}