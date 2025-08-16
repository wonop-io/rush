pub mod checks;
pub mod error;

pub use checks::{check_all_requirements, check_docker, check_rust_targets, check_trunk};
pub use error::{HelperError, HelperResult};

use std::process::Command;

pub fn run_preflight_checks() -> HelperResult<()> {
    check_all_requirements()
}

pub fn is_apple_silicon() -> bool {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("uname")
            .arg("-m")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok());

        output.is_some_and(|s| s.trim() == "arm64")
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub fn get_platform() -> String {
    if cfg!(target_os = "macos") {
        if is_apple_silicon() {
            "macos-arm64".to_string()
        } else {
            "macos-x86_64".to_string()
        }
    } else if cfg!(target_os = "linux") {
        "linux-x86_64".to_string()
    } else if cfg!(target_os = "windows") {
        "windows-x86_64".to_string()
    } else {
        "unknown".to_string()
    }
}
