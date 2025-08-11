use crate::error::Result;
use crate::output::{
    BufferedOutputDirector, OutputDirector, OutputSource, OutputStream, StdOutputDirector,
};

/// Example usage of the OutputDirector interface
pub async fn demonstrate_output_director() -> Result<()> {
    // Create a standard output director
    let std_director = StdOutputDirector::new();

    // Wrap it in a buffered director for line-based output
    let mut buffered_director = BufferedOutputDirector::new(std_director);

    // Create some output sources
    let database_source =
        OutputSource::with_color("helloworld.wonop.io-database", "container", "yellow");
    let backend_source =
        OutputSource::with_color("helloworld.wonop.io-backend", "container", "blue");
    let frontend_source =
        OutputSource::with_color("helloworld.wonop.io-frontend", "container", "purple");

    // Simulate some output streams
    let database_log = OutputStream::stdout(
        b"PostgreSQL Database system is ready to accept connections\n".to_vec(),
    );
    let backend_log =
        OutputStream::stdout("🚀 Server is successfully on FIRE!\n".as_bytes().to_vec());
    let frontend_log =
        OutputStream::stdout(b"Starting development server on http://localhost:3000\n".to_vec());
    let error_log = OutputStream::stderr(b"Warning: Deprecated API usage detected\n".to_vec());

    // Write the outputs through the director
    buffered_director
        .write_output(&database_source, &database_log)
        .await?;
    buffered_director
        .write_output(&backend_source, &backend_log)
        .await?;
    buffered_director
        .write_output(&frontend_source, &frontend_log)
        .await?;
    buffered_director
        .write_output(&backend_source, &error_log)
        .await?;

    // Simulate partial line output that gets buffered
    let partial1 = OutputStream::stdout(b"Loading configuration ".to_vec());
    let partial2 = OutputStream::stdout(b"files... ".to_vec());
    let partial3 = OutputStream::stdout(b"done!\n".to_vec());

    buffered_director
        .write_output(&database_source, &partial1)
        .await?;
    buffered_director
        .write_output(&database_source, &partial2)
        .await?;
    buffered_director
        .write_output(&database_source, &partial3)
        .await?;

    // Flush any remaining buffered output
    buffered_director.flush().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_output_director_demonstration() {
        let result = demonstrate_output_director().await;
        assert!(result.is_ok());
    }
}
