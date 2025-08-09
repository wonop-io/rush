use super::*;
use crate::create_k8s_validator;
use log::trace;
use std::process;

pub async fn execute(matches: &ArgMatches, ctx: &mut CliContext) -> Result<(), std::io::Error> {
    let _pop_dir = Directory::chdir(ctx.reactor.product_directory());
    if let Some(manifest_matches) = matches.subcommand_matches("manifests") {
        validate_manifests(manifest_matches, ctx).await
    } else {
        Ok(())
    }
}

async fn validate_manifests(
    matches: &ArgMatches,
    ctx: &mut CliContext,
) -> Result<(), std::io::Error> {
    let target_version = matches
        .get_one::<String>("version")
        .map(|v| v.as_str())
        .unwrap_or_else(|| ctx.config.k8s_version());
    let validator = create_k8s_validator(&ctx.config);

    let mut validation_failed = false;
    for component in ctx.reactor.cluster_manifests().components() {
        trace!(
            "Validating manifests for component: {}",
            component.spec().component_name
        );
        if let Err(e) = validator.validate(
            component.output_directory().to_str().unwrap(),
            target_version,
        ) {
            error!(
                "Validation failed for {}: {}",
                component.spec().component_name,
                e
            );
            validation_failed = true;
        }
    }

    if validation_failed {
        println!("One or more components failed validation!");
        process::exit(1);
    } else {
        println!("All manifests validated successfully!");
        process::exit(0);
    }
    #[allow(unreachable_code)]
    Ok(())
}
