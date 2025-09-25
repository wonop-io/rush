use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};

/// Rush development task runner
#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Development task runner for Rush", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run all tests with proper flags
    Test {
        /// Run only unit tests
        #[arg(long)]
        unit: bool,
        /// Run only integration tests
        #[arg(long)]
        integration: bool,
        /// Package to test
        #[arg(short, long)]
        package: Option<String>,
        /// Use serial execution for Docker tests
        #[arg(long)]
        serial: bool,
    },
    /// Run full CI pipeline locally
    Ci {
        /// Skip tests
        #[arg(long)]
        skip_tests: bool,
        /// Skip clippy
        #[arg(long)]
        skip_clippy: bool,
    },
    /// Format all code
    Fmt {
        /// Check only, don't modify files
        #[arg(long)]
        check: bool,
    },
    /// Run clippy with proper flags
    Clippy {
        /// Allow warnings (don't fail on warnings)
        #[arg(long)]
        allow_warnings: bool,
        /// Enable pedantic lints
        #[arg(long)]
        pedantic: bool,
    },
    /// Clean build artifacts and Docker containers
    Clean {
        /// Also clean Docker containers and images
        #[arg(long)]
        docker: bool,
        /// Deep clean (remove target, Cargo.lock, etc.)
        #[arg(long)]
        deep: bool,
    },
    /// Build release binaries
    Release {
        /// Target triple for cross-compilation
        #[arg(long)]
        target: Option<String>,
        /// Strip debug symbols
        #[arg(long)]
        strip: bool,
    },
    /// Install rush locally
    Install {
        /// Force reinstall even if already installed
        #[arg(long)]
        force: bool,
        /// Installation path (defaults to ~/.cargo/bin)
        #[arg(long)]
        path: Option<String>,
    },
    /// Check code quality (warnings, unused deps, etc.)
    Check {
        /// Check for unused dependencies
        #[arg(long)]
        deps: bool,
        /// Check for security vulnerabilities
        #[arg(long)]
        security: bool,
    },
    /// Generate documentation
    Doc {
        /// Open in browser after building
        #[arg(long)]
        open: bool,
        /// Include private items
        #[arg(long)]
        private: bool,
    },
    /// Run benchmarks
    Bench {
        /// Benchmark name pattern
        pattern: Option<String>,
    },
    /// Update dependencies
    Update {
        /// Only update patch versions
        #[arg(long)]
        conservative: bool,
        /// Dry run - show what would be updated
        #[arg(long)]
        dry_run: bool,
    },
    /// Workspace maintenance commands
    Workspace {
        /// Sort Cargo.toml files
        #[arg(long)]
        sort: bool,
        /// Apply workspace inheritance
        #[arg(long)]
        autoinherit: bool,
        /// Check for version mismatches
        #[arg(long)]
        check_versions: bool,
    },
    /// Development environment setup
    Setup {
        /// Install all required tools
        #[arg(long)]
        tools: bool,
        /// Setup git hooks
        #[arg(long)]
        hooks: bool,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Test {
            unit,
            integration,
            package,
            serial,
        } => run_tests(unit, integration, package, serial),
        Commands::Ci {
            skip_tests,
            skip_clippy,
        } => run_ci(skip_tests, skip_clippy),
        Commands::Fmt { check } => run_fmt(check),
        Commands::Clippy {
            allow_warnings,
            pedantic,
        } => run_clippy(allow_warnings, pedantic),
        Commands::Clean { docker, deep } => run_clean(docker, deep),
        Commands::Release { target, strip } => run_release(target, strip),
        Commands::Install { force, path } => run_install(force, path),
        Commands::Check { deps, security } => run_check(deps, security),
        Commands::Doc { open, private } => run_doc(open, private),
        Commands::Bench { pattern } => run_bench(pattern),
        Commands::Update {
            conservative,
            dry_run,
        } => run_update(conservative, dry_run),
        Commands::Workspace {
            sort,
            autoinherit,
            check_versions,
        } => run_workspace(sort, autoinherit, check_versions),
        Commands::Setup { tools, hooks } => run_setup(tools, hooks),
    }
}

// Command implementations

