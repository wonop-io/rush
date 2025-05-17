use log::error;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

pub fn load_dotenv(path: &Path) -> Result<HashMap<String, String>, std::io::Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut env_map = HashMap::new();
    let mut lines_iter = reader.lines().peekable();

    while let Some(Ok(line)) = lines_iter.next() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split the line into key and value
        if let Some((key, mut value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value_str = value.trim().to_string();
            
            // Handle quoted values
            if value_str.starts_with('"') && !value_str.ends_with('"') {
                // This is a multi-line value
                let mut full_value = value_str[1..].to_string();
                
                // Keep reading lines until we find the closing quote
                while let Some(Ok(next_line)) = lines_iter.next() {
                    full_value.push('\n');
                    full_value.push_str(&next_line);
                    if next_line.trim().ends_with('"') {
                        break;
                    }
                }
                
                // Remove the last quote
                if full_value.ends_with('"') {
                    full_value.pop();
                }
                
                env_map.insert(key, full_value);
            } else if value_str.starts_with('"') && value_str.ends_with('"') {
                // Single-line quoted value
                env_map.insert(key, value_str[1..value_str.len() - 1].to_string());
            } else {
                // Unquoted value
                env_map.insert(key, value_str);
            }
        }
    }

    Ok(env_map)
}

pub fn save_dotenv(path: &Path, env_map: HashMap<String, String>) -> Result<(), std::io::Error> {
    let mut file = match File::create(path) {
        Ok(file) => file,
        Err(e) => {
            error!("Failed to create dotenv file '{}': {}", path.display(), e);
            return Err(e);
        }
    };

    for (key, value) in env_map {
        match writeln!(file, "{}=\"{}\"", key, value) {
            Ok(_) => (),
            Err(e) => {
                error!("Failed to write to dotenv file '{}': {}", path.display(), e);
                return Err(e);
            }
        }
    }

    Ok(())
}
