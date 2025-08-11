use log::{debug, trace};
use std::env;
use std::path::{Path, PathBuf};

/// RAII guard for safely changing directories.
///
/// When this struct is created, it changes the current working directory
/// to the specified path and stores the previous directory. When the struct
/// is dropped (goes out of scope), it automatically restores the previous
/// directory.
///
/// This ensures that directory changes are always properly cleaned up,
/// even in the case of panics or early returns.
///
/// # Examples
///
/// ```rust
/// use rush_cli::utils::Directory;
///
/// // The directory is changed when the guard is created
/// {
///     let _guard = Directory::chdir("/tmp");
///     // Current directory is now /tmp
///     // ... do work in /tmp ...
/// } // Directory is automatically restored when _guard goes out of scope
/// ```
pub struct Directory {
    previous: PathBuf,
}

impl Directory {
    /// Changes the current directory to the specified path string.
    ///
    /// Returns a guard that will restore the previous directory when dropped.
    ///
    /// # Arguments
    ///
    /// * `dir` - The directory path to change to
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - Failed to get the current directory
    /// - Failed to change to the specified directory
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rush_cli::utils::Directory;
    /// let _guard = Directory::chdir("/tmp");
    /// // Work in /tmp
    /// // Directory is restored when _guard is dropped
    /// ```
    pub fn chdir(dir: &str) -> Self {
        trace!("Changing directory to: {}", dir);
        let previous = env::current_dir().expect("Failed to get current directory");
        debug!("Previous directory: {:?}", previous);
        env::set_current_dir(dir)
            .unwrap_or_else(|_| panic!("Failed to set current directory to {}", dir));
        Directory { previous }
    }

