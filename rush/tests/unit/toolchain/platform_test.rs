use rush_cli::toolchain::Platform;
use rush_cli::toolchain::platform::{OperatingSystem, ArchType};
use std::env;

#[test]
fn test_operating_system_default() {
    let os = OperatingSystem::default();
    
    // The default should match the current platform
    match env::consts::OS {
        "linux" => assert_eq!(os, OperatingSystem::Linux),
        "macos" => assert_eq!(os, OperatingSystem::MacOS),
        _ => {} // Skip test for unsupported platforms
    }
}

#[test]
fn test_operating_system_to_docker_target() {
    assert_eq!(OperatingSystem::Linux.to_docker_target(), "linux");
    assert_eq!(OperatingSystem::MacOS.to_docker_target(), "linux"); // Docker target is Linux even on macOS
}

#[test]
fn test_operating_system_from_str() {
    assert_eq!(OperatingSystem::from_str("linux"), OperatingSystem::Linux);
    assert_eq!(OperatingSystem::from_str("macos"), OperatingSystem::MacOS);
}

#[test]
#[should_panic(expected = "Invalid platform type")]
fn test_operating_system_from_str_invalid() {
    OperatingSystem::from_str("windows");
}

#[test]
fn test_operating_system_to_string() {
    assert_eq!(OperatingSystem::Linux.to_string(), "linux");
    assert_eq!(OperatingSystem::MacOS.to_string(), "macos");
}

#[test]
fn test_arch_type_default() {
    let arch = ArchType::default();
    
    // The default should match the current architecture
    match env::consts::ARCH {
        "x86_64" => assert_eq!(arch, ArchType::X86_64),
        "aarch64" => assert_eq!(arch, ArchType::AARCH64),
        _ => {} // Skip test for unsupported architectures
    }
}

#[test]
fn test_arch_type_to_docker_target() {
    assert_eq!(ArchType::X86_64.to_docker_target(), "amd64");
    assert_eq!(ArchType::AARCH64.to_docker_target(), "arm64");
}

#[test]
fn test_arch_type_from_str() {
    assert_eq!(ArchType::from_str("x86_64"), ArchType::X86_64);
    assert_eq!(ArchType::from_str("aarch64"), ArchType::AARCH64);
}

#[test]
#[should_panic(expected = "Invalid architecture type")]
fn test_arch_type_from_str_invalid() {
    ArchType::from_str("arm");
}

#[test]
fn test_arch_type_to_string() {
    assert_eq!(ArchType::X86_64.to_string(), "x86_64");
    assert_eq!(ArchType::AARCH64.to_string(), "aarch64");
}

#[test]
fn test_platform_default() {
    let platform = Platform::default();
    
    match (env::consts::OS, env::consts::ARCH) {
        ("linux", "x86_64") => {
            assert_eq!(platform.os, OperatingSystem::Linux);
            assert_eq!(platform.arch, ArchType::X86_64);
        },
        ("linux", "aarch64") => {
            assert_eq!(platform.os, OperatingSystem::Linux);
            assert_eq!(platform.arch, ArchType::AARCH64);
        },
        ("macos", "x86_64") => {
            assert_eq!(platform.os, OperatingSystem::MacOS);
            assert_eq!(platform.arch, ArchType::X86_64);
        },
        ("macos", "aarch64") => {
            assert_eq!(platform.os, OperatingSystem::MacOS);
            assert_eq!(platform.arch, ArchType::AARCH64);
        },
        _ => {} // Skip test for unsupported platforms
    }
}

#[test]
fn test_platform_new() {
    let platform = Platform::new("linux", "x86_64");
    assert_eq!(platform.os, OperatingSystem::Linux);
    assert_eq!(platform.arch, ArchType::X86_64);
    
    let platform = Platform::new("macos", "aarch64");
    assert_eq!(platform.os, OperatingSystem::MacOS);
    assert_eq!(platform.arch, ArchType::AARCH64);
}

#[test]
fn test_platform_to_rust_target() {
    let platform = Platform::new("linux", "x86_64");
    assert_eq!(platform.to_rust_target(), "x86_64-unknown-linux-gnu");
    
    let platform = Platform::new("macos", "aarch64");
    assert_eq!(platform.to_rust_target(), "aarch64-unknown-macos-gnu");
}

#[test]
fn test_platform_to_docker_target() {
    let platform = Platform::new("linux", "x86_64");
    assert_eq!(platform.to_docker_target(), "linux/amd64");
    
    let platform = Platform::new("macos", "aarch64");
    assert_eq!(platform.to_docker_target(), "linux/arm64");
}

#[test]
fn test_platform_to_string() {
    let platform = Platform::new("linux", "x86_64");
    assert_eq!(platform.to_string(), "linux-x86_64");
    
    let platform = Platform::new("macos", "aarch64");
    assert_eq!(platform.to_string(), "macos-aarch64");
}