use super::*;

pub async fn execute(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    // First check for kubectl
    if !ctx.toolchain.has_kubectl() {
        error!("kubectl not found");
        eprintln!("kubectl not found");
        process::exit(1);
    }

    // Select kubernetes context
    match ctx
        .reactor
        .select_kubernetes_context(ctx.config.kube_context())
        .await
    {
        Ok(_) => (),
        Err(e) => {
            error!("Failed to select kubernetes context: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }

    trace!("Rolling out to kubernetes");
    match ctx.reactor.rollout().await {
        Ok(_) => {
            trace!("Rollout completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Rollout failed: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
