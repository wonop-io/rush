# Issue: Incorrect I/O Implementation in Rush Output System

## Problem Summary

The current implementation incorrectly uses `docker logs -f` to capture container output instead of directly piping stdout/stderr from the spawned processes. This approach has several critical issues that affect real-time output streaming and color preservation.

## Current Incorrect Implementation

### 1. Container Output (`simple_output.rs`)

**Current (INCORRECT)**:
```rust
// Using docker logs command to follow the container logs
let mut child = Command::new("docker")
    .args(["logs", "-f", "--tail", "100", container_id])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
```

**Problem**: 
- `docker logs` retrieves logs from Docker's logging driver, not directly from the container
- Loses real-time streaming capability
- Doesn't preserve TTY/color information even if container was started with `-t`
- Adds unnecessary overhead and latency

### 2. Missing Direct Process Output Capture

The old implementation (in `old_src/container/docker.rs` lines 384-573) correctly:
1. Spawned `docker run` without `-d` (detached) flag
2. Directly piped stdout/stderr from the docker process
3. Read streams line by line and forwarded to output system

## Correct Implementation Pattern

### For Docker Containers

**Option 1: Run in Foreground (Preferred for Dev)**
```rust
// Run container in foreground, capturing output directly
let mut child = Command::new("docker")
    .args([
        "run",
        "--rm",           // Remove container when it exits
        "-t",            // Allocate pseudo-TTY for colors
        "--name", container_name,
        // ... other args
        image_name
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;

// Read from child.stdout and child.stderr directly
```

**Option 2: Use Docker Attach for Detached Containers**
```rust
// First start container detached
docker_client.start_container(container_id).await?;

// Then attach to get output streams
let mut child = Command::new("docker")
    .args(["attach", "--no-stdin", container_id])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
```

### For Build Scripts

**Current (Partially Correct)**:
The build script execution is closer to correct but still needs improvement:
```rust
let mut child = Command::new(&build_command[0])
    .args(&build_command[1..])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
```

This is the right approach - directly piping from the spawned process.

### For Docker Build

**Should be**:
```rust
let mut child = Command::new("docker")
    .args(["build", "-t", tag, context_path])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;

// Read build output directly from child.stdout/stderr
```

## Impact of Current Implementation

1. **No Real-time Output**: Logs are buffered by Docker's logging system
2. **Lost Color Information**: ANSI codes are stripped by the logging driver
3. **Performance Overhead**: Additional syscalls and memory usage
4. **Timing Issues**: Output may appear out of order or delayed
5. **Missing Output**: Some output (especially during container startup) may be lost

## Required Changes

### 1. Update `simple_output.rs`

- Remove `follow_container_logs_simple` function that uses `docker logs`
- Implement `attach_to_container` that uses `docker attach` or runs containers in foreground
- Ensure proper stream handling for both stdout and stderr

### 2. Update Container Lifecycle

- Modify how containers are started in `reactor.rs`
- Consider running development containers in foreground mode
- Implement proper signal handling for container shutdown

### 3. Preserve Stream Separation

- Keep stdout and stderr separate until the sink level
- Mark stderr output with `is_error: true` in LogEntry
- Allow sinks to decide how to display error output

## Example of Correct Implementation

```rust
pub async fn capture_process_output(
    command: &str,
    args: Vec<String>,
    component_name: String,
    sink: Arc<Mutex<Box<dyn Sink>>>,
    phase: LogPhase,
) -> Result<()> {
    let mut child = Command::new(command)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Process(format!("Failed to spawn process: {e}")))?;

    let stdout = child.stdout.take()
        .ok_or_else(|| Error::Process("Failed to capture stdout".into()))?;
    let stderr = child.stderr.take()
        .ok_or_else(|| Error::Process("Failed to capture stderr".into()))?;

    let mut handles = vec![];

    // Handle stdout
    let sink_clone = sink.clone();
    let component_clone = component_name.clone();
    let handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            let entry = LogEntry {
                component: component_clone.clone(),
                content: line.clone(),
                timestamp: Utc::now(),
                is_error: false,
                phase,
            };
            
            let mut sink_guard = sink_clone.lock().await;
            sink_guard.write(entry).await?;
            line.clear();
        }
        Ok::<(), Error>(())
    });
    handles.push(handle);

    // Handle stderr (similar pattern)
    // ...

    // Wait for process to complete
    let status = child.wait().await?;
    
    // Wait for all output to be read
    for handle in handles {
        handle.await??;
    }

    if !status.success() {
        return Err(Error::Process(format!("Process failed with status: {}", status)));
    }

    Ok(())
}
```

## Priority

**HIGH** - This is a fundamental issue affecting the core functionality of the output system. Without proper stream capture, users cannot see real-time output or colors, which significantly impacts the development experience.

## References

- Original working implementation: `old_src/container/docker.rs` lines 384-573
- Docker documentation on logging drivers vs direct output
- Unix process I/O fundamentals