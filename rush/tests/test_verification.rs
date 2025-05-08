//! Test Suite Verification
//!
//! This test module verifies that all the required test components are in place
//! and running correctly. It acts as a sanity check for the test infrastructure.

use std::path::Path;
use crate::test_utils::TestProjectBuilder;

#[cfg(test)]
mod tests {


    use super::*;

    #[test]
    fn verify_test_directory_structure() {
        // Check that the test directories exist
        assert!(Path::new("tests").exists());
        assert!(Path::new("tests/unit").exists());
        assert!(Path::new("tests/integration").exists());

        // Check that we have test utilities
        assert!(Path::new("tests/test_utils/mod.rs").exists());
        assert!(Path::new("tests/integration/common.rs").exists());
    }

    #[test]
    fn verify_external_tools() {
        // Check that git is available for git-related tests
        // Use std instead of rush_cli modules
        let git = which_command("git");
        println!("Found git at: {:?}", git);

        // This test is informational only - it will not fail if git is not found
        // The actual git tests will skip themselves if git is not available
    }

    // Simple which command implementation
    fn which_command(tool: &str) -> Option<String> {
        use std::process::Command;

        let output = Command::new("which").arg(tool).output().ok()?;

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
        None
    }

    #[test]
    fn verify_test_utilities() {
        // Create a test project using our test utilities
        let (_temp_dir, project_path) = TestProjectBuilder::new()
            .with_dockerfile()
            .with_dotenv()
            .build();

        // Verify the project was created correctly with our test utilities
        assert!(project_path.join("rushd.yaml").exists());
        assert!(project_path.join("Dockerfile").exists());
        assert!(project_path.join(".env").exists());
    }
}
