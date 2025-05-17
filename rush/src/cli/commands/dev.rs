use super::*;

pub async fn execute(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    trace!("Launching development environment");
    match ctx.reactor.launch().await {
        Ok(_) => {
            trace!("Development environment launched successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to launch development environment: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