    /// Changes the current directory to the specified Path.
    ///
    /// Returns a guard that will restore the previous directory when dropped.
    /// This is the same as `chdir` but accepts a `Path` instead of a string.
    ///
    /// # Arguments
    ///
    /// * `dir` - The directory path to change to
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - Failed to get the current directory  
    /// - Failed to change to the specified directory
    ///
    /// # Examples
    ///
    /// ```rust
    /// use rush_cli::utils::Directory;
    /// use std::path::Path;
    /// let path = Path::new("/tmp");
    /// let _guard = Directory::chpath(path);
    /// // Work in /tmp
    /// // Directory is restored when _guard is dropped
    /// ```
    pub fn chpath(dir: &Path) -> Self {
        trace!("Changing directory to: {:?}", dir);
        let previous = env::current_dir().expect("Failed to get current directory");
        debug!("Previous directory: {:?}", previous);
        env::set_current_dir(dir)
            .unwrap_or_else(|_| panic!("Failed to set current directory to {}", dir.display()));
        Directory { previous }
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        trace!("Restoring previous directory: {:?}", self.previous);
        env::set_current_dir(&self.previous).expect("Failed to restore previous directory");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::tempdir;

    /// Helper to compare current directory with expected, handling macOS canonicalization
    fn assert_current_dir_eq(expected: impl AsRef<Path>) {
        let current = env::current_dir().unwrap().canonicalize().unwrap();
        let expected = expected.as_ref().canonicalize().unwrap();
        assert_eq!(current, expected);
    }

    /// Helper to save and restore directory for test isolation
    struct TestGuard {
        original_dir: PathBuf,
    }

    impl TestGuard {
        fn new() -> Self {
            TestGuard {
                original_dir: env::current_dir().unwrap(),
            }
        }
    }

    impl Drop for TestGuard {
        fn drop(&mut self) {
            // Always try to restore the original directory
            let _ = env::set_current_dir(&self.original_dir);
        }
    }

    #[test]
    #[serial]
    fn test_directory_change_and_restore() {
        let _test_guard = TestGuard::new();
        let temp_dir = tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();

        // Use the Directory guard to change directory
        {
            let _guard = Directory::chdir(temp_dir.path().to_str().unwrap());
            assert_current_dir_eq(temp_dir.path());
        }

        // After the guard is dropped, we should be back to the original directory
        assert_current_dir_eq(&original_dir);
    }

    #[test]
    #[serial]
    fn test_directory_change_with_path() {
        let _test_guard = TestGuard::new();
        let temp_dir = tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();

        // Use the Directory guard to change directory using Path
        {
            let _guard = Directory::chpath(temp_dir.path());
            assert_current_dir_eq(temp_dir.path());
        }

        // After the guard is dropped, we should be back to the original directory
        assert_current_dir_eq(&original_dir);
    }

    #[test]
    #[serial]
    fn test_nested_directory_changes() {
        let _test_guard = TestGuard::new();
        let temp_dir1 = tempdir().unwrap();
        let temp_dir2 = tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();

        // Keep paths alive
        let path1 = temp_dir1.path().to_path_buf();
        let path2 = temp_dir2.path().to_path_buf();

        // Test nested directory changes
        {
            let _guard1 = Directory::chpath(&path1);
            assert_current_dir_eq(&path1);

            {
                let _guard2 = Directory::chpath(&path2);
                assert_current_dir_eq(&path2);
            }

            // After inner guard is dropped, we should be back to temp_dir1
            assert_current_dir_eq(&path1);
        }

        // After all guards are dropped, we should be back to the original directory
        assert_current_dir_eq(&original_dir);

        // Keep temp dirs alive until end of test
        drop(temp_dir1);
        drop(temp_dir2);
    }

    #[test]
    #[serial]
    #[should_panic(expected = "Failed to set current directory")]
    fn test_invalid_directory_panics() {
        // Try to change to a non-existent directory
        let _guard = Directory::chdir("/this/path/does/not/exist/at/all");
    }

    #[test]
    #[serial]
    fn test_directory_restored_on_panic() {
        let _test_guard = TestGuard::new();
        let temp_dir = tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Use catch_unwind to test panic safety
        let result = std::panic::catch_unwind(|| {
            let _guard = Directory::chpath(&temp_path);
            assert_current_dir_eq(&temp_path);
            panic!("Test panic");
        });

        assert!(result.is_err());
        // Directory should be restored even after panic
        assert_current_dir_eq(&original_dir);
    }

    #[test]
    #[serial]
    fn test_multiple_sequential_changes() {
        let _test_guard = TestGuard::new();
        let temp_dir1 = tempdir().unwrap();
        let temp_dir2 = tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();

        // Keep paths alive
        let path1 = temp_dir1.path().to_path_buf();
        let path2 = temp_dir2.path().to_path_buf();

        // First change
        {
            let _guard = Directory::chpath(&path1);
            assert_current_dir_eq(&path1);
        }
        assert_current_dir_eq(&original_dir);

        // Second change
        {
            let _guard = Directory::chpath(&path2);
            assert_current_dir_eq(&path2);
        }
        assert_current_dir_eq(&original_dir);

        // Keep temp dirs alive until end of test
        drop(temp_dir1);
        drop(temp_dir2);
    }

    #[test]
    #[serial]
    fn test_create_and_change_directory() {
        let _test_guard = TestGuard::new();
        let temp_base = tempdir().unwrap();
        let new_dir = temp_base.path().join("new_directory");
        let original_dir = env::current_dir().unwrap();

        // Create a new directory
        fs::create_dir(&new_dir).unwrap();

        // Change to the newly created directory
        {
            let _guard = Directory::chpath(&new_dir);
            assert_current_dir_eq(&new_dir);

            // Verify we can create files in the new directory
            fs::write("test_file.txt", b"test content").unwrap();
            assert!(Path::new("test_file.txt").exists());
        }

        // After guard is dropped, we should be back to the original directory
        assert_current_dir_eq(&original_dir);
        // And the file should not exist in the original directory
        assert!(!Path::new("test_file.txt").exists());

        // Keep temp dir alive until end of test
        drop(temp_base);
    }
}
