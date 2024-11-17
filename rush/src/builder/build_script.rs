use crate::builder::BuildType;
use std::error::Error;
use tera::Context;

use crate::builder::BuildContext;
use crate::builder::TEMPLATES;

pub struct BuildScript {
    build_type: BuildType,
}

impl BuildScript {
    pub fn new(build_type: BuildType) -> Self {
        BuildScript { build_type }
    }

    pub fn render(&self, context: &BuildContext) -> String {
        let context = Context::from_serialize(context).expect("Could not create context");

        match &self.build_type {
            BuildType::TrunkWasm { .. } => {
                match TEMPLATES.render("build/wasm_trunk.sh", &context) {
                    Ok(s) => s,
                    Err(e) => {
                        println!("Error: {}", e);
                        let mut cause = e.source();
                        while let Some(e) = cause {
                            println!("Reason: {}", e);
                            cause = e.source();
                        }
                        panic!("Failed rendering");
                    }
                }
            }
            BuildType::RustBinary { .. } => {
                match TEMPLATES.render("build/rust_binary.sh", &context) {
                    Ok(s) => s,
                    Err(e) => {
                        println!("Error: {}", e);
                        let mut cause = e.source();
                        while let Some(e) = cause {
                            println!("Reason: {}", e);
                            cause = e.source();
                        }
                        panic!("Failed rendering");
                    }
                }
            }
            BuildType::Zola { .. } => match TEMPLATES.render("build/zola.sh", &context) {
                Ok(s) => s,
                Err(e) => {
                    println!("Error: {}", e);
                    let mut cause = e.source();
                    while let Some(e) = cause {
                        println!("Reason: {}", e);
                        cause = e.source();
                    }
                    panic!("Failed rendering");
                }
            },
            BuildType::Book { .. } => match TEMPLATES.render("build/mdbook.sh", &context) {
                Ok(s) => s,
                Err(e) => {
                    println!("Error: {}", e);
                    let mut cause = e.source();
                    while let Some(e) = cause {
                        println!("Reason: {}", e);
                        cause = e.source();
                    }
                    panic!("Failed rendering");
                }
            },
            BuildType::Script { .. } => "".to_string(),
            BuildType::PureKubernetes => "".to_string(),
            BuildType::KubernetesInstallation { .. } => "".to_string(),
            BuildType::Ingress { .. } => "".to_string(),
            BuildType::PureDockerImage { .. } => "".to_string(),
        }
    }
}
