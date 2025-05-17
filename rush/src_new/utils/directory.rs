use log::{debug, trace};
use std::env;
use std::path::{Path, PathBuf};

pub struct Directory {
    previous: PathBuf,
}

impl Directory {
    /// Changes the current directory to the specified path
    /// and returns a guard that will restore the previous directory when dropped
    pub fn chdir(dir: &str) -> Self {
        trace!("Changing directory to: {}", dir);
        let previous = env::current_dir().expect("Failed to get current directory");
        debug!("Previous directory: {:?}", previous);
        env::set_current_dir(dir)
            .unwrap_or_else(|_| panic!("Failed to set current directory to {}", dir));
        Directory { previous }
    }

    /// Changes the current directory to the specified Path
    /// and returns a guard that will restore the previous directory when dropped
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
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_directory_change_and_restore() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();

        // Use the Directory guard to change directory
        {
            let _guard = Directory::chdir(temp_dir.path().to_str().unwrap());
            assert_eq!(env::current_dir().unwrap(), temp_dir.path());
        }

        // After the guard is dropped, we should be back to the original directory
        assert_eq!(env::current_dir().unwrap(), original_dir);
    }

    #[test]
    fn test_directory_change_with_path() {
        // Create a temporary directory for testing
        let temp_dir = tempdir().unwrap();
        let original_dir = env::current_dir().unwrap();

        // Use the Directory guard to change directory using Path
        {
            let _guard = Directory::chpath(temp_dir.path());
            assert_eq!(env::current_dir().unwrap(), temp_dir.path());
        }

        // After the guard is dropped, we should be back to the original directory
        assert_eq!(env::current_dir().unwrap(), original_dir);
    }
}
