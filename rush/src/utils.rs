use std::sync::mpsc::{self, Receiver, Sender};

use colored::ColoredString;
use colored::Colorize;
use log::{debug, error, info, trace, warn};
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::str;
use tokio::io::AsyncRead;
use tokio::{
    io::{self, AsyncBufReadExt},
    process::Command as TokioCommand,
};

pub struct DockerCrossCompileGuard {
    cross_container_opts: Option<String>,
    docker_default_platform: Option<String>,
    target: String,
}

impl DockerCrossCompileGuard {
    pub fn new(target: &str) -> Self {
        debug!(
            "Creating new DockerCrossCompileGuard with target: {}",
            target
        );
        let cross_container_opts = match env::var("CROSS_CONTAINER_OPTS") {
            Ok(val) => {
                debug!("Found existing CROSS_CONTAINER_OPTS: {}", val);
                Some(val)
            }
            Err(_) => {
                debug!("No existing CROSS_CONTAINER_OPTS found");
                None
            }
        };
        let docker_default_platform = match env::var("DOCKER_DEFAULT_PLATFORM") {
            Ok(val) => {
                debug!("Found existing DOCKER_DEFAULT_PLATFORM: {}", val);
                Some(val)
            }
            Err(_) => {
                debug!("No existing DOCKER_DEFAULT_PLATFORM found");
                None
            }
        };

        // Set default Docker and Kubernetes target platforms
        env::set_var("CROSS_CONTAINER_OPTS", format!("--platform {}", target));
        env::set_var("DOCKER_DEFAULT_PLATFORM", target);
        trace!(
            "Set CROSS_CONTAINER_OPTS and DOCKER_DEFAULT_PLATFORM to {}",
            target
        );

        DockerCrossCompileGuard {
            cross_container_opts,
            docker_default_platform,
            target: target.to_string(),
        }
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}

impl Drop for DockerCrossCompileGuard {
    fn drop(&mut self) {
        debug!("Dropping DockerCrossCompileGuard");
        match &self.cross_container_opts {
            Some(v) => {
                env::set_var("CROSS_CONTAINER_OPTS", v);
                debug!("Restored CROSS_CONTAINER_OPTS to: {}", v);
            }
            None => {
                env::remove_var("CROSS_CONTAINER_OPTS");
                debug!("Removed CROSS_CONTAINER_OPTS");
            }
        }
        match &self.docker_default_platform {
            Some(v) => {
                env::set_var("DOCKER_DEFAULT_PLATFORM", v);
                debug!("Restored DOCKER_DEFAULT_PLATFORM to: {}", v);
            }
            None => {
                env::remove_var("DOCKER_DEFAULT_PLATFORM");
                debug!("Removed DOCKER_DEFAULT_PLATFORM");
            }
        }
    }
}

pub struct Directory {
    previous: PathBuf,
}

impl Directory {
    pub fn chdir(dir: &str) -> Self {
        trace!("Changing directory to: {}", dir);
        let previous = env::current_dir().expect("Failed to get current directory");
        debug!("Previous directory: {:?}", previous);
        env::set_current_dir(dir)
            .unwrap_or_else(|_| panic!("Failed to set current directory to {}", dir));
        Directory { previous }
    }

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
        env::set_current_dir(self.previous.clone())
            .expect("Failed to set current directory to previous");
    }
}

pub fn which(tool: &str) -> Option<String> {
    debug!("Searching for tool: {}", tool);
    let which_output = match Command::new("which")
        .args([tool])
        .output()
        .map_err(|e| e.to_string())
    {
        Ok(output) => output,
        Err(e) => {
            warn!("Failed to execute 'which' command: {}", e);
            return None;
        }
    };

    let which = match std::str::from_utf8(&which_output.stdout).map_err(|e| e.to_string()) {
        Ok(s) => s.trim().to_string(),
        Err(e) => {
            warn!("Failed to parse 'which' output: {}", e);
            return None;
        }
    };

    if !which_output.status.success() || which.is_empty() {
        debug!("Tool '{}' not found", tool);
        None
    } else {
        trace!("Found tool '{}' at path: {}", tool, which);
        Some(which)
    }
}

pub fn first_which(candidates: Vec<&str>) -> Option<String> {
    debug!("Searching for first available tool among: {:?}", candidates);
    for candidate in &candidates {
        if let Some(path) = which(candidate) {
            trace!(
                "Found first available tool '{}' at path: {}",
                candidate,
                path
            );
            return Some(path);
        }
    }
    warn!("None of the candidate tools were found");
    None
}

pub fn resolve_toolchain_path(path: &str, tool: &str) -> Option<String> {
    debug!(
        "Resolving toolchain path for '{}' in directory: {}",
        tool, path
    );
    let read_dir = match std::fs::read_dir(path) {
        Ok(read_dir) => read_dir,
        Err(e) => {
            warn!("Failed to read directory '{}': {}", path, e);
            return None;
        }
    };
    let result = read_dir
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.file_name().to_string_lossy().contains(tool))
        .map(|entry| entry.path().to_string_lossy().into_owned());
    match &result {
        Some(path) => trace!("Resolved toolchain path for '{}': {}", tool, path),
        None => warn!("Failed to resolve toolchain path for '{}'", tool),
    }
    result
}

