//! Unit test module
//! 
//! This file organizes all unit tests for the project

// Re-export individual unit test modules
mod container_test;
mod dotenv_utils_test;
mod path_matcher_test;
mod secrets_adapter_test;
mod toolchain_test;
mod utils_test;
mod vault_test;

#[cfg(test)]
#[ctor::ctor]
fn setup() {
    // Initialize the logger for tests with lower verbosity for unit tests
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Warn)
        .is_test(true)
        .try_init();
    
    println!("Running unit tests...");
}

#[cfg(test)]
#[ctor::dtor]
fn teardown() {
    println!("Unit tests completed.");
}