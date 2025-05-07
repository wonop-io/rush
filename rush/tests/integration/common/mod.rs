//! Common utilities for integration tests
//! 
//! This module provides utilities that are shared across integration tests.

use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Sets up environment variables for integration testing
pub fn setup_test_env() {
    env::set_var("RUSH_TEST_MODE", "true");
    env::set_var("RUSH_LOG_LEVEL", "debug");
}

/// Cleans up environment variables after integration tests
pub fn cleanup_test_env() {
    env::remove_var("RUSH_TEST_MODE");
    env::remove_var("RUSH_LOG_LEVEL");
}

/// Creates a temporary directory for integration testing
pub fn create_temp_dir() -> TempDir {
    tempfile::tempdir().expect("Failed to create temporary directory")
}

/// Creates a temporary product directory with a basic rushd.yaml file
pub fn create_temp_product() -> (TempDir, PathBuf) {
    let temp_dir = create_temp_dir();
    let product_path = temp_dir.path().to_path_buf();
    
    // Create basic rushd.yaml
    let rushd_yaml = r#"
name: test-product
description: Test product for integration tests
env:
  - name: TEST_ENV
    value: test_value
"#;
    
    let rushd_path = product_path.join("rushd.yaml");
    let mut file = File::create(rushd_path).unwrap();
    file.write_all(rushd_yaml.as_bytes()).unwrap();
    
    (temp_dir, product_path)
}