#\!/bin/bash
cd /Users/tfr/Documents/Projects/rush/rush
mkdir -p ./test_output

cat > test_main.rs << 'RUST'
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    println\!("Testing FileOutputDirector...");
    
    // Test file creation
    match rush_cli::output::FileOutputDirector::new("./test_output").await {
        Ok(mut director) => {
            println\!("FileOutputDirector created successfully\!");
            
            let source = rush_cli::output::OutputSource::new("test-container", "container");
            let data = b"Hello from test\!\n".to_vec();
            let stream = rush_cli::output::OutputStream::stdout(data);
            
            match director.write_output(&source, &stream).await {
                Ok(_) => println\!("Output written successfully\!"),
                Err(e) => println\!("Failed to write output: {}", e),
            }
            
            match director.flush().await {
                Ok(_) => println\!("Flushed successfully\!"),
                Err(e) => println\!("Failed to flush: {}", e),
            }
        }
        Err(e) => {
            println\!("Failed to create FileOutputDirector: {}", e);
        }
    }
    
    // Check if file was created
    if std::path::Path::new("./test_output/test-container.log").exists() {
        println\!("SUCCESS: Log file was created\!");
        if let Ok(contents) = std::fs::read_to_string("./test_output/test-container.log") {
            println\!("Contents:\n{}", contents);
        }
    } else {
        println\!("ERROR: Log file was not created\!");
    }
}
RUST

cargo run --bin rush --features=test --manifest-path Cargo.toml -- --help >/dev/null 2>&1
cargo build --lib 2>/dev/null
rustc --edition 2021 test_main.rs -L target/debug -L target/debug/deps --extern rush_cli=target/debug/librush_cli.rlib $(find target/debug/deps -name "*.rlib" -exec echo "--extern $(basename {} .rlib | sed 's/-[^-]*$//')={}" \; | sort -u | head -20) 2>/dev/null && ./test_main