fn run_tests(unit: bool, integration: bool, package: Option<String>, serial: bool) -> Result<()> {
    println!("{}", "🧪 Running tests...".green().bold());

    let pb = create_spinner("Running tests...");

    let mut cmd = Command::new("cargo");
    cmd.arg("test");

    if let Some(pkg) = package {
        cmd.args(["--package", &pkg]);
    } else {
        cmd.arg("--workspace");
    }

    if unit {
        cmd.arg("--lib");
    } else if integration {
        cmd.arg("--test");
        cmd.arg("*");
    }

    if serial {
        cmd.args(["--", "--test-threads=1"]);
    }

    pb.finish_with_message("Tests started");

    run_command(cmd, "Tests")?;

    println!("{}", "✅ Tests passed!".green());
    Ok(())
}

fn run_ci(skip_tests: bool, skip_clippy: bool) -> Result<()> {
    println!("{}", "🚀 Running CI pipeline...".blue().bold());
    println!();

    let steps: Vec<(&str, Box<dyn Fn() -> Result<()>>)> = vec![
        ("Checking formatting", Box::new(|| run_fmt(true))),
        (
            "Checking Cargo.toml sorting",
            Box::new(|| run_workspace(true, false, false)),
        ),
        (
            "Running tests",
            Box::new(move || {
                if skip_tests {
                    Ok(())
                } else {
                    run_tests(false, false, None, false)
                }
            }),
        ),
        (
            "Running clippy",
            Box::new(move || {
                if skip_clippy {
                    Ok(())
                } else {
                    run_clippy(false, false)
                }
            }),
        ),
        (
            "Checking for security issues",
            Box::new(|| run_check(false, true)),
        ),
    ];

    for (name, step) in steps {
        print!("{} {}... ", "►".blue(), name);
        match step() {
            Ok(_) => println!("{}", "✓".green()),
            Err(e) => {
                println!("{}", "✗".red());
                return Err(e);
            }
        }
    }

    println!();
    println!("{}", "✅ CI pipeline passed!".green().bold());
    Ok(())
}

fn run_fmt(check: bool) -> Result<()> {
    if !check {
        println!("{}", "🎨 Formatting code...".cyan().bold());
    }

    let mut cmd = Command::new("cargo");
    cmd.args(["fmt", "--all"]);

    if check {
        cmd.args(["--", "--check"]);
    }

    run_command(cmd, "Formatting")?;

    if !check {
        println!("{}", "✅ Code formatted!".green());
    }
    Ok(())
}

fn run_clippy(allow_warnings: bool, pedantic: bool) -> Result<()> {
    println!("{}", "📋 Running clippy...".yellow().bold());

    let pb = create_spinner("Analyzing code...");

    let mut cmd = Command::new("cargo");
    cmd.args(["clippy", "--workspace", "--all-targets", "--all-features"]);

    if !allow_warnings {
        cmd.args(["--", "-D", "warnings"]);
    }

    if pedantic {
        cmd.args(["-W", "clippy::pedantic"]);
    }

    pb.finish_with_message("Analysis complete");

    run_command(cmd, "Clippy")?;

    println!("{}", "✅ Clippy passed!".green());
    Ok(())
}

fn run_clean(docker: bool, deep: bool) -> Result<()> {
    println!("{}", "🧹 Cleaning...".magenta().bold());

    // Clean Cargo artifacts
    let pb = create_spinner("Cleaning build artifacts...");
    let mut cmd = Command::new("cargo");
    cmd.arg("clean");
    run_command(cmd, "Cargo clean")?;
    pb.finish_with_message("Build artifacts cleaned");

    if deep {
        println!("  Removing Cargo.lock...");
        let _ = std::fs::remove_file("Cargo.lock");

        println!("  Removing target directories...");
        let _ = std::fs::remove_dir_all("target");
        for entry in std::fs::read_dir(".")? {
            let entry = entry?;
            if entry.path().is_dir() {
                let target_path = entry.path().join("target");
                if target_path.exists() {
                    let _ = std::fs::remove_dir_all(target_path);
                }
            }
        }
    }

    if docker {
        println!("  Cleaning Docker containers...");
        let pb = create_spinner("Stopping and removing containers...");

        // Stop all rush containers
        let mut cmd = Command::new("docker");
        cmd.args(["ps", "-a", "-q", "--filter", "label=rush"]);
        if let Ok(output) = cmd.output() {
            let containers = String::from_utf8_lossy(&output.stdout);
            for container_id in containers.lines() {
                if !container_id.is_empty() {
                    let _ = Command::new("docker")
                        .args(["rm", "-f", container_id])
                        .output();
                }
            }
        }

        pb.finish_with_message("Docker cleaned");
    }

    println!("{}", "✅ Cleanup complete!".green());
    Ok(())
}

