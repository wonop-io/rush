use std::{
    iter::Product, sync::mpsc::{self, Sender}
};
use tokio::sync::broadcast::{Receiver as BroadcastReceiver};

use colored::Colorize;
use tokio::process::Command;
use super::status::Status;
use std::sync::Arc;
use crate::{toolchain::ToolchainContext, utils::DockerCrossCompileGuard};
use crate::builder::{ComponentBuildSpec};
use crate::utils::{handle_stream, run_command, run_command_in_window};
use crate::builder::{BuildContext};
use crate::builder::BuildType;
use crate::container::{ServicesSpec, ServiceSpec};
use std::collections::HashMap;
use crate::Directory;
use std::path::Path;
use std::sync::Mutex;
use core::cell::RefCell;
use std::rc::Rc;
use crate::builder::Config;

impl TryInto<DockerImage> for Arc<Mutex<ComponentBuildSpec>> {
    type Error = String;
    fn try_into(self) -> Result<DockerImage, String> {
        DockerImage::from_docker_spec(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct DockerImage {
    
    image_name: String,
    repo: Option<String>,
    tag: Option<String>, 
    depends_on: Vec<String>,
    context_dir: Option<String>,

    // Derived from Dockerfile
    exposes: Vec<String>,

    port: Option<u16>,
    target_port: Option<u16>,

    // Spec
    config: Arc<Config>,
    spec: Arc<Mutex<ComponentBuildSpec>>,
    toolchain: Option<Arc<ToolchainContext>>,
    network_name: Option<String>
}


impl DockerImage {
    pub fn depends_on(&self) -> &Vec<String> {
        &self.depends_on
    }
    pub fn image_name(&self) -> &str {
        &self.image_name
    }
    pub fn set_network_name(&mut self, network_name: String) {
        self.network_name = Some(network_name);
    }

    pub fn create_cross_compile_guard(build_type: &BuildType, toolchain: &ToolchainContext) -> DockerCrossCompileGuard {
        let target = match build_type {
            BuildType::PureDockerImage{ .. } => toolchain.host(),
            _ => toolchain.target()
        };

        DockerCrossCompileGuard::new(&target.to_docker_target())    
    }

    pub fn from_docker_spec(spec: Arc<Mutex<ComponentBuildSpec>>) -> Result<Self, String> {
        let orig_spec = spec.clone();
        let spec = spec.lock().unwrap();
        let config = spec.config();
        let (dockerfile_path, context_dir) = match &spec.build_type {
            BuildType::TrunkWasm{ dockerfile_path, context_dir, .. } => (Some(dockerfile_path.clone()), context_dir.clone()),
            BuildType::RustBinary{ dockerfile_path, context_dir,.. } => (Some(dockerfile_path.clone()), context_dir.clone()),
            BuildType::Script{ dockerfile_path, context_dir,.. } => (Some(dockerfile_path.clone()), context_dir.clone()),
            BuildType::Ingress{ dockerfile_path, context_dir, ..} => (Some(dockerfile_path.clone()), context_dir.clone()),
            _ => (None, None)        
        };

        let (port, target_port, exposes) = if let Some(dockerfile_path) = dockerfile_path {
            let dockerfile_contents = std::fs::read_to_string(&dockerfile_path)
                .expect(&format!("Failed to read Dockerfile: {}", dockerfile_path).to_string());

            let exposes = dockerfile_contents.lines()
                .map(|line| line.trim())
                .filter(|line| line.starts_with("EXPOSE"))
                .map(|line| line.trim_start_matches("EXPOSE").trim().to_string())
                .collect::<Vec<_>>();

            let port = match exposes.first() {
                
                Some(port) => Some(port.parse::<u16>().unwrap()),
                None => None,
            };
            let target_port = port.clone();
            (port, target_port, exposes)
        } else {
            (None, None, Vec::new())
        };

        // Spec overrides auto deduced ports
        let port = if let Some(p) = spec.port {
            Some(p)
        } else {
            port
        };

        let target_port = if let Some(p) = spec.target_port {
            Some(p)
        } else {
            target_port
        };

        let (image_name, tag) = match &spec.build_type {
            BuildType::PureDockerImage{ image_name_with_tag,.. } => {
                let split = image_name_with_tag.split(":").collect::<Vec<&str>>();
                if split.len() > 2 {
                    panic!("Image name with tag should not contain more than one colon");
                }
                else if split.len() == 2 {
                    (split.first().unwrap().to_string(), Some(split.last().unwrap().to_string()))
                } else {
                    (split.first().unwrap().to_string(), None)
                }
            }
            _ => (format!("{}-{}", spec.product_name, spec.component_name), None),
        };

        let product_name = spec.product_name.clone();
        let depends_on = spec.depends_on.iter().map(move |s| 
            format!("{}-{}", product_name, s)).collect::<Vec<String>>();
        Ok(DockerImage {
            image_name,
            repo: None, // Assuming repo is not part of ComponentBuildSpec and defaults to None
            depends_on,
            context_dir,
            tag,
            exposes,
            config,
            spec: orig_spec,
            port,
            target_port,
            toolchain: None,
            network_name: None
        })
    }

    pub fn port(&self) -> Option<u16> {
        self.port.clone()
    }

    pub fn target_port(&self) -> Option<u16> {
        self.target_port.clone()
    }
    
    pub fn set_port(&mut self, port: u16) {
        self.port = Some(port);
    }
    /*
    pub fn set_target_port(&mut self, target_port: u16) {
        self.target_port = Some(target_port);
    }

    pub fn set_color(&mut self, color: String) {
        self.spec.color = color;
    }

    pub fn set_product_name(&mut self, product_name: String) {
        self.spec.product_name = product_name;
        self.image_name = format!("{}-{}", self.spec.product_name, self.spec.component_name);    
    }

    pub fn set_component_name(&mut self, component_name: String) {
        self.spec.component_name = component_name;
        self.image_name = format!("{}-{}", self.spec.product_name, self.spec.component_name);    
    }
    */
    pub fn set_tag(&mut self, tag: String) {
        self.tag = Some(tag);
    }

    pub fn tagged_image_name(&self) -> String {
        format!("{}:{}", self.image_name, self.tag.clone().expect("Image is not tagged"))
    }

    pub fn set_toolchain(&mut self, toolchain: Arc<ToolchainContext>) {
        self.toolchain = Some(toolchain);
    }

    /*
    pub fn set_services(&mut self, services: Arc<ServicesSpec>) {
        self.spec.set_services(services);
    }
    */

    pub fn generate_build_context(&self) -> BuildContext {
        self.spec.lock().unwrap().generate_build_context(self.toolchain.clone())
    }

    pub fn build_script(&self, ctx: &BuildContext) ->Option<String> {
        let ret = self.spec.lock().unwrap().build_script(ctx);
        
        if ret.is_empty() {
            None
        } else {
            Some(ret)
        }
    }

    pub fn spec(&self) -> ComponentBuildSpec {
        self.spec.lock().unwrap().clone()
    }

    pub fn component_name(&self) -> String {
        self.spec.lock().unwrap().component_name.clone()
    }

    pub fn identifier(&self) -> String {
        match &self.repo {
            Some(r) => format!("{}/{}", r, self.tagged_image_name()),
            None => {
                match &self.spec.lock().unwrap().build_type {
                    BuildType::PureDockerImage{ image_name_with_tag,.. } => image_name_with_tag.clone(),
                    _ => self.tagged_image_name(),
                }
            }
        }
    }


    pub fn launch(&mut self, max_label_length: usize, mut terminate_receiver: BroadcastReceiver<()>, status_sender: Sender<Status>) -> tokio::task::JoinHandle<()> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };

        let _ = status_sender.send(Status::Awaiting);

        let task = self.clone();
        let network_name = self.network_name.clone().expect("Network name not set");

        let (command, entrypoint) = match &self.spec.lock().unwrap().build_type {
            BuildType::PureDockerImage{ command, entrypoint , ..} => (command.clone(), entrypoint.clone()),
            _ => (None, None),
        };

        tokio::spawn(async move { 
            let spec = task.spec.lock().unwrap().clone();
            let env_guard = DockerImage::create_cross_compile_guard(&spec.build_type, &toolchain);
            
            let show_arch = false; // TODO: Make a config parameter
            let formatted_label = if show_arch {
                format!("{} [{}]", spec.component_name, env_guard.target())
            } else {
                format!("{}", spec.component_name)
            };

            //task.clean().await;
            let _ = status_sender.send(Status::InProgress);
            let mut args = vec!["run".to_string(), "--name".to_string(), spec.component_name.clone(), "--network".to_string(), network_name];
            
            if let Some(entrypoint) = entrypoint {
                args.push("--entrypoint".to_string());
                args.push(entrypoint.clone());
            }            
            if let Some(port) = task.port {
                if let Some(target_port) = task.target_port {
                    args.push("-p".to_string());
                    args.push(format!("{}:{}", port, target_port));
                }
            }

            if let Some(env_vars) = &spec.env {
                for (key, value) in env_vars {
                    args.push("-e".to_string());
                    args.push(format!("{}={}", key, value));
                }
            }

            if let Some(volumes) = &spec.volumes {
                for (host_path, container_path) in volumes {
                    args.push("-v".to_string());
                    args.push(format!("{}:{}", host_path, container_path));
                }
            }

            for arg in &spec.docker_extra_run_args {
                args.push(arg.clone());
            }            

            args.push(task.tagged_image_name());
            if let Some(command) = command {
                args.push(command.clone());
            }

            println!("Running docker for {}: {}", spec.component_name, args.join(" "));
            let mut child_process_result = Command::new(toolchain.docker())
                .args(args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            match child_process_result {
                Ok(ref mut child) => {
                    let (stdout, stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());

                    let formatted_label = format!("{:width$}", formatted_label, width = max_label_length).color(spec.color.as_str()).bold();
                    let (tx, rx) = mpsc::channel();

                    let stdout_task = tokio::spawn(handle_stream(stdout, tx.clone()));
                    let stderr_task = tokio::spawn(handle_stream(stderr, tx));

                    
                    let lines = Arc::new(Mutex::new(Vec::new()));
                    let lines_clone = lines.clone();
                    let formatted_label_clone = formatted_label.clone();

                    tokio::spawn(async move {
                        while let Ok(line) = rx.recv() {
                            let mut lines = lines_clone.lock().unwrap();
                            lines.push(line.trim_end().to_string());
                            let clean_line = line.trim_end().replace("\r", "").replace("\n",""); // .replace("\x1B", "")
                            println!("{} |   {}", formatted_label_clone, clean_line);
                        }
                    });

                    tokio::select! {
                        _ = futures::future::join_all(vec![stdout_task, stderr_task]) => {

                        }
                        _ =  terminate_receiver.recv() => {
                            // TODO: See you can find something more cross-platform friendly
                            let mut kill = Command::new("kill")
                            .args(["-s", "TERM", &child.id().unwrap().to_string()])
                            .spawn().expect("Failed to kill process");
                        kill.wait().await.unwrap();
                            let _ = child.kill();
                            let _ = status_sender.send(Status::Terminate);                                
                        }
                    }                        


                    if let Some(code) = child.wait().await.unwrap().code() {
                        let message = format!("Process exited with code: {}", code);
                        println!("{} |   {}", formatted_label, message.bold().white());
                        let _ = status_sender.send(Status::Finished(code));
                    } else {
                        eprintln!("{}", format!("Terminating {}.", spec.component_name).bold().white());
                    }
                },
                Err(_) => {
                    eprintln!("Failed to launch {}.", task.tagged_image_name());
                }
            }

            if terminate_receiver.try_recv().is_ok() {
                if let Ok(mut child) = child_process_result {
                    let _ = child.kill();
                    let _ = status_sender.send(Status::Terminate);
                }
            }
        
        })
    }

    pub async fn kill(&self) {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };
        let component_name = self.spec.lock().unwrap().component_name.clone();
        let _ = run_command("kill".white().bold(), toolchain.docker(), vec!["kill", &component_name]).await;
    }


    pub async fn clean(&self) {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };
        let component_name = self.spec.lock().unwrap().component_name.clone();
        let args = vec!["rm", &component_name];

        let _ = run_command("clean".white().bold(), toolchain.docker(), args.into()).await;

        // TODO: Remove artefacts
    }

