// Main test runner for Rush CLI tests
#[path = "common/mod.rs"]
pub mod common;

// Import test modules
#[path = "container/mod.rs"]
mod container_tests;

// Integration tests for Docker
#[path = "docker_integration_test.rs"]
mod docker_integration;

// Additional test modules can be added here as they are developed

// Unit tests in this file
#[cfg(test)]
mod tests {
    // This test is just a placeholder to ensure the test infrastructure is working
    #[test]
    fn test_sanity_check() {
        assert!(true, "Basic sanity check");
    }
}