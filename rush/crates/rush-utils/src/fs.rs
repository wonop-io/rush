use log::{debug, error, trace};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::env;

/// Reads the contents of a file to a string
///
/// # Arguments
///
/// * `path` - Path to the file to read
///
/// # Returns
///
/// Returns a Result containing either the file contents as a String, or an error
pub fn read_to_string<P: AsRef<Path>>(path: P) -> io::Result<String> {
    trace!("Reading file to string: {:?}", path.as_ref());
    let mut file = File::open(&path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    debug!(
        "Successfully read {} bytes from {:?}",
        contents.len(),
        path.as_ref()
    );
    Ok(contents)
}

/// Writes a string to a file
///
/// # Arguments
///
/// * `path` - Path to the file to write
/// * `contents` - String content to write to the file
///
/// # Returns
///
/// Returns a Result indicating success or an error
pub fn write_string<P: AsRef<Path>>(path: P, contents: &str) -> io::Result<()> {
    trace!("Writing string to file: {:?}", path.as_ref());
    let mut file = File::create(&path)?;
    file.write_all(contents.as_bytes())?;
    debug!(
        "Successfully wrote {} bytes to {:?}",
        contents.len(),
        path.as_ref()
    );
    Ok(())
}

/// Creates a directory and all parent directories if they don't exist
///
/// # Arguments
///
/// * `path` - Path to the directory to create
///
/// # Returns
///
/// Returns a Result indicating success or an error
pub fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    trace!("Creating directory and parents: {:?}", path.as_ref());
    fs::create_dir_all(&path)?;
    debug!("Successfully created directory: {:?}", path.as_ref());
    Ok(())
}

/// Removes a file
///
/// # Arguments
///
/// * `path` - Path to the file to remove
///
/// # Returns
///
/// Returns a Result indicating success or an error
pub fn remove_file<P: AsRef<Path>>(path: P) -> io::Result<()> {
    trace!("Removing file: {:?}", path.as_ref());
    fs::remove_file(&path)?;
    debug!("Successfully removed file: {:?}", path.as_ref());
    Ok(())
}

/// Removes a directory and all its contents
///
/// # Arguments
///
/// * `path` - Path to the directory to remove
///
/// # Returns
///
/// Returns a Result indicating success or an error
pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    trace!("Removing directory and contents: {:?}", path.as_ref());
    fs::remove_dir_all(&path)?;
    debug!("Successfully removed directory: {:?}", path.as_ref());
    Ok(())
}

/// Checks if a path exists
///
/// # Arguments
///
/// * `path` - Path to check
///
/// # Returns
///
/// Returns a boolean indicating if the path exists
pub fn path_exists<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().exists()
}

/// Copies a file from source to destination
///
/// # Arguments
///
/// * `from` - Source path
/// * `to` - Destination path
///
/// # Returns
///
/// Returns a Result containing the number of bytes copied, or an error
pub fn copy_file<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<u64> {
    trace!("Copying file from {:?} to {:?}", from.as_ref(), to.as_ref());
    let bytes_copied = fs::copy(&from, &to)?;
    debug!("Successfully copied {} bytes", bytes_copied);
    Ok(bytes_copied)
}

/// Recursively copies a directory and its contents
///
/// # Arguments
///
/// * `from` - Source directory path
/// * `to` - Destination directory path
///
/// # Returns
///
/// Returns a Result indicating success or an error
pub fn copy_dir_all<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> io::Result<()> {
    trace!(
        "Copying directory from {:?} to {:?}",
        from.as_ref(),
        to.as_ref()
    );

    let from_path = from.as_ref();
    let to_path = to.as_ref();

    if !from_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Source directory does not exist: {from_path:?}"),
        ));
    }

    if !from_path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Source is not a directory: {from_path:?}"),
        ));
    }

    if !to_path.exists() {
        fs::create_dir_all(to_path)?;
    }

    for entry in fs::read_dir(from_path)? {
        let entry = entry?;
        let from_entry_path = entry.path();
        let to_entry_path = to_path.join(entry.file_name());

        if from_entry_path.is_dir() {
            copy_dir_all(&from_entry_path, &to_entry_path)?;
        } else {
            fs::copy(&from_entry_path, &to_entry_path)?;
        }
    }

    debug!("Successfully copied directory");
    Ok(())
}

/// Gets the canonical, absolute form of a path with all intermediate components normalized
///
/// # Arguments
///
/// * `path` - Path to canonicalize
///
/// # Returns
///
/// Returns a Result containing the canonicalized path, or an error
pub fn canonicalize<P: AsRef<Path>>(path: P) -> io::Result<PathBuf> {
    trace!("Canonicalizing path: {:?}", path.as_ref());
    let canonical = fs::canonicalize(&path)?;
    debug!("Canonicalized path: {:?}", canonical);
    Ok(canonical)
}

/// Safely reads a file to a string, providing a default value if the file doesn't exist
///
/// # Arguments
///
/// * `path` - Path to the file to read
/// * `default` - Default value to return if file doesn't exist
///
/// # Returns
///
/// The file contents as a String, or the default value
pub fn read_to_string_or_default<P: AsRef<Path>>(path: P, default: String) -> String {
    trace!("Reading file with default: {:?}", path.as_ref());
    match read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) => {
            error!("Failed to read file {:?}: {}", path.as_ref(), e);
            default
        }
    }
}

/// Find the project root by looking for specific marker files
///
/// Searches upward from the current directory for files that indicate
/// a project root (rush.yaml, .git, Cargo.toml, package.json, etc.)
///
/// # Returns
///
/// Returns an Option containing the project root path if found
pub fn find_project_root() -> Option<PathBuf> {
    let current_dir = env::current_dir().ok()?;
    let mut path = current_dir.as_path();
    
    loop {
        // Check for various project root indicators
        if path.join("rush.yaml").exists() 
            || path.join("rush.yml").exists()
            || path.join(".git").exists()
            || path.join("Cargo.toml").exists()
            || path.join("package.json").exists() {
            return Some(path.to_path_buf());
        }
        
        // Move up to parent directory
        path = path.parent()?;
    }
}
