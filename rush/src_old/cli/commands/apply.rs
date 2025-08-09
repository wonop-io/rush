use super::*;

pub async fn execute(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    ensure_kubectl(ctx).await?;

    trace!("Applying kubernetes manifests");
    match ctx.reactor.apply().await {
        Ok(_) => {
            trace!("Apply completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Apply failed: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

pub async fn unapply(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    ensure_kubectl(ctx).await?;

    trace!("Unapplying kubernetes manifests");
    match ctx.reactor.unapply().await {
        Ok(_) => {
            trace!("Unapply completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Unapply failed: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

async fn ensure_kubectl(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    if !ctx.toolchain.has_kubectl() {
        error!("kubectl not found");
        eprintln!("kubectl not found");
        process::exit(1);
    }

    match ctx
        .reactor
        .select_kubernetes_context(ctx.config.kube_context())
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("Failed to select kubernetes context: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}
