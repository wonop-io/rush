use log::error;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

pub fn load_dotenv(path: &Path) -> Result<HashMap<String, String>, std::io::Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut env_map = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split the line into key and value
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if value.starts_with('"') && value.ends_with('"') {
                env_map.insert(key, value[1..value.len() - 1].to_string());
            } else {
                env_map.insert(key, value);
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
