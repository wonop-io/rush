use rush_cli::cli;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    // Initialize the application
    match cli::init_application().await {
        Ok(_) => (),
        Err(e) => {
            eprintln!("Failed to initialize application: {}", e);
            std::process::exit(1);
        }
    }

    // Parse command line arguments
    let matches = cli::parse_args();

    // Initialize CLI context with common resources
    let mut ctx = match cli::create_context(&matches).await {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("Failed to create context: {}", e);
            std::process::exit(1);
        }
    };

    // Execute the appropriate command based on arguments
    match cli::execute_command(&matches, &mut ctx).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Command failed: {}", e);
            std::process::exit(1);
        }
    }
}
