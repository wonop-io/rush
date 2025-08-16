//! CLI integration for output handling
//!
//! This module is kept for backward compatibility but now just creates sinks
//! instead of full sessions.

use clap::ArgMatches;

/// Parse CLI arguments to get output format
/// This is a simplified version that returns format string for sink creation
pub fn get_output_format_from_cli(matches: &ArgMatches) -> String {
    // Try both with hyphen and underscore for compatibility
    let format = matches
        .get_one::<String>("output-format")
        .or_else(|| matches.get_one::<String>("output_format"));

    format
        .map(|s| s.to_string())
        .unwrap_or_else(|| "auto".to_string())
}

/// Check if no-color flag is set
pub fn get_no_color_from_cli(matches: &ArgMatches) -> bool {
    matches.get_flag("no-color")
}
