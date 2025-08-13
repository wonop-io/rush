use crate::config::create_session_from_config;
use crate::event::LogLevel;
use crate::filter::{ComponentFilter, PhaseFilter};
use crate::session::{OutputMode, OutputSession, SessionBuilder};
use crate::sink::FileSink;
use clap::ArgMatches;
use rush_config::loader::DevOutputConfig;
use rush_core::error::Result;
use std::path::PathBuf;

/// Parse CLI arguments and create an output session
pub fn create_session_from_cli(matches: &ArgMatches) -> Result<OutputSession> {
    // Check if we should use config file settings
    let use_config = !matches.contains_id("output-format") 
        && !matches.contains_id("log-level")
        && !matches.contains_id("filter-components")
        && !matches.contains_id("exclude-components");
    
    if use_config {
        // Use configuration from rushd.yaml
        let config = DevOutputConfig::default();
        return create_session_from_config(&config);
    }
    
    // Otherwise build from CLI arguments
    let mut builder = SessionBuilder::new();
    
    // Parse output format
    if let Some(format) = matches.get_one::<String>("output-format") {
        let mode = match format.as_str() {
            "auto" => OutputMode::Auto,
            "simple" => OutputMode::Simple,
            "split" => OutputMode::Split,
            "dashboard" => OutputMode::Dashboard,
            "web" => OutputMode::Web,
            _ => OutputMode::Auto,
        };
        builder = builder.mode(mode);
    }
    
    // Parse log level
    if let Some(level_str) = matches.get_one::<String>("log-level") {
        let level = match level_str.as_str() {
            "trace" => LogLevel::Trace,
            "debug" => LogLevel::Debug,
            "info" => LogLevel::Info,
            "warn" => LogLevel::Warn,
            "error" => LogLevel::Error,
            _ => LogLevel::Info,
        };
        builder = builder.filter(Box::new(crate::filter::LevelFilter::new(level)));
    }
    
    // Parse component filters
    if let Some(components) = matches.get_many::<String>("filter-components") {
        let component_list: Vec<String> = components.cloned().collect();
        builder = builder.filter(Box::new(ComponentFilter::allowlist(component_list)));
    } else if let Some(components) = matches.get_many::<String>("exclude-components") {
        let component_list: Vec<String> = components.cloned().collect();
        builder = builder.filter(Box::new(ComponentFilter::denylist(component_list)));
    }
    
    // Parse phase filters
    let show_build = !matches.get_flag("show-runtime-only");
    let show_runtime = !matches.get_flag("show-build-only");
    if !show_build || !show_runtime {
        builder = builder.filter(Box::new(PhaseFilter::new(
            show_build,
            show_runtime,
            true, // Always show system messages
        )));
    }
    
    // Add file output if requested
    if let Some(output_type) = matches.get_one::<String>("output") {
        if output_type == "files" || output_type == "both" {
            let output_dir = matches
                .get_one::<String>("output-dir")
                .map(|s| s.as_str())
                .unwrap_or("logs");
            
            let log_path = PathBuf::from(output_dir).join("rush-dev.log");
            if let Ok(file_sink) = FileSink::new(&log_path) {
                builder = builder.sink(Box::new(file_sink));
            }
        }
    }
    
    // Handle color settings
    let no_color = matches.get_flag("no-color");
    if no_color {
        // The session builder will handle this internally based on terminal capabilities
        // For now, we'll rely on the default behavior
    }
    
    builder.build()
}

/// Get output configuration from CLI arguments (for backward compatibility)
pub fn get_output_config_from_cli(matches: &ArgMatches) -> DevOutputConfig {
    let mut config = DevOutputConfig::default();
    
    // Parse output format
    if let Some(format) = matches.get_one::<String>("output-format") {
        config.mode = format.clone();
    }
    
    // Parse log level
    if let Some(level) = matches.get_one::<String>("log-level") {
        config.log_level = level.clone();
    }
    
    // Parse component filters
    if let Some(components) = matches.get_many::<String>("filter-components") {
        config.components.include = Some(components.cloned().collect());
    } else if let Some(components) = matches.get_many::<String>("exclude-components") {
        config.components.exclude = Some(components.cloned().collect());
    }
    
    // Parse phase filters
    config.phases.show_build = !matches.get_flag("show-runtime-only");
    config.phases.show_runtime = !matches.get_flag("show-build-only");
    
    // Parse color settings
    if matches.get_flag("no-color") {
        config.colors.enabled = "false".to_string();
    }
    
    // Parse file output settings
    if let Some(output_type) = matches.get_one::<String>("output") {
        if output_type == "files" || output_type == "both" {
            let output_dir = matches
                .get_one::<String>("output-dir")
                .map(|s| s.as_str())
                .unwrap_or("logs");
            
            config.file_log = Some(rush_config::loader::FileLogConfig {
                enabled: true,
                path: format!("{}/rush-dev.log", output_dir),
            });
        }
    }
    
    config
}