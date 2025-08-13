use rush_output::prelude::*;
use rush_output::event::{CompileStage, LogLevel, OutputMetadata};
use rush_output::sink::{TerminalLayout, TerminalSink};
use rush_output::formatter::PlainFormatter;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("Testing direct layout configuration...\n");
    
    // Test each layout directly
    let layouts = vec![
        ("Linear", TerminalLayout::Linear),
        ("Split", TerminalLayout::Split { 
            panes: vec![
                rush_output::sink::PaneConfig::new("Build"),
                rush_output::sink::PaneConfig::new("Runtime"),
            ]
        }),
        ("Dashboard", TerminalLayout::Dashboard { widgets: vec![] }),
        ("Tree", TerminalLayout::Tree),
        ("Web", TerminalLayout::Web),
    ];
    
    for (name, layout) in layouts {
        println!("\n{}", "=".repeat(60));
        println!("Testing {} Layout", name);
        println!("{}\n", "=".repeat(60));
        
        // Create a sink with the specific layout
        let sink = Box::new(TerminalSink::new()
            .with_formatter(Box::new(PlainFormatter::default()))
            .with_layout(layout));
        
        let mut session = rush_output::session::SessionBuilder::new()
            .sinks(vec![sink])
            .build()?;
        
        // Simulate build phase
        let backend_source = OutputSource::new("backend", "build");
        let build_event = OutputEvent::compile_time(
            backend_source.clone(),
            CompileStage::Compilation,
            "backend".to_string(),
            OutputStream::stdout(b"Compiling backend service...\n".to_vec()),
        );
        session.submit(build_event).await?;
        
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
        
        session.flush().await?;
        
        // Small delay to see the output
        sleep(Duration::from_millis(100)).await;
    }
    
    println!("\n{}", "=".repeat(60));
    println!("All layouts tested successfully!");
    println!("{}", "=".repeat(60));
    
    Ok(())
}

type Result<T> = std::result::Result<T, rush_core::error::Error>;