fn run_release(target: Option<String>, strip: bool) -> Result<()> {
    println!("{}", "📦 Building release...".blue().bold());

    let pb = create_spinner("Building release binaries...");

    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--release"]);

    if let Some(ref target) = target {
        cmd.args(["--target", target]);
    }

    pb.finish_with_message("Build complete");

    run_command(cmd, "Release build")?;

    if strip {
        println!("  Stripping debug symbols...");
        let binary_path = if let Some(ref target) = target {
            format!("target/{target}/release/rush")
        } else {
            "target/release/rush".to_string()
        };

        let mut strip_cmd = Command::new("strip");
        strip_cmd.arg(&binary_path);
        let _ = strip_cmd.output(); // Ignore errors as strip might not be available
    }

    println!("{}", "✅ Release build complete!".green());
    Ok(())
}

fn run_install(force: bool, path: Option<String>) -> Result<()> {
    println!("{}", "📥 Installing Rush...".blue().bold());

    let mut cmd = Command::new("cargo");
    cmd.args(["install", "--path", "crates/rush-cli"]);

    if force {
        cmd.arg("--force");
    }

    if let Some(path) = path {
        cmd.args(["--root", &path]);
    }

    run_command(cmd, "Installation")?;

    println!("{}", "✅ Rush installed successfully!".green());
    Ok(())
}

fn run_check(deps: bool, security: bool) -> Result<()> {
    println!("{}", "🔍 Running checks...".yellow().bold());

    // Basic cargo check
    let mut cmd = Command::new("cargo");
    cmd.args(["check", "--workspace", "--all-targets"]);
    run_command(cmd, "Cargo check")?;

    if deps {
        println!("  Checking for unused dependencies...");

        // Check if cargo-udeps is installed
        if which::which("cargo-udeps").is_ok() {
            let mut cmd = Command::new("cargo");
            cmd.args(["+nightly", "udeps", "--all-targets"]);
            let _ = run_command(cmd, "Unused deps check");
        } else {
            println!("    {} cargo-udeps not found, skipping", "⚠".yellow());
        }
    }

    if security {
        println!("  Checking for security vulnerabilities...");

        // Check if cargo-audit is installed
        if which::which("cargo-audit").is_ok() {
            let mut cmd = Command::new("cargo");
            cmd.arg("audit");
            run_command(cmd, "Security audit")?;
        } else {
            println!("    {} cargo-audit not found, skipping", "⚠".yellow());
        }
    }

    println!("{}", "✅ All checks passed!".green());
    Ok(())
}

fn run_doc(open: bool, private: bool) -> Result<()> {
    println!("{}", "📚 Building documentation...".cyan().bold());

    let pb = create_spinner("Generating documentation...");

    let mut cmd = Command::new("cargo");
    cmd.args(["doc", "--workspace", "--no-deps"]);

    if private {
        cmd.arg("--document-private-items");
    }

    if open {
        cmd.arg("--open");
    }

    pb.finish_with_message("Documentation built");

    run_command(cmd, "Documentation")?;

    println!("{}", "✅ Documentation generated!".green());
    Ok(())
}

fn run_bench(pattern: Option<String>) -> Result<()> {
    println!("{}", "⚡ Running benchmarks...".yellow().bold());

    let mut cmd = Command::new("cargo");
    cmd.args(["bench", "--workspace"]);

    if let Some(pattern) = pattern {
        cmd.arg(&pattern);
    }

    run_command(cmd, "Benchmarks")?;

    println!("{}", "✅ Benchmarks complete!".green());
    Ok(())
}

