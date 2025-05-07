//! Main integration test runner
//!
//! This file serves as the main entry point for integration tests.
mod common;

// Re-export individual test modules
mod basic_test;
mod dotenv_integration_test;

// Setup function that runs before all tests
#[cfg(test)]
#[ctor::ctor]
fn setup() {
    // Initialize the logger for tests
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .try_init();
    
    println!("Running integration tests...");
}

// Teardown function that runs after all tests
#[cfg(test)]
#[ctor::dtor]
fn teardown() {
    println!("Integration tests completed.");
}