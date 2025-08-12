use rush_cli::{args, context_builder, execute, init};
use rush_core::shutdown;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    // Set up signal handlers for graceful shutdown
    shutdown::setup_signal_handlers();

    // Parse command line arguments
    let matches = args::parse_args();

    // Set up logging based on command line arguments
    context_builder::setup_logging(&matches);

    // Initialize the application
    match init::init_application().await {
        Ok(_) => (),
        Err(e) => {
            eprintln!("Failed to initialize application: {e}");
            std::process::exit(1);
        }
    }

    // Initialize CLI context with common resources
    let mut ctx = match context_builder::create_context(&matches).await {
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
