use rush_output::prelude::*;
use rush_output::event::{CompileStage, LogLevel, OutputMetadata};
use rush_output::config::create_session_from_config;
use rush_config::loader::DevOutputConfig;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Example 1: Simple session with default configuration
    println!("=== Example 1: Simple Output Session ===\n");
    
    let mut session = OutputSession::builder()
        .mode(rush_output::session::OutputMode::Simple)
        .build()?;
    
    // Create some example events
    let backend_source = OutputSource::new("backend", "container");
    let frontend_source = OutputSource::new("frontend", "container");
    
    // Compile-time event
    let compile_event = OutputEvent::compile_time(
        backend_source.clone(),
        CompileStage::Compilation,
        "backend".to_string(),
        OutputStream::stdout(b"Compiling backend service...\n".to_vec()),
    );
    
    session.submit(compile_event).await?;
    
    // Runtime event
    let runtime_event = OutputEvent::runtime(
        backend_source.clone(),
        OutputStream::stdout(b"Server listening on port 8080\n".to_vec()),
        Some("container_123".to_string()),
    );
    
    session.submit(runtime_event).await?;
    
    // Error event with metadata
    let mut error_event = OutputEvent::runtime(
        frontend_source,
        OutputStream::stderr(b"Error: Failed to connect to database\n".to_vec()),
        None,
    );
    error_event.metadata = OutputMetadata::default()
        .with_level(LogLevel::Error)
        .with_tag("component", "database-connector");
    
    session.submit(error_event).await?;
    
    session.flush().await?;
    
    println!("\nSession stats:");
    println!("  Events processed: {}", session.stats().events_processed);
    println!("  Events filtered: {}", session.stats().events_filtered);
    println!("  Events routed: {}", session.stats().events_routed);
    
    // Example 2: Session with filtering
    println!("\n=== Example 2: Filtered Output Session ===\n");
    
    let mut filtered_session = OutputSession::builder()
        .filter(Box::new(ComponentFilter::allowlist(vec!["backend".to_string()])))
        .filter(Box::new(PhaseFilter::runtime()))
        .build()?;
    
    // This will pass the filters
    let backend_runtime = OutputEvent::runtime(
        OutputSource::new("backend", "container"),
        OutputStream::stdout(b"Backend: Processing request\n".to_vec()),
        None,
    );
    filtered_session.submit(backend_runtime).await?;
    
    // This will be filtered out (wrong component)
    let frontend_runtime = OutputEvent::runtime(
        OutputSource::new("frontend", "container"),
        OutputStream::stdout(b"Frontend: Rendering page\n".to_vec()),
        None,
    );
    filtered_session.submit(frontend_runtime).await?;
    
    // This will be filtered out (wrong phase)
    let backend_compile = OutputEvent::compile_time(
        OutputSource::new("backend", "container"),
        CompileStage::Compilation,
        "backend".to_string(),
        OutputStream::stdout(b"Compiling backend...\n".to_vec()),
    );
    filtered_session.submit(backend_compile).await?;
    
    println!("Filtered session stats:");
    println!("  Events processed: {}", filtered_session.stats().events_processed);
    println!("  Events filtered: {}", filtered_session.stats().events_filtered);
    println!("  Events routed: {}", filtered_session.stats().events_routed);
    
    // Example 3: Session from configuration
    println!("\n=== Example 3: Session from Configuration ===\n");
    
    let config = DevOutputConfig::default();
    let _config_session = create_session_from_config(&config)?;
    
    println!("Created session from default configuration");
    println!("  Mode: auto");
    println!("  Log level: info");
    println!("  Colors: auto");
    
    Ok(())
}

type Result<T> = std::result::Result<T, rush_core::error::Error>;