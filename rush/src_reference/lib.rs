//! Rush CLI Library
//! 
//! This is the library portion of the Rush CLI tool.
//! It exports modules for use in tests and potentially other crates.

// Re-export modules needed for testing
pub mod builder;
pub mod cluster;
pub mod container;
pub mod dotenv_utils;
pub mod git;
pub mod path_matcher;
pub mod public_env_defs;
pub mod toolchain;
pub mod utils;
pub mod vault;

// Add any necessary module-level documentation or helper functions here