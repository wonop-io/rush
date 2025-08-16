//! MCP server command for Rush

use clap::{Arg, ArgMatches, Command};
use log::info;
use rush_core::error::{Error, Result};
use rush_mcp::{McpServer, McpServerConfig, StdioTransport};

/// Create the MCP subcommand
pub fn create_command() -> Command {
    Command::new("mcp")
        .about("Model Context Protocol (MCP) server for Rush")
        .subcommand(
            Command::new("serve")
                .about("Start the MCP server")
                .arg(
                    Arg::new("stdio")
                        .long("stdio")
                        .help("Use stdio transport (for subprocess mode)")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("port"),
                )
                .arg(
                    Arg::new("port")
                        .long("port")
                        .short('p')
                        .help("Port to listen on (for network mode)")
                        .value_parser(clap::value_parser!(u16))
                        .default_value("3333"),
                )
                .arg(
                    Arg::new("buffer-size")
                        .long("buffer-size")
                        .help("Maximum log buffer size")
                        .value_parser(clap::value_parser!(usize))
                        .default_value("1000"),
                ),
        )
}

/// Execute the MCP command
pub async fn execute(matches: &ArgMatches) -> Result<()> {
    match matches.subcommand() {
        Some(("serve", serve_matches)) => serve(serve_matches).await,
        _ => {
            eprintln!("Usage: rush mcp serve [OPTIONS]");
            eprintln!("Try 'rush mcp --help' for more information.");
            Ok(())
        }
    }
}

/// Start the MCP server
async fn serve(matches: &ArgMatches) -> Result<()> {
    let use_stdio = matches.get_flag("stdio");
    let port = *matches.get_one::<u16>("port").unwrap_or(&3333);
    let buffer_size = *matches.get_one::<usize>("buffer-size").unwrap_or(&1000);

    let config = McpServerConfig {
        name: "rush-mcp".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        buffer_size,
        enable_experimental: false,
    };

    if use_stdio {
        info!("Starting MCP server in stdio mode");
        eprintln!("Rush MCP server starting in stdio mode...");
        
        // Create stdio transport
        let transport = StdioTransport::new()
            .map_err(|e| Error::Other(format!("Failed to create stdio transport: {}", e)))?;
        
        // Create and run server
        let server = McpServer::new(config);
        server
            .run(transport)
            .await
            .map_err(|e| Error::Other(format!("MCP server error: {}", e)))?;
    } else {
        // Network mode not yet implemented
        eprintln!("Network mode not yet implemented. Use --stdio for subprocess mode.");
        eprintln!("");
        eprintln!("Example MCP client configuration:");
        eprintln!("{{");
        eprintln!("  \"mcpServers\": {{");
        eprintln!("    \"rush\": {{");
        eprintln!("      \"command\": \"rush\",");
        eprintln!("      \"args\": [\"mcp\", \"serve\", \"--stdio\"],");
        eprintln!("      \"env\": {{}}");
        eprintln!("    }}");
        eprintln!("  }}");
        eprintln!("}}");
        return Err(Error::Other("Network mode not implemented".into()));
    }

    Ok(())
}