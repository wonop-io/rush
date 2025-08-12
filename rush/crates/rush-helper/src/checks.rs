use crate::error::{HelperError, HelperResult};
use std::process::Command;
use which::which;

pub fn check_all_requirements() -> HelperResult<()> {
    let mut errors = Vec::new();
    let mut commands = Vec::new();
    
    // Check Rust and required targets
    if let Err(e) = check_rust_targets() {
        errors.push(e.get_message());
        commands.extend(e.get_fix_commands());
    }
    
    // Check Docker
    if let Err(e) = check_docker() {
        errors.push(e.get_message());
        commands.extend(e.get_fix_commands());
    }
    
    // Check trunk for WASM builds
    if let Err(e) = check_trunk() {
        errors.push(e.get_message());
        commands.extend(e.get_fix_commands());
    }
    
    // Check platform-specific tools
    if let Err(e) = check_platform_specific() {
        errors.push(e.get_message());
        commands.extend(e.get_fix_commands());
    }
    
    if !errors.is_empty() {
        return Err(HelperError::MultipleIssues {
            issues: errors.join("\n"),
            commands,
        });
    }
    
    Ok(())
}

pub fn check_rust_targets() -> HelperResult<()> {
    // Check if rustup is installed
    if which("rustup").is_err() {
        return Err(HelperError::missing_tool(
            "rustup",
            vec![
                "curl".to_string(),
                "--proto".to_string(),
                "'=https'".to_string(),
                "--tlsv1.2".to_string(),
                "-sSf".to_string(),
                "https://sh.rustup.rs".to_string(),
                "|".to_string(),
                "sh".to_string(),
            ],
        ));
    }
    
    // Get list of installed targets
    let output = Command::new("rustup")
        .args(&["target", "list", "--installed"])
        .output()
        .map_err(|e| HelperError::CommandFailed(format!("Failed to list rustup targets: {}", e)))?;
    
    let installed_targets = String::from_utf8_lossy(&output.stdout);
    
    // Check for required targets
    let mut missing_targets = Vec::new();
    
    // Always need wasm target for frontend
    if !installed_targets.contains("wasm32-unknown-unknown") {
        missing_targets.push("wasm32-unknown-unknown");
    }
    
    // Check for x86_64 Linux target (needed for cross-compilation)
    if !installed_targets.contains("x86_64-unknown-linux-gnu") {
        missing_targets.push("x86_64-unknown-linux-gnu");
    }
    
    // On Apple Silicon, also check for x86_64 Darwin target
    if crate::is_apple_silicon() && !installed_targets.contains("x86_64-apple-darwin") {
        missing_targets.push("x86_64-apple-darwin");
    }
    
    if !missing_targets.is_empty() {
        let mut errors = Vec::new();
        let mut commands = Vec::new();
        
        for target in missing_targets {
            errors.push(format!("Missing Rust target: {}", target));
            commands.push(vec![
                "rustup".to_string(),
                "target".to_string(),
                "add".to_string(),
                target.to_string(),
            ]);
        }
        
        return Err(HelperError::MultipleIssues {
            issues: errors.join("\n"),
            commands,
        });
    }
    
    Ok(())
}

pub fn check_docker() -> HelperResult<()> {
    // Check if Docker is installed
    if which("docker").is_err() {
        return Err(HelperError::MissingTool {
            message: "Docker is not installed. Please install Docker Desktop from https://www.docker.com/products/docker-desktop".to_string(),
            command: vec![],
        });
    }
    
    // Check if Docker daemon is running
    let output = Command::new("docker")
        .arg("info")
        .output();
    
    if output.is_err() || !output.unwrap().status.success() {
        return Err(HelperError::ConfigurationError {
            message: "Docker daemon is not running. Please start Docker Desktop".to_string(),
            command: vec![],
        });
    }
    
    // Check for buildx
    let buildx_output = Command::new("docker")
        .args(&["buildx", "version"])
        .output();
    
    if buildx_output.is_err() || !buildx_output.unwrap().status.success() {
        return Err(HelperError::MissingTool {
            message: "Docker buildx is not available. It should come with Docker Desktop".to_string(),
            command: vec![],
        });
    }
    
    Ok(())
}

pub fn check_trunk() -> HelperResult<()> {
    // Check for trunk or wasm-trunk
    let trunk_exists = which("trunk").is_ok();
    let wasm_trunk_exists = which("wasm-trunk").is_ok();
    
    if !trunk_exists && !wasm_trunk_exists {
        return Err(HelperError::MissingTool {
            message: "trunk is not installed (required for WASM frontend builds)".to_string(),
            command: vec![
                "cargo".to_string(),
                "install".to_string(),
                "trunk".to_string(),
            ],
        });
    }
    
    Ok(())
}

pub fn check_platform_specific() -> HelperResult<()> {
    if crate::is_apple_silicon() {
        check_apple_silicon_toolchain()
    } else {
        Ok(())
    }
}

fn check_apple_silicon_toolchain() -> HelperResult<()> {
    // Check for x86_64-unknown-linux-gnu toolchain
    let output = Command::new("brew")
        .args(&["list", "--formula"])
        .output();
    
    if output.is_err() {
        return Err(HelperError::MissingTool {
            message: "Homebrew is not installed (required for cross-compilation toolchain)".to_string(),
            command: vec![
                "/bin/bash".to_string(),
                "-c".to_string(),
                "\"$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\"".to_string(),
            ],
        });
    }
    
    let output = output.unwrap();
    let installed_formulae = String::from_utf8_lossy(&output.stdout);
    
    if !installed_formulae.contains("x86_64-unknown-linux-gnu") {
        return Err(HelperError::MissingTool {
            message: "x86_64-unknown-linux-gnu toolchain not installed (required for cross-compilation on Apple Silicon)".to_string(),
            command: vec![
                "arch".to_string(),
                "-arm64".to_string(),
                "brew".to_string(),
                "install".to_string(),
                "SergioBenitez/osxct/x86_64-unknown-linux-gnu".to_string(),
            ],
        });
    }
    
    // Rush handles linker configuration through environment variables, no need to check cargo config
    Ok(())
}

pub fn check_rush_version() -> HelperResult<String> {
    let output = Command::new("rush")
        .arg("--version")
        .output();
    
    match output {
        Ok(o) if o.status.success() => {
            let version = String::from_utf8_lossy(&o.stdout);
            Ok(version.trim().to_string())
        }
        _ => Err(HelperError::MissingTool {
            message: "rush-cli is not installed or not in PATH".to_string(),
            command: vec![
                "cargo".to_string(),
                "install".to_string(),
                "rush-cli".to_string(),
            ],
        })
    }
}