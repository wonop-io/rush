use log::{debug, error, trace};
use std::io;
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Executes a command and returns the output
///
/// # Arguments
///
/// * `command` - The command to execute
/// * `args` - The arguments to pass to the command
/// * `working_dir` - Optional working directory
///
/// # Returns
///
/// The command output if successful
pub fn execute_command(
    command: &str,
    args: &[&str],
    working_dir: Option<&Path>,
) -> io::Result<Output> {
    trace!("Executing command: {} with args: {:?}", command, args);

    let mut cmd = Command::new(command);
    cmd.args(args);

    if let Some(dir) = working_dir {
        debug!("Using working directory: {}", dir.display());
        cmd.current_dir(dir);
    }

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Command execution failed: {}", stderr);
    } else {
        trace!("Command executed successfully");
    }

    Ok(output)
}

/// Runs a command and captures its output as a string
///
/// # Arguments
///
/// * `command` - The command to execute
/// * `args` - The arguments to pass to the command
/// * `working_dir` - Optional working directory
///
/// # Returns
///
/// The command output as a string if successful
pub fn get_command_output(
    command: &str,
    args: &[&str],
    working_dir: Option<&Path>,
) -> io::Result<String> {
    let output = execute_command(command, args, working_dir)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Executes a command in the background
///
/// # Arguments
///
/// * `command` - The command to execute
/// * `args` - The arguments to pass to the command
/// * `working_dir` - Optional working directory
///
/// # Returns
///
/// The child process handle
pub fn spawn_process(
    command: &str,
    args: &[&str],
    working_dir: Option<&Path>,
) -> io::Result<std::process::Child> {
    trace!("Spawning process: {} with args: {:?}", command, args);

    let mut cmd = Command::new(command);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

    if let Some(dir) = working_dir {
        debug!("Using working directory: {}", dir.display());
        cmd.current_dir(dir);
    }

    let child = cmd.spawn()?;
    trace!("Process spawned with PID: {:?}", child.id());

    Ok(child)
}

/// Checks if a command exists in the system PATH
///
/// # Arguments
///
/// * `command` - The command to check
///
/// # Returns
///
/// True if the command exists, false otherwise
pub fn command_exists(command: &str) -> bool {
    let cmd_check = format!("command -v {} >/dev/null 2>&1", command);

    let args = if cfg!(target_os = "windows") {
        vec!["/C", "where", command]
    } else {
        vec!["-c", &cmd_check]
    };

    let shell = if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "sh"
    };

    match Command::new(shell).args(&args).status() {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}