fn run_update(conservative: bool, dry_run: bool) -> Result<()> {
    println!("{}", "🔄 Updating dependencies...".blue().bold());

    if dry_run {
        // Check outdated dependencies
        if which::which("cargo-outdated").is_ok() {
            let mut cmd = Command::new("cargo");
            cmd.arg("outdated");
            run_command(cmd, "Outdated check")?;
        } else {
            println!("{} cargo-outdated not found", "⚠".yellow());
        }
    } else {
        let mut cmd = Command::new("cargo");
        cmd.arg("update");

        if conservative {
            cmd.arg("--conservative");
        }

        run_command(cmd, "Update")?;
    }

    println!("{}", "✅ Dependencies updated!".green());
    Ok(())
}

fn run_workspace(sort: bool, autoinherit: bool, check_versions: bool) -> Result<()> {
    if sort {
        println!("  Sorting Cargo.toml files...");

        // Check if cargo-sort is installed
        if which::which("cargo-sort").is_ok() {
            let mut cmd = Command::new("cargo");
            cmd.args(["sort", "--workspace"]);
            run_command(cmd, "Cargo sort")?;
        } else {
            println!("    {} cargo-sort not found, skipping", "⚠".yellow());
        }
    }

    if autoinherit {
        println!("  Applying workspace inheritance...");

        // Check if cargo-autoinherit is installed
        if which::which("cargo-autoinherit").is_ok() {
            let mut cmd = Command::new("cargo");
            cmd.arg("autoinherit");
            let _ = run_command(cmd, "Autoinherit");
        } else {
            println!("    {} cargo-autoinherit not found, skipping", "⚠".yellow());
        }
    }

    if check_versions {
        println!("  Checking version consistency...");
        // This would check that all workspace members have consistent versions
        // For now, just a placeholder
        println!("    Version check not yet implemented");
    }

    Ok(())
}

fn run_setup(tools: bool, hooks: bool) -> Result<()> {
    println!(
        "{}",
        "🔧 Setting up development environment...".magenta().bold()
    );

    if tools {
        println!("  Installing required tools...");

        let tools_to_install = vec![
            ("cargo-sort", "cargo install cargo-sort"),
            ("cargo-audit", "cargo install cargo-audit"),
            ("cargo-outdated", "cargo install cargo-outdated"),
            ("cargo-udeps", "cargo install cargo-udeps"),
        ];

        for (name, install_cmd) in tools_to_install {
            print!("    Installing {name}... ");
            if which::which(name).is_err() {
                let parts: Vec<&str> = install_cmd.split_whitespace().collect();
                let mut cmd = Command::new(parts[0]);
                for part in &parts[1..] {
                    cmd.arg(part);
                }

                if run_command(cmd, name).is_ok() {
                    println!("{}", "✓".green());
                } else {
                    println!("{}", "✗".red());
                }
            } else {
                println!("{}", "already installed ✓".green());
            }
        }
    }

    if hooks {
        println!("  Setting up git hooks...");

        // Create pre-commit hook
        let hook_content = r#"#!/bin/bash
# Rush pre-commit hook

echo "Running pre-commit checks..."

# Format check
cargo fmt --check || {
    echo "Code is not formatted. Run 'cargo xtask fmt' to fix."
    exit 1
}

# Clippy check
cargo clippy -- -D warnings || {
    echo "Clippy found issues. Fix them before committing."
    exit 1
}

# Tests
cargo test --lib || {
    echo "Tests failed. Fix them before committing."
    exit 1
}

echo "Pre-commit checks passed!"
"#;

        let hook_path = ".git/hooks/pre-commit";
        std::fs::write(hook_path, hook_content)?;

        // Make it executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(hook_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(hook_path, perms)?;
        }

        println!("    Pre-commit hook installed ✓");
    }

    println!("{}", "✅ Setup complete!".green());
    Ok(())
}

// Helper functions

fn run_command(mut cmd: Command, name: &str) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {name}"))?;

    if !status.success() {
        anyhow::bail!("{} failed with status: {:?}", name, status);
    }

    Ok(())
}

fn create_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}
