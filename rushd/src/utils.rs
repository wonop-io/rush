use std::{
    sync::{mpsc::{self, Receiver, Sender}}
};

use colored::{Colorize};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::str;
use tokio::{
    io::{self, AsyncBufReadExt},
    process::Command as TokioCommand,
};
use tokio::io::AsyncRead;
use colored::ColoredString;
use std::path::Path;


pub struct DockerCrossCompileGuard {
    cross_container_opts: Option<String>,
    docker_default_platform: Option<String>,
    target: String
}

impl DockerCrossCompileGuard {
    pub fn new(target: &str) -> Self {
        let cross_container_opts = match env::var("CROSS_CONTAINER_OPTS") {
            Ok(val) => Some(val),
            Err(e) => None,
        };
        let docker_default_platform = match env::var("DOCKER_DEFAULT_PLATFORM") {
            Ok(val) => Some(val),
            Err(e) => None,
        };

        // Set default Docker and Kubernetes target platforms
        env::set_var("CROSS_CONTAINER_OPTS", &format!("--platform {}", target));
        env::set_var("DOCKER_DEFAULT_PLATFORM", &target);

        DockerCrossCompileGuard { cross_container_opts, docker_default_platform, target: target.to_string() }
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}

impl Drop for DockerCrossCompileGuard {
    fn drop(&mut self) {
        match &self.cross_container_opts {
            Some(v) => env::set_var("CROSS_CONTAINER_OPTS", v),
            None => env::remove_var("CROSS_CONTAINER_OPTS"),
        }
        match &self.docker_default_platform {
            Some(v) => env::set_var("DOCKER_DEFAULT_PLATFORM", v),
            None => env::remove_var("DOCKER_DEFAULT_PLATFORM"),
        }
    }
}


pub struct Directory {
    previous: PathBuf,
}

impl Directory {
    pub fn chdir(dir: &str) -> Self {
        let previous = env::current_dir().expect("Failed to get current directory");
        env::set_current_dir(dir).expect(&format!("Failed to set current directory to {}", dir));
        Directory { previous }
    }

    pub fn chpath(dir: &Path) -> Self {
        let previous = env::current_dir().expect("Failed to get current directory");
        env::set_current_dir(dir).expect(&format!("Failed to set current directory to {}", dir.display()));
        Directory { previous }
    }    
}

impl Drop for Directory {
    fn drop(&mut self) {
        env::set_current_dir(self.previous.clone()).expect("Failed to set current directory to previous");
    }
}


pub fn which(tool: &str) -> Option<String> {
    let which_output = match Command::new("which")
        .args(&[tool])
        .output()
        .map_err(|e| e.to_string()) {
            Ok(output) => output,
            Err(_) => return None,
        };

    let which = match std::str::from_utf8(&which_output.stdout).map_err(|e| e.to_string()) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return None,
    };

    if !which_output.status.success() || which.is_empty() {
        None
    } else {
        Some(which)
    }

}

pub fn first_which(candidates: Vec<&str>) -> Option<String> {
    for candidate in &candidates {
        if let Some(path) = which(candidate) {
            return Some(path);
        }
    }
    None
}

pub fn resolve_toolchain_path(path: &str, tool: &str) -> Option<String> {
    let read_dir = match std::fs::read_dir(path) {
        Ok(read_dir) => read_dir,
        Err(_) => return None,
    };
    read_dir.filter_map(|entry| entry.ok())
                            .find(|entry| entry.file_name().to_string_lossy().contains(tool))
                            .map(|entry| entry.path().to_string_lossy().into_owned())
}


pub async fn handle_stream<R: AsyncRead + Unpin>(
    reader: R,
    sender: Sender<String>,
) {
    let mut reader = io::BufReader::new(reader);
    let mut line = String::new();
    // let mut lines_in_window = Vec::with_capacity(10);

    while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
        if !line.trim().is_empty() {
            let parts = line.split('\r');
            let line = parts.last().unwrap_or(&line);
            sender.send(line.to_string()).unwrap_or_else(|e| {
                eprintln!("Failed to send line to channel: {}", e);
            });
        }
        line.clear();
    }
}

pub async fn run_command_in_window(window_size: usize, formatted_label: &str, command: &str, args: Vec<&str>) -> Result<(), String> {

    // Creating a clear space for the window 
    for _ in 0..=window_size {
        println!("");
    }

    let debug_args = args.join(" ");
    // Settting process up
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
    let mut child = TokioCommand::new(command)
        .args(args)
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
        // println!("Received line: {}", line.trim_end());
        lines.push(line.trim_end().to_string());
        
        // Printing the last ten lines
        let skip = if lines.len() < window_size {
            0
        } else {
            lines.len() - window_size
        };

        lines_in_window = lines.iter().skip(skip).map(|line| line.clone()).collect::<Vec<_>>();        
        print!("{}", format!("\r\x1B[{}A", lines_in_window.len()));
        for line in lines_in_window.iter() {
           let clean_line = line.trim_end().replace("\x1B", "").replace("\r", "").replace("\n","");           
           println!("       {}  |   {}", formatted_label.bold().color("white"),  clean_line);
        }        
    }

    let _ = tokio::join!(stdout_task, stderr_task);

    drop(rx); // Close the channel by dropping the receiver

    lines.insert(0, format!("---"));
    lines.insert(0, format!("Command: {} {}", command, debug_args));
    lines.insert(0, format!("Working directory: {}", env::current_dir().expect("Failed to get current directory").display()));

    print!("{}", format!("\r\x1B[{}A", lines_in_window.len()));
    for _ in lines_in_window.iter() {
         println!("{}", format!("\r\x1B[2K"));
    }
    print!("{}", format!("\r\x1B[{}A", lines_in_window.len()+1));
    print!("{}", format!("\x1B[?7h"));
    if let Some(code) = child.wait().await.unwrap().code() {
        if code != 0 {
            Err(lines.join("\n"))
        } else {
            Ok(())
        }
    } else {
        Err(lines.join("\n"))
    }
}


pub async fn run_command(formatted_label: ColoredString, command: &str, args: Vec<&str>) -> Result<(), String> {
    let debug_args = args.join(" ");
    // Settting process up
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();
    let mut child = TokioCommand::new(command)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to execute host command");

    let (stdout, stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());

    let stdout_task = tokio::spawn(handle_stream(stdout,  tx.clone()));
    let stderr_task = tokio::spawn(handle_stream(stderr,  tx));

    let mut lines = Vec::new();
    while let Ok(line) = rx.recv() {
        lines.push(line.trim_end().to_string());        
        let clean_line = line.trim_end().replace("\x1B", "").replace("\r", "").replace("\n","");           
        println!("       {}  |   {}", formatted_label,  clean_line);
    }

    let _ = tokio::join!(stdout_task, stderr_task);
    drop(rx);

    lines.insert(0, format!("---"));
    lines.insert(0, format!("Command: {} {}", command, debug_args));
    lines.insert(0, format!("Working directory: {}", env::current_dir().expect("Failed to get current directory").display()));

    if let Some(code) = child.wait().await.unwrap().code() {
        if code != 0 {
            Err(lines.join("\n"))
        } else {
            Ok(())
        }
    } else {
        Err(lines.join("\n"))
    }
}


