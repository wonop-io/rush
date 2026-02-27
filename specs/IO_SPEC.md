# Rush I/O Specification

## Overview

Rush needs to capture and forward output from various processes (Docker builds, container execution, and build scripts) to configurable output sinks. The system must capture stdout and stderr streams in real-time as they are produced, not via polling or log retrieval commands.

## Core Requirements

### 1. Direct Stream Capture

All output must be captured directly from spawned child processes:

- **Build Scripts**: When executing build scripts (e.g., `bash build_script.sh`), capture stdout/stderr directly from the spawned bash process
- **Docker Build**: When running `docker build`, capture stdout/stderr from the docker process itself
- **Docker Run**: When starting containers with `docker run`, capture output directly from the docker process, NOT via `docker logs`

### 2. Real-time Streaming

- Output must be forwarded to sinks as it is produced, line by line
- No buffering that would delay output visibility
- Preserve the distinction between stdout and stderr streams

### 3. Output Preservation

- ANSI color codes must be preserved when present
- Control characters should be maintained for proper terminal formatting
- Line endings should be handled correctly across platforms

## Implementation Requirements

### Process Spawning

All processes should be spawned with piped stdout/stderr:

```rust
let mut child = Command::new(command)
    .args(args)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
```

### Stream Reading

Each stream (stdout/stderr) should be read asynchronously:

```rust
let stdout = child.stdout.take();
let stderr = child.stderr.take();

// Spawn tasks to read each stream
tokio::spawn(async move {
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    
    while reader.read_line(&mut line).await? > 0 {
        // Forward to sink
        sink.write(LogEntry { ... }).await?;
        line.clear();
    }
});
```

### Docker Specific Requirements

#### Docker Build
```bash
docker build -t image:tag .
```
- Must capture build progress output
- Must preserve build step indicators
- Must capture error messages from failed builds

#### Docker Run
```bash
docker run -d --name container image:tag
```
- Must NOT use `docker logs -f` after container starts
- Instead, should capture output directly from the `docker run` process
- For detached containers (`-d`), may need to use `docker attach` or run without `-d` and manage lifecycle differently

#### Alternative: Docker Attach
For already running containers:
```bash
docker attach --no-stdin container_name
```
- Provides direct stream access to container's stdout/stderr
- Better than `docker logs` for real-time streaming

## Sink Interface

The Sink trait should receive structured log entries:

```rust
pub struct LogEntry {
    pub component: String,      // Component name (e.g., "frontend", "backend")
    pub content: String,         // The actual log line
    pub timestamp: DateTime<Utc>,
    pub is_error: bool,         // Whether from stderr
    pub phase: LogPhase,        // Build, Runtime, or System
}
```

## Output Formats

### Standard Output
- Simple line-by-line output with component prefix
- Colors enabled by default (unless `--no-color`)

### Split Output
- Prefixed with phase indicators: `[BUILD]`, `[RUNTIME]`, `[SYSTEM]`
- Component name and timestamp included
- Clear visual separation between phases

### No-Color Output
- Plain text without ANSI codes
- Suitable for CI/CD environments

## Error Handling

- Failed processes should have their stderr captured and displayed
- Exit codes should be checked and reported
- Timeouts should be configurable for long-running operations

## Performance Considerations

- Use buffered readers for efficiency
- Avoid unnecessary string allocations
- Handle backpressure when sinks are slow
- Clean up resources (close pipes, wait for child processes)

## Testing Requirements

- Verify output is captured in real-time, not batched
- Test with processes that produce colored output
- Test with processes that produce large amounts of output
- Test error scenarios (process failures, timeouts)
- Verify no output is lost or corrupted