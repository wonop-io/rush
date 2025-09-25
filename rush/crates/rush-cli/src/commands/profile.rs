//! Performance profiling command implementation

use std::fs;

use clap::ArgMatches;
use log::{info, warn};
use rush_container::profiling;
use rush_core::error::Result;
use serde_json;

use crate::context::CliContext;

/// Execute the profile command
pub async fn execute(matches: &ArgMatches, ctx: &mut CliContext) -> Result<()> {
    let output_file = matches
        .get_one::<String>("output")
        .map(|s| s.as_str())
        .unwrap_or("rush-profile.json");

    let format = matches
        .get_one::<String>("format")
        .map(|s| s.as_str())
        .unwrap_or("json");

    info!(
        "Starting performance profiling (output: {}, format: {})",
        output_file, format
    );

    // Enable profiling globally - this needs to be set BEFORE any tracing initialization
    std::env::set_var("RUSH_PROFILE", "1");

    // Set up tracing for better span capture
    std::env::set_var(
        "RUST_LOG",
        "rush_container=trace,rush_docker=trace,rush_cli=debug",
    );

    // Enable the performance tracker
    profiling::global_tracker().enable();

    // Get the subcommand to profile
    let subcommand = matches
        .get_one::<String>("command")
        .map(|s| s.as_str())
        .ok_or_else(|| {
            rush_core::error::Error::InvalidInput("No command specified to profile".to_string())
        })?;

    // Get any additional arguments
    let additional_args: Vec<String> = matches
        .get_many::<String>("args")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    info!(
        "Profiling command: {} with args: {:?}",
        subcommand, additional_args
    );

    // Check for common flags in additional args
    let force_rebuild = additional_args.iter().any(|arg| arg == "--force-rebuild");

    if force_rebuild {
        info!("Force rebuild flag detected, enabling force rebuild");
        ctx.reactor.set_force_rebuild(true);
    }

    // Execute the command based on what was requested
    match subcommand {
        "build" => {
            info!("Profiling build command...");
            ctx.reactor.build().await?;
        }
        "dev" => {
            info!("Profiling dev environment startup...");
            // Just profile the startup, not the full dev loop
            ctx.reactor.launch().await?;
        }
        "push" => {
            info!("Profiling build and push...");
            ctx.reactor.build_and_push().await?;
        }
        _ => {
            warn!("Unknown command to profile: {}", subcommand);
            return Err(rush_core::error::Error::InvalidInput(format!(
                "Unknown command: {}",
                subcommand
            )));
        }
    }

    // Get the performance report
    let tracker = profiling::global_tracker();
    let report = tracker.generate_report().await;

    // Save the report based on format
    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&report).map_err(|e| {
                rush_core::error::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
            })?;
            fs::write(output_file, json).map_err(|e| rush_core::error::Error::Io(e))?;
            info!("Performance report saved to: {}", output_file);

            // Print summary to console
            print_summary(&report);
        }
        "flamegraph" => {
            warn!("Flamegraph format requires running with RUSH_FLAMEGRAPH=1 environment variable");
            warn!(
                "Rerun with: RUSH_FLAMEGRAPH=1 rush profile --format flamegraph {}",
                subcommand
            );
        }
        "chrome-trace" => {
            // Convert to Chrome trace format
            let trace = convert_to_chrome_trace(&report);
            let json = serde_json::to_string_pretty(&trace).map_err(|e| {
                rush_core::error::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
            })?;
            let trace_file = output_file.replace(".json", ".trace.json");
            fs::write(&trace_file, json).map_err(|e| rush_core::error::Error::Io(e))?;
            info!("Chrome trace saved to: {}", trace_file);
            info!("Open chrome://tracing and load this file to visualize");
        }
        _ => {
            warn!("Unknown format: {}, using JSON", format);
            let json = serde_json::to_string_pretty(&report).map_err(|e| {
                rush_core::error::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
            })?;
            fs::write(output_file, json).map_err(|e| rush_core::error::Error::Io(e))?;
        }
    }

    Ok(())
}

fn print_summary(report: &rush_container::profiling::PerformanceReport) {
    println!("\n=== Performance Profile Summary ===\n");

    println!("Total operations recorded: {}", report.total_entries);

    println!("\n--- Operation Statistics ---");
    for (op, stats) in &report.operation_stats {
        println!("\n{}:", op);
        println!("  Count: {}", stats.count);
        println!("  Total: {:?}", stats.total_duration);
        println!("  Average: {:?}", stats.avg_duration);
        println!("  Min: {:?}", stats.min_duration);
        println!("  Max: {:?}", stats.max_duration);
        println!("  P50: {:?}", stats.p50);
        println!("  P95: {:?}", stats.p95);
        println!("  P99: {:?}", stats.p99);
    }

    println!("\n--- Top 10 Slowest Operations ---");
    for (i, (op, duration)) in report.slowest_operations.iter().take(10).enumerate() {
        println!("{}. {} - {:?}", i + 1, op, duration);
    }
}

#[derive(serde::Serialize)]
struct ChromeTraceEvent {
    name: String,
    cat: String,      // category
    ph: String,       // phase: "B" for begin, "E" for end, "X" for complete
    ts: u64,          // timestamp in microseconds
    dur: Option<u64>, // duration for "X" events
    pid: u32,         // process id
    tid: u32,         // thread id
    args: serde_json::Value,
}

fn convert_to_chrome_trace(
    report: &rush_container::profiling::PerformanceReport,
) -> Vec<ChromeTraceEvent> {
    let mut events = Vec::new();
    let pid = std::process::id();

    // Convert timeline entries to Chrome trace events
    for entry in &report.timeline {
        let ts_micros = entry.timestamp.as_micros() as u64;
        let dur_micros = entry.duration.as_micros() as u64;

        events.push(ChromeTraceEvent {
            name: entry.operation.clone(),
            cat: entry.component.as_deref().unwrap_or("general").to_string(),
            ph: "X".to_string(),
            ts: ts_micros,
            dur: Some(dur_micros),
            pid,
            tid: 1, // Main thread
            args: serde_json::json!({
                "component": entry.component,
                "duration_ms": entry.duration.as_millis()
            }),
        });
    }

    events
}

/// Add a special mode for continuous profiling
pub async fn execute_continuous(_ctx: &mut CliContext) -> Result<()> {
    info!("Starting continuous performance profiling...");

    // Enable profiling
    std::env::set_var("RUSH_PROFILE", "1");

    // Start a background task to periodically export metrics
    let _tracker = profiling::global_tracker();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;

            // Export current metrics
            if let Ok(json) = profiling::global_tracker().export_json().await {
                let filename = format!(
                    "rush-profile-{}.json",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                );
                if let Err(e) = fs::write(&filename, json) {
                    warn!("Failed to write profile: {}", e);
                } else {
                    info!("Profile snapshot saved to: {}", filename);
                }
            }
        }
    });

    Ok(())
}
