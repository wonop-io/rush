//! Logging configuration for Rush CLI
//!
//! This module sets up the unified logging system that routes all logs
//! through the output sink.

use std::env;
use std::sync::Arc;

use clap::ArgMatches;
use log::Level;
use rush_output::log_bridge;
use rush_output::simple::{Sink, SplitSink, StdoutSink};
use tokio::sync::Mutex;

/// Create the global output sink based on command line arguments
pub fn create_output_sink(matches: &ArgMatches) -> Arc<Mutex<Box<dyn Sink>>> {
    // Check if we're in dev mode for split output
    let is_dev_mode = matches.subcommand_matches("dev").is_some();
    let output_format = matches
        .subcommand_matches("dev")
        .and_then(|dev_matches| dev_matches.get_one::<String>("output-format"))
        .map(|s| s.as_str());

    let sink: Box<dyn Sink> = if is_dev_mode && output_format == Some("split") {
        Box::new(SplitSink::new())
    } else {
        Box::new(StdoutSink::new())
    };

    Arc::new(Mutex::new(sink))
}

/// Setup logging to route through the output sink
pub fn setup_logging_with_sink(
    matches: &ArgMatches,
    sink: Arc<Mutex<Box<dyn Sink>>>,
) -> Result<(), log::SetLoggerError> {
    // Determine log level from command line or environment
    let log_level = if let Some(level_str) = matches.get_one::<String>("log_level") {
        env::set_var("RUST_LOG", level_str);
        match level_str.to_lowercase().as_str() {
            "trace" => Level::Trace,
            "debug" => Level::Debug,
            "info" => Level::Info,
            "warn" => Level::Warn,
            "error" => Level::Error,
            _ => Level::Info,
        }
    } else if let Ok(rust_log) = env::var("RUST_LOG") {
        match rust_log.to_lowercase().as_str() {
            "trace" => Level::Trace,
            "debug" => Level::Debug,
            "info" => Level::Info,
            "warn" => Level::Warn,
            "error" => Level::Error,
            _ => Level::Info,
        }
    } else {
        // Default to info level
        Level::Info
    };

    // Initialize the sink-based logger
    log_bridge::init_with_sink(sink, log_level)?;

    // Test the logger is working
    log::info!("Rush logging initialized at level: {log_level:?}");
    log::debug!("Debug logging is enabled");
    log::trace!("Trace logging is enabled");
    Ok(())
}

/// Setup traditional env_logger (fallback for when sink is not available)
pub fn setup_env_logging(matches: &ArgMatches) {
    if let Some(log_level) = matches.get_one::<String>("log_level") {
        env::set_var("RUST_LOG", log_level);
        env_logger::builder().parse_env("RUST_LOG").init();
        log::trace!("Log level set to: {log_level}");
    } else {
        env_logger::init();
    }
    log::trace!("Starting Rush application");
}