    pub async fn kill_and_clean(&self) {
        self.kill().await;
        self.clean().await;
    }

    pub async fn run(&self) -> Result<(), String> {
        self.build().await?;


        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };        

        let env_guard = DockerImage::create_cross_compile_guard(&self.spec.lock().unwrap().build_type, &toolchain);

        let formatted_label = format!("{} [{}]", 
            self.spec.lock().unwrap().component_name.to_string(),
            env_guard.target()
        );
        let formatted_label =formatted_label.white().bold();
        let extra_args = self.spec.lock().unwrap().docker_extra_run_args.clone();

        // TODO: Get ports
        let image_name = self.tagged_image_name();
        let mut args = vec!["run", "-p", "8000:80", &image_name];
        for arg in &extra_args {
            args.push(&arg);
        }

        match run_command(formatted_label, toolchain.docker(), args).await {
            Ok(_) => {
                Ok(())
            }
            Err(e) => {
                Err(e)
            }
        }
    }

    pub async fn push(&self) -> Result<(), String> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };
        
        let spec = self.spec.lock().unwrap().clone();
        // Nothing to do for components that does not have a k8s
        if spec.k8s.is_none() || spec.build_type == BuildType::PureKubernetes {
            return Ok(());
        }
        if let BuildType::KubernetesInstallation{..} = spec.build_type {
            return Ok(());
        }

        let tag = self.tagged_image_name();
        let docker_registry = self.config.docker_registry();
        let docker_tag = format!("{}/{}", docker_registry, tag);
        match run_command("tag".white().bold(), toolchain.docker(), vec!["tag", &tag, &docker_tag]).await {
            Ok(_) => (),
            Err(e) => {
                return Err(e)
            }
        }        
        
        match run_command("push".white().bold(), toolchain.docker(), vec!["push", &docker_tag]).await {
            Ok(_) => {
                Ok(())
            }
            Err(e) => {
                Err(e)
            }
        }
    }

    pub async fn build_and_push(&self) -> Result<(), String> {
        self.build().await?;
        self.push().await
    }

    pub async fn build(&self) -> Result<(), String> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };
        let spec = self.spec.lock().unwrap().clone();



        let dockerfile_path = match &spec.build_type {
            BuildType::TrunkWasm{ dockerfile_path, .. } => dockerfile_path.clone(),
            BuildType::RustBinary{ dockerfile_path,.. } => dockerfile_path.clone(),
            BuildType::Script{ dockerfile_path,.. } => dockerfile_path.clone(),
            BuildType::Ingress{ dockerfile_path, ..} => dockerfile_path.clone(),
            _ => return Ok(())
        };
        let context_dir = match &self.context_dir {
            Some(context_dir) => context_dir.clone(),
            None => ".".to_string(),
        };

        let _env_guard = DockerImage::create_cross_compile_guard(&self.spec.lock().unwrap().build_type, &toolchain);

        let dockerfile_path = std::path::Path::new(&dockerfile_path);
        let dockerfile_dir = dockerfile_path.parent().expect("Failed to get dockerfile directory");
        let dockerfile_name = dockerfile_path.file_name().expect("Failed to get dockerfile name").to_str().expect("Failed to convert dockerfile name to str");

        let ctx = self.generate_build_context();

        // Creating artefacts if needed
        let artefacts = spec.build_artefacts();
        if !artefacts.is_empty() {        
            let artefact_output_dir = Path::new(&spec.artefact_output_dir);
            std::fs::create_dir_all(&artefact_output_dir).expect("Failed to create artefact output directory");

            let _dir_raii = Directory::chpath(artefact_output_dir);
            for (_k,artefact) in artefacts {
                artefact.render_to_file(&ctx);
            }        
        }

        // Cross compiling if needed
        if let Some(build_command) = &self.build_script(&ctx) {
            match run_command_in_window(10, "build", "sh", vec!["-c", build_command]).await {
                Ok(_) => (),
                Err(e) => {
                    return Err(e);
                }
            }
        }


        let _dir_raii = Directory::chpath(dockerfile_dir);

        let tag = self.tagged_image_name();
        let build_command_args = vec!["build", "-t", &tag, "-f", dockerfile_name, &context_dir];
        match run_command_in_window(10, "docker",toolchain.docker(), build_command_args).await {
            Ok(_) => {
                Ok(())
            }
            Err(e) => {
                Err(e)
            }
        }
    }
    
}


