# Rush Helper

A utility crate for checking and validating Rush CLI dependencies and toolchain requirements.

## Overview

The `rush-helper` crate provides comprehensive dependency checking for the Rush deployment tool. It ensures all necessary tools, Rust targets, and platform-specific requirements are installed before running Rush commands.

## Features

- **Rust Target Validation**: Checks for required Rust compilation targets
  - `wasm32-unknown-unknown` for frontend builds
  - `x86_64-unknown-linux-gnu` for cross-compilation
  - `x86_64-apple-darwin` for Apple Silicon compatibility

- **Tool Detection**: Verifies installation of essential tools
  - Docker and Docker Buildx
  - Trunk for WASM builds
  - Platform-specific toolchains

- **Apple Silicon Support**: Special handling for M1/M2 Macs
  - Checks for x86_64 cross-compilation toolchain
  - Validates linker configuration
  - Detects Homebrew formulae

- **Actionable Error Messages**: Provides exact commands to fix missing dependencies

## Usage

### As a Library

```rust
use rush_helper::{run_preflight_checks, HelperError};

fn main() {
    match run_preflight_checks() {
        Ok(_) => println!("All dependencies installed!"),
        Err(e) => {
            eprintln!("Missing dependencies: {}", e.get_message());
            
            // Get fix commands
            for cmd in e.get_fix_commands() {
                eprintln!("Run: {}", cmd.join(" "));
            }
        }
    }
}
```

### Command Line

When integrated with rush-cli:

```bash
# Check all dependencies
rush check-deps

# Skip dependency checks
rush --skip-checks <product> dev
```

## Error Types

The crate provides structured errors with fix commands:

- `MissingTool`: A required tool is not installed
- `MissingTarget`: A Rust compilation target is missing
- `ConfigurationError`: Configuration files need updates
- `MultipleIssues`: Multiple problems detected

## Platform Detection

```rust
use rush_helper::{is_apple_silicon, get_platform};

if is_apple_silicon() {
    println!("Running on Apple Silicon");
}

println!("Platform: {}", get_platform());
// Output: "macos-arm64", "linux-x86_64", etc.
```

## Requirements

The helper checks for:

1. **Rust Toolchain**
   - rustup installed
   - Required compilation targets

2. **Docker**
   - Docker daemon running
   - Docker Buildx available

3. **Frontend Tools**
   - Trunk or wasm-trunk for WASM builds
   - wasm32-unknown-unknown target

4. **Cross-Compilation (Apple Silicon)**
   - x86_64-unknown-linux-gnu toolchain via Homebrew
   - Proper linker configuration in ~/.cargo/config.toml

## Example Output

```
❌ Missing dependencies detected:
Missing Rust target: x86_64-unknown-linux-gnu

📦 To fix these issues, run:
  rustup target add x86_64-unknown-linux-gnu

💡 Tip: After installing missing targets, you may need to configure your linker.
   For Apple Silicon cross-compilation, ensure you have the x86_64 toolchain installed.
```

## License

MIT