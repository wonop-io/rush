# Rush Output System Usage Examples

## New Enhanced Output System

The Rush development environment now includes an enhanced output system with multiple visualization modes and filtering capabilities.

### CLI Arguments for Output Format

The `rush dev` command now supports several new arguments for controlling output:

#### Output Format Selection

```bash
# Use automatic format detection (default)
rush myapp dev --output-format auto

# Simple line-by-line output (classic mode)
rush myapp dev --output-format simple

# Split terminal view (build and runtime in separate panes)
rush myapp dev --output-format split

# Dashboard view with widgets and metrics
rush myapp dev --output-format dashboard

# Web browser view
rush myapp dev --output-format web
```

#### Log Level Filtering

```bash
# Show only info level and above (default)
rush myapp dev --log-level info

# Show debug messages
rush myapp dev --log-level debug

# Show only errors
rush myapp dev --log-level error
```

#### Component Filtering

```bash
# Show only specific components
rush myapp dev --filter-components backend frontend

# Hide specific components
rush myapp dev --exclude-components ingress localstack

# Combine with other filters
rush myapp dev --filter-components backend --log-level debug
```

#### Phase Filtering

```bash
# Show only build/compilation output
rush myapp dev --show-build-only

# Show only runtime output
rush myapp dev --show-runtime-only
```

#### File Logging

```bash
# Save logs to files in addition to terminal output
rush myapp dev --output files --output-dir ./logs

# Both terminal and file output
rush myapp dev --output both --output-dir ./logs
```

#### Other Options

```bash
# Disable colored output
rush myapp dev --no-color

# Disable timestamps in file logs
rush myapp dev --no-timestamps

# Disable source names in logs
rush myapp dev --no-source-names

# Disable output buffering
rush myapp dev --no-buffering
```

### Configuration via rushd.yaml

You can also configure the output system in your `rushd.yaml` file:

```yaml
# rushd.yaml
dev_output:
  # Output mode
  mode: split  # auto, simple, split, dashboard, web
  
  # Component filtering
  components:
    include: ["backend", "frontend", "database"]
    # OR use exclude
    # exclude: ["ingress"]
  
  # Phase filtering
  phases:
    show_build: true
    show_runtime: true
    show_system: true
  
  # Log level
  log_level: debug  # trace, debug, info, warn, error
  
  # Colors
  colors:
    enabled: auto  # auto, true, false
    theme: default  # default, monokai, dracula
  
  # File logging
  file_log:
    enabled: true
    path: "./logs/rush-dev.log"
  
  # Web view settings (when mode is "web")
  web:
    port: 8080
    open_browser: true
```

### Example Combinations

#### Development with Focused Debugging

```bash
# Focus on backend with debug logging in split view
rush myapp dev \
  --output-format split \
  --filter-components backend \
  --log-level debug
```

#### Clean Production-like View

```bash
# Show only errors from runtime
rush myapp dev \
  --output-format simple \
  --show-runtime-only \
  --log-level error
```

#### Full Debugging with File Output

```bash
# Everything with file logging for later analysis
rush myapp dev \
  --output-format dashboard \
  --log-level trace \
  --output both \
  --output-dir ./debug-logs
```

#### Monitoring Specific Services

```bash
# Watch database and cache services only
rush myapp dev \
  --filter-components postgres redis \
  --output-format split
```

## Benefits of the New System

1. **Better Organization**: Separate build and runtime output for clarity
2. **Reduced Noise**: Filter out components you're not working on
3. **Flexible Visualization**: Choose the view that fits your workflow
4. **Development Focus**: Optimized for local development experience
5. **Zero Configuration**: Works great out of the box with sensible defaults

## Output Modes Explained

### Auto Mode
Automatically selects the best output mode based on:
- Terminal capabilities
- Terminal width
- Whether output is piped or interactive

### Simple Mode
Traditional line-by-line output with timestamps and component names:
```
16:23:45.123 backend | Starting server...
16:23:45.456 frontend | Compiling TypeScript...
16:23:46.789 database | PostgreSQL ready
```

### Split Mode
Divides the terminal into panes for build and runtime output:
```
┌─────────── Build Output ───────────┬──────── Runtime Output ────────┐
│ frontend | Compiling TypeScript... │ backend  | Server starting...  │
│ backend  | Building Rust...        │ database | PostgreSQL ready    │
│ frontend | ✓ Build complete        │ backend  | Listening on :8080  │
└─────────────────────────────────────┴─────────────────────────────────┘
```

### Dashboard Mode
Rich TUI with component tree, progress bars, and metrics:
- Component hierarchy view
- Build progress indicators
- Recent logs window
- System metrics (CPU, memory, uptime)
- Interactive controls

### Web Mode
Browser-based view at http://localhost:8080 with:
- Real-time log streaming
- Advanced filtering and search
- Component selection
- Log history
- Export capabilities

## Troubleshooting

If you encounter issues with the output system:

1. **Terminal compatibility**: Try `--output-format simple` for basic terminals
2. **Performance**: Use `--exclude-components` to reduce log volume
3. **Color issues**: Use `--no-color` if colors don't display correctly
4. **Missing logs**: Check `--log-level` isn't filtering too aggressively

## Migration from Old System

The new system is backward compatible. Your existing workflows will continue to work, but you can now:
- Add `--output-format split` for better visualization
- Use `--filter-components` to focus on specific services
- Add `--log-level debug` when troubleshooting

The old `--silence` argument still works but is now equivalent to `--exclude-components`.