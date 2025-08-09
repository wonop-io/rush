use super::*;

pub async fn execute(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    trace!("Building components");
    match ctx.reactor.build().await {
        Ok(_) => {
            trace!("Build completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Build failed: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

pub async fn push(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    trace!("Building and pushing components");
    match ctx.reactor.build_and_push().await {
        Ok(_) => {
            trace!("Build and push completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Build and push failed: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
