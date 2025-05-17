use super::*;
use colored::Colorize;
use std::process;
use tera::Context;

pub async fn execute(matches: &ArgMatches, ctx: &mut CliContext) -> Result<(), std::io::Error> {
    trace!("Executing 'describe' subcommand");

    if let Some(_) = matches.subcommand_matches("toolchain") {
        describe_toolchain(ctx).await
    } else if let Some(_) = matches.subcommand_matches("images") {
        describe_images(ctx).await
    } else if let Some(_) = matches.subcommand_matches("services") {
        describe_services(ctx).await
    } else if let Some(build_script_matches) = matches.subcommand_matches("build-script") {
        describe_build_script(build_script_matches, ctx).await
    } else if let Some(build_context_matches) = matches.subcommand_matches("build-context") {
        describe_build_context(build_context_matches, ctx).await
    } else if let Some(artefacts_matches) = matches.subcommand_matches("artefacts") {
        describe_artefacts(artefacts_matches, ctx).await
    } else if let Some(_) = matches.subcommand_matches("k8s") {
        describe_k8s(ctx).await
    } else {
        Ok(())
    }
}

async fn describe_toolchain(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    println!("{:#?}", ctx.toolchain);
    debug!("Described toolchain");
    process::exit(0);
    #[allow(unreachable_code)]
    Ok(())
}

async fn describe_images(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    println!("{:#?}", ctx.reactor.images());
    debug!("Described images");
    process::exit(0);
    #[allow(unreachable_code)]
    Ok(())
}

async fn describe_services(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    println!("{:#?}", ctx.reactor.services());
    debug!("Described services");
    process::exit(0);
    #[allow(unreachable_code)]
    Ok(())
}

async fn describe_build_script(
    matches: &ArgMatches,
    ctx: &mut CliContext,
) -> Result<(), std::io::Error> {
    trace!("Describing build script");
    if let Some(component_name) = matches.get_one::<String>("component_name") {
        trace!("Describing build script for component: {}", component_name);
        let image = ctx
            .reactor
            .get_image(component_name)
            .expect("Component not found");
        let secrets = ctx
            .vault
            .lock()
            .unwrap()
            .get(&ctx.product_name, component_name, &ctx.environment)
            .await
            .unwrap_or_default();
        let build_ctx = image.generate_build_context(secrets);

        println!("{}", image.build_script(&build_ctx).unwrap());
        debug!("Described build script for component: {}", component_name);
        process::exit(0);
    }
    error!("No component name provided");
    process::exit(1);
    #[allow(unreachable_code)]
    Ok(())
}

async fn describe_build_context(
    matches: &ArgMatches,
    ctx: &mut CliContext,
) -> Result<(), std::io::Error> {
    if let Some(component_name) = matches.get_one::<String>("component_name") {
        trace!("Describing build context for component: {}", component_name);
        let image = ctx
            .reactor
            .get_image(component_name)
            .expect("Component not found");
        let secrets = ctx
            .vault
            .lock()
            .unwrap()
            .get(&ctx.product_name, component_name, &ctx.environment)
            .await
            .unwrap_or_default();
        let build_ctx = image.generate_build_context(secrets);
        let tera_ctx = Context::from_serialize(build_ctx).expect("Could not create context");
        println!("{:#?}", tera_ctx);
        debug!("Described build context for component: {}", component_name);
        process::exit(0);
    }
    error!("No component name provided");
    process::exit(1);
    #[allow(unreachable_code)]
    Ok(())
}

async fn describe_artefacts(
    matches: &ArgMatches,
    ctx: &mut CliContext,
) -> Result<(), std::io::Error> {
    if let Some(component_name) = matches.get_one::<String>("component_name") {
        let _pop_dir = Directory::chdir(ctx.reactor.product_directory());
        trace!("Describing artefacts for component: {}", component_name);
        let image = ctx
            .reactor
            .get_image(component_name)
            .expect("Component not found");
        let secrets = ctx
            .vault
            .lock()
            .unwrap()
            .get(&ctx.product_name, component_name, &ctx.environment)
            .await
            .unwrap_or_default();
        let build_ctx = image.generate_build_context(secrets);
        for (k, v) in image.spec().build_artefacts() {
            let message = format!("{} {}", "Artefact".green(), k.white());
            println!("{}\n", &message.bold());

            println!("{}\n", v.render(&build_ctx));
        }
        debug!("Described artefacts for component: {}", component_name);
        process::exit(0);
    }
    error!("No component name provided");
    process::exit(1);
    #[allow(unreachable_code)]
    Ok(())
}

async fn describe_k8s(ctx: &mut CliContext) -> Result<(), std::io::Error> {
    trace!("Describing Kubernetes manifests");
    let manifests = ctx.reactor.cluster_manifests();
    for component in manifests.components() {
        println!(
            "{} -> {}",
            component.input_directory().display(),
            component.output_directory().display()
        );
        let spec = component.spec();
        let secrets = ctx
            .vault
            .lock()
            .unwrap()
            .get(&ctx.product_name, &spec.component_name, &ctx.environment)
            .await
            .unwrap_or_default();
        let build_ctx = spec.generate_build_context(Some(ctx.toolchain.clone()), secrets);
        for manifest in component.manifests() {
            println!("{}", manifest.render(&build_ctx));
        }
        println!();
    }
    debug!("Described Kubernetes manifests");
    process::exit(0);
    #[allow(unreachable_code)]
    Ok(())
}
