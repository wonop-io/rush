use rush_output::event::{CompileStage, LogLevel, OutputEvent, OutputMetadata};
use rush_output::session::{OutputMode, OutputSession, SessionBuilder};
use rush_output::source::OutputSource;
use rush_output::stream::OutputStream;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Test each layout mode
    let modes = vec![
        ("Simple", OutputMode::Simple),
        ("Split", OutputMode::Split),
        ("Dashboard", OutputMode::Dashboard),
        ("Web", OutputMode::Web),
    ];

    for (name, mode) in modes {
        println!("\n{}", "=".repeat(60));
        println!("Testing {} Mode", name);
        println!("{}\n", "=".repeat(60));

        let mut session = SessionBuilder::new().mode(mode).build()?;

        // Simulate build phase
        let backend_source = OutputSource::new("backend", "build");
        let build_event = OutputEvent::compile_time(
            backend_source.clone(),
            CompileStage::Compilation,
            "backend".to_string(),
            OutputStream::stdout(b"Compiling backend service...\n".to_vec()),
        );
        session.submit(build_event).await?;

        // Simulate another build event
        let frontend_source = OutputSource::new("frontend", "build");
        let frontend_build = OutputEvent::compile_time(
            frontend_source.clone(),
            CompileStage::Compilation,
            "frontend".to_string(),
            OutputStream::stdout(b"Building frontend assets...\n".to_vec()),
        );
        session.submit(frontend_build).await?;

        // Simulate runtime phase
        let backend_runtime = OutputEvent::runtime(
            OutputSource::new("backend", "container"),
            OutputStream::stdout(b"Server started on port 8080\n".to_vec()),
            Some("container_123".to_string()),
        );
        session.submit(backend_runtime).await?;

        // Simulate system event
        let system_event = OutputEvent::system(
            OutputSource::new("rush", "system"),
            "network".to_string(),
            OutputStream::stdout(b"Docker network created\n".to_vec()),
        );
        session.submit(system_event).await?;

        // Simulate an error
        let mut error_event = OutputEvent::runtime(
            OutputSource::new("database", "container"),
            OutputStream::stderr(b"ERROR: Connection refused\n".to_vec()),
            None,
        );
        error_event.metadata = OutputMetadata::default().with_level(LogLevel::Error);
        session.submit(error_event).await?;

        session.flush().await?;

        // Small delay to see the output
        sleep(Duration::from_millis(500)).await;
    }

    println!("\n{}", "=".repeat(60));
    println!("All layout modes tested successfully!");
    println!("{}", "=".repeat(60));

    Ok(())
}

type Result<T> = std::result::Result<T, rush_core::error::Error>;
