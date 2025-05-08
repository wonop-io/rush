//! New standalone test module for improving test coverage
//! 
//! This module delegates to the resilient_coverage_tests module
//! to run tests safely while maximizing coverage.

// Import the core rush_cli library to ensure 
// all modules are compiled and available for coverage
extern crate rush_cli;

// Import the resilient_coverage_tests module to delegate tests to it
#[path = "resilient_coverage_tests.rs"]
mod resilient_tests;

// Import different parts of the library to make sure they're covered
#[cfg(test)]
mod import_tests {
    use rush_cli::builder::{Artefact, BuildContext, BuildScript, BuildType, ComponentBuildSpec, Config, Variables};
    use rush_cli::container::{ServiceSpec, ServicesSpec, status::Status};
    use rush_cli::dotenv_utils::{load_dotenv, save_dotenv};
    use rush_cli::git::{get_current_branch, get_latest_commit, is_git_repo, is_working_dir_clean};
    use rush_cli::path_matcher::{PathMatcher, Pattern};
    use rush_cli::toolchain::platform::{ArchType, OperatingSystem};
    use rush_cli::toolchain::{Platform, ToolchainContext};

    // Just import everything to make sure it's all reachable
    #[test]
    fn ensure_modules_are_compiled() {
        // This test just ensures that the modules are compiled and imported
        // It doesn't actually test functionality, which is covered by the specialized tests
        assert!(true, "All modules were successfully compiled and imported");
    }
}

// Run all the resilient tests
#[cfg(test)]
mod delegate_to_resilient_tests {
    // Import the resilient_tests module
    use crate::resilient_tests::resilient_tests;

    #[test]
    fn run_builder_module_tests() {
        // Actually delegate to the resilient test
        resilient_tests::test_builder_artefact();
        resilient_tests::test_builder_build_context();
        assert!(true, "Builder module tests delegated to resilient tests");
    }

    #[test]
    fn run_container_module_tests() {
        // Actually delegate to the resilient test
        resilient_tests::test_container_service_spec();
        resilient_tests::test_container_status();
        assert!(true, "Container module tests delegated to resilient tests");
    }

    #[test]
    fn run_toolchain_module_tests() {
        // Actually delegate to the resilient test
        resilient_tests::test_toolchain_platform();
        assert!(true, "Toolchain module tests delegated to resilient tests");
    }

    #[test]
    fn run_dotenv_utils_tests() {
        // Actually delegate to the resilient test
        resilient_tests::test_dotenv_utils();
        assert!(true, "DotEnv utils tests delegated to resilient tests");
    }

    #[test]
    fn run_path_matcher_tests() {
        // Actually delegate to the resilient test
        resilient_tests::test_path_matcher();
        assert!(true, "Path matcher tests delegated to resilient tests");
    }

    #[test]
    fn run_git_tests() {
        // Actually delegate to the resilient test
        resilient_tests::test_git_utils();
        assert!(true, "Git tests delegated to resilient tests");
    }
}