use std::env;
use std::path::{Path, PathBuf};

/// Expands a path that may contain environment variables or the tilde character
/// for the user's home directory.
///
/// # Arguments
///
/// * `path` - A string slice containing the path to expand
///
/// # Returns
///
/// A PathBuf with all expansions performed
///
/// # Examples
///
/// ```
/// use rush_cli::utils::expand_path;
///
/// let expanded = expand_path("~/projects");
/// // This will expand to the user's home directory + "/projects"
///
/// let expanded = expand_path("$HOME/projects");
/// // This will also expand to the user's home directory + "/projects"
/// ```
pub fn expand_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let path_str = path.as_ref().to_string_lossy();

    // Handle tilde expansion for home directory
    if path_str.starts_with("~/") {
        if let Some(home_dir) = dirs::home_dir() {
            return home_dir.join(&path_str[2..]);
        }
    }

    // Handle environment variable expansion
    let mut result = path_str.to_string();
    if result.contains('$') {
        let mut start_idx = 0;
        while let Some(idx) = result[start_idx..].find('$') {
            let real_idx = start_idx + idx;
            let var_name_end = result[real_idx + 1..]
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .map_or(result.len(), |pos| real_idx + 1 + pos);

            if real_idx + 1 < var_name_end {
                let var_name = &result[real_idx + 1..var_name_end];
                if let Ok(var_value) = env::var(var_name) {
                    result.replace_range(real_idx..var_name_end, &var_value);
                    start_idx = real_idx + var_value.len();
                } else {
                    start_idx = var_name_end;
                }
            } else {
                start_idx = real_idx + 1;
            }
        }
    }

    PathBuf::from(result)
}

/// Makes a path absolute by resolving it against the current working directory
/// if it's relative.
///
/// # Arguments
///
/// * `path` - The path to make absolute
///
/// # Returns
///
/// An absolute PathBuf
pub fn absolute_path<P: AsRef<Path>>(path: P) -> std::io::Result<PathBuf> {
    let path = path.as_ref();
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        env::current_dir().map(|cwd| cwd.join(path))
    }
}

/// Converts a path to a canonical, absolute form by expanding
/// environment variables, resolving against the current directory
/// if necessary, and resolving all symbolic links.
///
/// # Arguments
///
/// * `path` - The path to canonicalize
///
/// # Returns
///
/// A canonical, absolute PathBuf or an error if canonicalization fails
pub fn canonical_path<P: AsRef<Path>>(path: P) -> std::io::Result<PathBuf> {
    let expanded = expand_path(path);
    let absolute = absolute_path(expanded)?;
    absolute.canonicalize()
}

/// Finds a parent directory matching a predicate by walking up
/// the directory tree from the given path.
///
/// # Arguments
///
/// * `start_path` - The path to start from
/// * `predicate` - A function that returns true if the directory matches the criteria
///
/// # Returns
///
/// The first matching directory, or None if no match is found
pub fn find_dir_recursively<P, F>(start_path: P, predicate: F) -> Option<PathBuf>
where
    P: AsRef<Path>,
    F: Fn(&Path) -> bool,
{
    let path = absolute_path(start_path).ok()?;
    let mut current = path.as_path();

    if predicate(current) {
        return Some(current.to_path_buf());
    }

    while let Some(parent) = current.parent() {
        if predicate(parent) {
            return Some(parent.to_path_buf());
        }
        current = parent;
    }

    None
}

/// Finds the project root directory by looking for a .git directory
/// or other common project markers.
///
/// # Arguments
///
/// * `start_path` - The path to start the search from
///
/// # Returns
///
/// The project root directory, or None if not found
pub fn find_project_root<P: AsRef<Path>>(start_path: P) -> Option<PathBuf> {
    find_dir_recursively(start_path, |path| {
        path.join(".git").exists()
            || path.join("Cargo.toml").exists()
            || path.join("package.json").exists()
            || path.join("rush.yaml").exists()
    })
}

/// Gets a path relative to the project root.
///
/// # Arguments
///
/// * `path` - The path to make relative to the project root
///
/// # Returns
///
/// A path relative to the project root, or the original path if
/// the project root cannot be determined or if the path is not
/// within the project.
pub fn path_relative_to_project_root<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = absolute_path(path.as_ref()).unwrap_or_else(|_| path.as_ref().to_path_buf());

    if let Some(project_root) = find_project_root(env::current_dir().unwrap_or_default()) {
        if let Ok(rel_path) = path.strip_prefix(&project_root) {
            return rel_path.to_path_buf();
        }
    }

    path
}
