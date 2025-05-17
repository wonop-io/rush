use super::*;

pub async fn execute(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    ensure_kubectl(ctx).await?;

    trace!("Installing manifests");
    match ctx.reactor.install_manifests().await {
        Ok(_) => {
            trace!("Installation completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Installation failed: {}", e);
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

pub async fn uninstall(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    ensure_kubectl(ctx).await?;

    trace!("Uninstalling manifests");
    match ctx.reactor.uninstall_manifests().await {
        Ok(_) => {
            trace!("Uninstallation completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Uninstallation failed: {}", e);
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
