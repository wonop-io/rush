use colored::Colorize;
use rush_cli::{args, context_builder, execute, init, logging};
use rush_core::shutdown;
use rush_helper::{run_preflight_checks, HelperError};

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    // Set up signal handlers for graceful shutdown
    shutdown::setup_signal_handlers();

    // Parse command line arguments
    let matches = args::parse_args();

    // Create the global output sink
    let output_sink = logging::create_output_sink(&matches);

    // Set up logging to route through the sink
    if let Err(e) = logging::setup_logging_with_sink(&matches, output_sink.clone()) {
        // Fallback to env_logger if sink setup fails
        eprintln!("Failed to setup sink-based logging: {}, falling back to env_logger", e);
        context_builder::setup_logging(&matches);
    }

    // Initialize tracing for profiling if enabled
    if std::env::var("RUSH_PROFILE").is_ok() || matches.subcommand_matches("profile").is_some() {
        rush_container::profiling::init_tracing();
    }

    // Handle MCP command early (doesn't need context)
    if let Some(mcp_matches) = matches.subcommand_matches("mcp") {
        return match rush_cli::commands::mcp::execute(mcp_matches).await {
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!("MCP server error: {}", e);
                std::process::exit(1);
            }
        };
    }

    // Handle check-deps command early (doesn't need context)
    if matches.subcommand_matches("check-deps").is_some() {
        println!("{}", "🔍 Checking rush dependencies...".cyan().bold());

        match rush_helper::check_all_requirements() {
            Ok(_) => {
                println!("{}", "✅ All dependencies are installed!".green().bold());

                // Also show the rush version
                if let Ok(version) = rush_helper::checks::check_rush_version() {
                    println!("📦 Rush version: {}", version.green());
                }

                println!("\n{}", "Platform information:".yellow());
                println!("  Platform: {}", rush_helper::get_platform());
                println!(
                    "  Apple Silicon: {}",
                    if rush_helper::is_apple_silicon() {
                        "Yes"
                    } else {
                        "No"
                    }
                );

                return Ok(());
            }
            Err(e) => {
                eprintln!("{}", "❌ Missing dependencies detected:".red().bold());
                eprintln!("{}", e.get_message());

                let commands = e.get_fix_commands();
                if !commands.is_empty() {
                    eprintln!("\n{}", "📦 To fix these issues, run:".yellow().bold());
                    for cmd in commands {
                        eprintln!("  {}", cmd.join(" ").green());
                    }
                }

                std::process::exit(1);
            }
        }
    }

    // Run preflight checks to ensure all required tools are installed (unless skipped)
    if !matches.get_flag("skip_checks") {
        if let Err(e) = run_preflight_checks() {
            eprintln!("{}", "❌ Missing dependencies detected:".red().bold());
            eprintln!("{}", e.get_message());

            let commands = e.get_fix_commands();
            if !commands.is_empty() {
                eprintln!("\n{}", "📦 To fix these issues, run:".yellow().bold());
                for cmd in commands {
                    eprintln!("  {}", cmd.join(" ").green());
                }
            }

            // Special handling for the specific error the user encountered
            if let HelperError::MissingTarget { .. } = e {
                eprintln!("\n{}", "💡 Tip: After installing missing targets, you may need to configure your linker.".yellow());
                eprintln!("{}", "   For Apple Silicon cross-compilation, ensure you have the x86_64 toolchain installed.".yellow());
            }

            eprintln!(
                "\n{}",
                "To skip these checks, use: rush --skip-checks".cyan()
            );
            std::process::exit(1);
        }
    }

    // Initialize the application
    match init::init_application().await {
        Ok(_) => (),
        Err(e) => {
            eprintln!("Failed to initialize application: {e}");
            std::process::exit(1);
        }
    }

    // Initialize CLI context with common resources
    let mut ctx = match context_builder::create_context(&matches, output_sink.clone()).await {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("Failed to create context: {e}");
            std::process::exit(1);
        }
    };

    // Execute the appropriate command based on arguments
    match execute::execute_command(&matches, &mut ctx).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Command failed: {e}");
            std::process::exit(1);
        }
    }
}
