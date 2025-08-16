use std::env;
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
        return Ok(());
    }

    match args[1].as_str() {
        "ci" | "run-ci" => run_ci(),
        "fmt" => run_fmt(false),
        "fmt-check" => run_fmt(true),
        "sort" => run_sort(false),
        "sort-check" => run_sort(true),
        "autoinherit" => run_autoinherit(false),
        "autoinherit-check" => run_autoinherit(true),
        "clippy" => run_clippy(),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_help();
            Err("Unknown command".into())
        }
    }
}

fn print_help() {
    println!("Rush xtask - Development task runner");
    println!();
    println!("Usage: cargo xtask <command>");
    println!();
    println!("Commands:");
    println!("  ci, run-ci       Run all CI checks");
    println!("  fmt              Format code");
    println!("  fmt-check        Check code formatting");
    println!("  sort             Sort Cargo.toml files");
    println!("  sort-check       Check Cargo.toml sorting");
    println!("  autoinherit      Apply workspace inheritance");
    println!("  autoinherit-check Check workspace inheritance");
    println!("  clippy           Run clippy linter");
    println!("  help             Show this help message");
}

fn run_ci() -> Result<()> {
    println!("🔍 Running CI checks...");

    // Check formatting
    println!("📝 Checking formatting...");
    run_fmt(true)?;

    // Check Cargo.toml sorting
    println!("🔤 Checking Cargo.toml sorting...");
    run_sort(true)?;

    // Check workspace inheritance
    println!("🔗 Checking workspace inheritance...");
    run_autoinherit(true)?;

    // Run clippy
    println!("📋 Running clippy...");
    run_clippy()?;

    println!("✅ All CI checks passed!");
    Ok(())
}

fn run_fmt(check: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(["fmt", "--all"]);

    if check {
        cmd.args(["--", "--check"]);
    }

    run_command(cmd)
}

fn run_sort(check: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args(["sort", "--workspace"]);

    if check {
        cmd.arg("--check");
    }

    run_command(cmd)
}

fn run_autoinherit(check: bool) -> Result<()> {
    if check {
        // cargo-autoinherit doesn't have a --check flag
        // We'll skip this check in CI mode
        println!("⚠️  cargo-autoinherit doesn't support check mode, skipping...");
        return Ok(());
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("autoinherit");

    run_command(cmd)
}

fn run_clippy() -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]);

    run_command(cmd)
}

fn run_command(mut cmd: Command) -> Result<()> {
    let status = cmd.status()?;

    if !status.success() {
        return Err(format!("Command failed with status: {status:?}").into());
    }

    Ok(())
}