pub async fn handle_stream<R: AsyncRead + Unpin>(reader: R, sender: Sender<String>) {
    let mut reader = io::BufReader::new(reader);
    let mut line = String::new();

    loop {
        match reader.read_line(&mut line).await {
            Ok(0) => {
                debug!("Reached end of stream");
                break;
            }
            Ok(n) if n > 0 => {
                debug!("Read {} bytes", n);
                if !line.trim().is_empty() {
                    let parts = line.split('\r');
                    let line = parts.last().unwrap_or(&line);
                    sender.send(line.to_string()).unwrap_or_else(|e| {
                        error!("Failed to send line to channel: {}", e);
                    });
                }
                line.clear();
            }
            Ok(_) => {
                debug!("Read 0 bytes");
                // No bytes read, but not end of stream
                tokio::task::yield_now().await;
                continue;
            }
            Err(e) => {
                error!("Error reading line: {}", e);
                break;
            }
        }

        tokio::task::yield_now().await;
    }
}

pub async fn run_command_in_window(
    window_size: usize,
    formatted_label: &str,
    command: &str,
    args: Vec<&str>,
) -> Result<String, String> {
    let debug_args = args.join(" ");
    trace!("Running command in window: {} {}", command, debug_args);

    // Creating a clear space for the window
    for _ in 0..=window_size {
        println!();
    }

    let debug_args = args.join(" ");
    // Settting process up
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
    let mut child = TokioCommand::new(command)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to execute host command");

    let formatted_label = formatted_label.to_string();
    let (stdout, stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());

    let stdout_task = tokio::spawn(handle_stream(stdout, tx.clone()));
    let stderr_task = tokio::spawn(handle_stream(stderr, tx));

    let mut lines = Vec::new();
    let mut lines_in_window = Vec::new();
    print!("{}", format!("\x1B[?7l"));
    while let Ok(line) = rx.recv() {
        trace!("Received line: {}", line.trim_end());
        lines.push(line.trim_end().to_string());

        // Printing the last ten lines
        let skip = if lines.len() < window_size {
            0
        } else {
            lines.len() - window_size
        };

        lines_in_window = lines.iter().skip(skip).cloned().collect::<Vec<_>>();
        print!("{}", format!("\r\x1B[{}A", lines_in_window.len()));
        for line in lines_in_window.iter() {
            let clean_line = line.trim_end().replace(['\x1B', '\r', '\n'], "");
            println!(
                "       {}  |   {}",
                formatted_label.bold().color("white"),
                clean_line
            );
        }
    }

    let _ = tokio::join!(stdout_task, stderr_task);

    drop(rx); // Close the channel by dropping the receiver
    let output = lines.join("\n");
    lines.insert(0, "---".to_string());
    lines.insert(0, format!("Command: {} {}", command, debug_args));
    lines.insert(
        0,
        format!(
            "Working directory: {}",
            env::current_dir()
                .expect("Failed to get current directory")
                .display()
        ),
    );

    print!("{}", format!("\r\x1B[{}A", lines_in_window.len()));
    for _ in lines_in_window.iter() {
        println!("{}", format!("\r\x1B[2K"));
    }
    print!("{}", format!("\r\x1B[{}A", lines_in_window.len() + 1));
    print!("{}", format!("\x1B[?7h"));
    if let Some(code) = child.wait().await.unwrap().code() {
        if code != 0 {
            error!("Command failed with exit code: {}", code);
            Err(lines.join("\n"))
        } else {
            trace!("Command completed successfully");
            Ok(output)
        }
    } else {
        error!("Command was terminated by a signal");
        Err(lines.join("\n"))
    }
}

pub async fn run_command(
    formatted_label: ColoredString,
    command: &str,
    args: Vec<&str>,
) -> Result<String, String> {
    let debug_args = args.join(" ");
    trace!("Running command: {} {}", command, debug_args);

    // Settting process up
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
    let mut child = TokioCommand::new(command)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to execute host command");

    let (stdout, stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());

    let stdout_task = tokio::spawn(handle_stream(stdout, tx.clone()));
    let stderr_task = tokio::spawn(handle_stream(stderr, tx));

    let mut lines = Vec::new();
    while let Ok(line) = rx.recv() {
        trace!("Received line: {}", line.trim_end());
        lines.push(line.trim_end().to_string());
        let clean_line = line.trim_end().replace(['\x1B', '\r', '\n'], "");
        println!("       {}  |   {}", formatted_label, clean_line);
    }

    let _ = tokio::join!(stdout_task, stderr_task);
    drop(rx);
    let output = lines.join("\n");
    lines.insert(0, "---".to_string());
    lines.insert(0, format!("Command: {} {}", command, debug_args));
    lines.insert(
        0,
        format!(
            "Working directory: {}",
            env::current_dir()
                .expect("Failed to get current directory")
                .display()
        ),
    );

    if let Some(code) = child.wait().await.unwrap().code() {
        if code != 0 {
            error!("Command failed with exit code: {}", code);
            Err(lines.join("\n"))
        } else {
            trace!("Command completed successfully");
            Ok(output)
        }
    } else {
        error!("Command was terminated by a signal");
        Err(lines.join("\n"))
    }
}
