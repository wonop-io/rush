use log::debug;
use log::error;
use log::trace;
use tokio::io::{self, AsyncBufReadExt, AsyncRead};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;

/// Executes a command and captures its output
///
/// # Arguments
///
/// * `formatted_label` - A label to display with command output
/// * `command` - The command to execute
/// * `args` - Arguments for the command
///
/// # Returns
///
/// * `Result<String, String>` - The command output or an error message
pub async fn run_command(
    formatted_label: &str,
    command: &str,
    args: Vec<&str>,
) -> Result<String, String> {
    let debug_args = args.join(" ");
    trace!("Running command: {} {}", command, debug_args);

    // Create process
    let mut child = Command::new(command)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to execute command: {e}"))?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    // Create channels for handling output
    let (tx, mut rx) = mpsc::channel(100); // Add buffer size
    let stdout_task = tokio::spawn(handle_stream(stdout, tx.clone()));
    let stderr_task = tokio::spawn(handle_stream(stderr, tx));

    // Collect output
    let mut lines = Vec::new();
    while let Some(line) = rx.recv().await {
        trace!("Received line: {}", line.trim_end());
        lines.push(line.trim_end().to_string());
        let clean_line = line.trim_end().replace(['\x1B', '\r', '\n'], "");
        println!("       {formatted_label}  |   {clean_line}");
    }

    // Wait for streams to complete
    let _ = tokio::join!(stdout_task, stderr_task);

    // Build output
    let output = lines.join("\n");

    // Wait for command to complete
    match child.wait().await {
        Ok(status) => {
            if let Some(code) = status.code() {
                if code != 0 {
                    error!("Command failed with exit code: {}", code);
                    Err(format!("Command failed with exit code: {code}"))
                } else {
                    trace!("Command completed successfully");
                    Ok(output)
                }
            } else {
                error!("Command was terminated by a signal");
                Err("Command was terminated by a signal".to_string())
            }
        }
        Err(e) => {
            error!("Failed to wait for command completion: {}", e);
            Err(format!("Failed to wait for command completion: {e}"))
        }
    }
}

/// Executes a command with output displayed in a scrolling window
///
/// # Arguments
///
/// * `window_size` - Number of lines to show in the window
/// * `formatted_label` - A label to display with command output
/// * `command` - The command to execute
/// * `args` - Arguments for the command
///
/// # Returns
///
/// * `Result<String, String>` - The command output or an error message
pub async fn run_command_in_window(
    window_size: usize,
    formatted_label: &str,
    command: &str,
    args: Vec<&str>,
) -> Result<String, String> {
    let debug_args = args.join(" ");
    trace!("Running command in window: {} {}", command, debug_args);

    // Creating a clear space for the window
    for _ in 0..window_size {
        println!();
    }

    // Setting up process
    let (tx, mut rx): (Sender<String>, Receiver<String>) = mpsc::channel(100); // Add buffer size
    let mut child = Command::new(command)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to execute command: {e}"))?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    let stdout_task = tokio::spawn(handle_stream(stdout, tx.clone()));
    let stderr_task = tokio::spawn(handle_stream(stderr, tx));

    let mut lines = Vec::new();
    let mut lines_in_window: Vec<String> = Vec::new();

    // Disable line wrapping for better output control
    print!("\x1B[?7l");

    while let Some(line) = rx.recv().await {
        trace!("Received line: {}", line.trim_end());
        lines.push(line.trim_end().to_string());

        // Calculate which lines to show in the window
        let skip = if lines.len() < window_size {
            0
        } else {
            lines.len() - window_size
        };

        lines_in_window = lines.iter().skip(skip).cloned().collect::<Vec<_>>();

        // Move cursor up to the beginning of the window
        print!("\r\x1B[{}A", lines_in_window.len());

        // Print each line in the window
        for line in lines_in_window.iter() {
            let clean_line = line.trim_end().replace(['\x1B', '\r', '\n'], "");
            println!("       {formatted_label}  |   {clean_line}");
        }
    }

    // Wait for stream processing to complete
    let _ = tokio::join!(stdout_task, stderr_task);

    // Prepare output
    let output = lines.join("\n");

    // Clean up the window display
    print!("\r\x1B[{}A", lines_in_window.len());
    for _ in lines_in_window.iter() {
        println!("\r\x1B[2K"); // Clear the entire line
    }

    // Reset cursor position and re-enable line wrapping
    print!("\r\x1B[{}A", lines_in_window.len());
    print!("\x1B[?7h");

    // Wait for command to complete
    match child.wait().await {
        Ok(status) => {
            if let Some(code) = status.code() {
                if code != 0 {
                    error!("Command failed with exit code: {}", code);
                    Err(format!("Command failed with exit code: {code}"))
                } else {
                    trace!("Command completed successfully");
                    Ok(output)
                }
            } else {
                error!("Command was terminated by a signal");
                Err("Command was terminated by a signal".to_string())
            }
        }
        Err(e) => {
            error!("Failed to wait for command completion: {}", e);
            Err(format!("Failed to wait for command completion: {e}"))
        }
    }
}

/// Processes output stream from a command
///
/// # Arguments
///
/// * `reader` - The stream to read from
/// * `sender` - Channel to send output lines
async fn handle_stream<R: AsyncRead + Unpin>(reader: R, sender: Sender<String>) {
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
                    let mut parts = line.split('\r');
                    let line = parts.next_back().unwrap_or(&line);
                    if let Err(e) = sender.send(line.to_string()).await {
                        error!("Failed to send line to channel: {}", e);
                        break;
                    }
                }
                line.clear();
            }
            Ok(_) => {
                debug!("No bytes read, but not end of stream");
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
