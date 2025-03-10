use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use tokio::sync::broadcast::Receiver as BroadcastReceiver;

use super::status::Status;
use crate::builder::BuildContext;
use crate::builder::BuildType;
use crate::builder::ComponentBuildSpec;
use crate::builder::Config;
use crate::utils::{handle_stream, run_command, run_command_in_window};
use crate::vault::Vault;
use crate::Directory;
use crate::{toolchain::ToolchainContext, utils::DockerCrossCompileGuard};
use colored::Colorize;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::process::Command;

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
    docker_file: Option<PathBuf>,

    depends_on: Vec<String>,
    context_dir: Option<String>,
    should_rebuild: bool,

    // Derived from Dockerfile
    exposes: Vec<String>,

    port: Option<u16>,
    target_port: Option<u16>,

    // Spec
    config: Arc<Config>,
    spec: Arc<Mutex<ComponentBuildSpec>>,
    toolchain: Option<Arc<ToolchainContext>>,
    vault: Option<Arc<Mutex<dyn Vault + Send>>>,
    network_name: Option<String>,

    dev_ignore_image: bool,
    silence_output: bool,
    was_recently_rebuild: bool,
}

impl DockerImage {
    pub fn was_recently_rebuild(&self) -> bool {
        self.was_recently_rebuild
    }

    pub fn set_was_recently_rebuild(&mut self, v: bool) {
        self.was_recently_rebuild = v;
    }

    pub fn depends_on(&self) -> &Vec<String> {
        &self.depends_on
    }

    pub fn set_silence_output(&mut self, silence_output: bool) {
        self.silence_output = silence_output;
    }

    pub fn should_ignore_in_devmode(&self) -> bool {
        self.dev_ignore_image
    }

    pub fn set_ignore_in_devmode(&mut self, ignore: bool) {
        self.dev_ignore_image = ignore;
    }

    pub fn image_name(&self) -> &str {
        &self.image_name
    }

    pub fn should_rebuild(&self) -> bool {
        self.should_rebuild
    }

    pub fn set_should_rebuild(&mut self, should_rebuild: bool) {
        self.should_rebuild = should_rebuild;
    }

    pub fn set_network_name(&mut self, network_name: String) {
        debug!("Setting network name to: {}", network_name);
        self.network_name = Some(network_name);
    }

    pub fn create_cross_compile_guard(
        build_type: &BuildType,
        toolchain: &ToolchainContext,
    ) -> DockerCrossCompileGuard {
        let target = match build_type {
            BuildType::PureDockerImage { .. } => toolchain.host(),
            _ => toolchain.target(),
        };

        debug!(
            "Creating cross compile guard for target: {}",
            target.to_docker_target()
        );
        DockerCrossCompileGuard::new(&target.to_docker_target())
    }

    pub fn from_docker_spec(spec: Arc<Mutex<ComponentBuildSpec>>) -> Result<Self, String> {
        let orig_spec = spec.clone();
        let spec = spec.lock().unwrap();
        let config = spec.config();
        let (dockerfile_path, context_dir) = match &spec.build_type {
            BuildType::TrunkWasm {
                dockerfile_path,
                context_dir,
                ..
            } => (Some(dockerfile_path.clone()), context_dir.clone()),
            BuildType::DixiousWasm {
                dockerfile_path,
                context_dir,
                ..
            } => (Some(dockerfile_path.clone()), context_dir.clone()),
            BuildType::RustBinary {
                dockerfile_path,
                context_dir,
                ..
            } => (Some(dockerfile_path.clone()), context_dir.clone()),
            BuildType::Book {
                dockerfile_path,
                context_dir,
                ..
            } => (Some(dockerfile_path.clone()), context_dir.clone()),
            BuildType::Zola {
                dockerfile_path,
                context_dir,
                ..
            } => (Some(dockerfile_path.clone()), context_dir.clone()),
            BuildType::Script {
                dockerfile_path,
                context_dir,
                ..
            } => (Some(dockerfile_path.clone()), context_dir.clone()),
            BuildType::Ingress {
                dockerfile_path,
                context_dir,
                ..
            } => (Some(dockerfile_path.clone()), context_dir.clone()),
            _ => (None, None),
        };

        let (port, target_port, exposes) = if let Some(dockerfile_path) = dockerfile_path {
            trace!("Reading Dockerfile: {}", dockerfile_path);
            let dockerfile_contents =
                std::fs::read_to_string(&dockerfile_path).unwrap_or_else(|_| {
                    panic!(
                        "{}",
                        format!("Failed to read Dockerfile: {}", dockerfile_path).to_string()
                    )
                });

            let exposes = dockerfile_contents
                .lines()
                .map(|line| line.trim())
                .filter(|line| line.starts_with("EXPOSE"))
                .map(|line| line.trim_start_matches("EXPOSE").trim().to_string())
                .collect::<Vec<_>>();

            let port = exposes.first().map(|port| port.parse::<u16>().unwrap());
            let target_port = port;
            debug!(
                "Parsed from Dockerfile - Port: {:?}, Target Port: {:?}, Exposes: {:?}",
                port, target_port, exposes
            );
            (port, target_port, exposes)
        } else {
            (None, None, Vec::new())
        };

        // Spec overrides auto deduced ports
        let port = if let Some(p) = spec.port {
            debug!("Overriding port with spec value: {}", p);
            Some(p)
        } else {
            port
        };

        let target_port = if let Some(p) = spec.target_port {
            debug!("Overriding target port with spec value: {}", p);
            Some(p)
        } else {
            target_port
        };

        let (image_name, tag) = match &spec.build_type {
            BuildType::PureDockerImage {
                image_name_with_tag,
                ..
            } => {
                let split = image_name_with_tag.split(':').collect::<Vec<&str>>();
                if split.len() > 2 {
                    panic!("Image name with tag should not contain more than one colon");
                } else if split.len() == 2 {
                    (
                        split.first().unwrap().to_string(),
                        Some(split.last().unwrap().to_string()),
                    )
                } else {
                    (split.first().unwrap().to_string(), None)
                }
            }
            _ => (
                format!("{}-{}", spec.product_name, spec.component_name),
                None,
            ),
        };

        let product_name = spec.product_name.clone();
        let depends_on = spec
            .depends_on
            .iter()
            .map(move |s| format!("{}-{}", product_name, s))
            .collect::<Vec<String>>();

        let docker_file = DockerImage::docker_path_from_spec(&spec);
        trace!(
            "Created DockerImage for {}-{}",
            spec.product_name,
            spec.component_name
        );
        Ok(DockerImage {
            image_name,
            repo: None, // Assuming repo is not part of ComponentBuildSpec and defaults to None
            depends_on,
            docker_file,
            context_dir,
            should_rebuild: true,
            tag,
            exposes,
            config,
            spec: orig_spec,
            port,
            target_port,
            toolchain: None,
            vault: None,
            network_name: None,
            dev_ignore_image: false,
            silence_output: false,
            was_recently_rebuild: false,
        })
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn target_port(&self) -> Option<u16> {
        self.target_port
    }

    pub fn set_port(&mut self, port: u16) {
        debug!("Setting port to: {}", port);
        self.port = Some(port);
    }

    pub fn set_tag(&mut self, tag: String) {
        debug!("Setting tag to: {}", tag);
        self.tag = Some(tag);
    }

    pub fn tagged_image_name(&self) -> String {
        let base_tag = self.tag.clone().expect("Image is not tagged");
        let tag = if let Some((_, s)) = self.get_context_path() {
            let path = s.display().to_string();
            let tag = match self
                .toolchain
                .clone()
                .expect("Toolchain not found")
                .get_git_wip(&path)
            {
                Ok(wip) => format!("{}{}", base_tag, wip),
                Err(_e) => base_tag,
            };
            tag
        } else {
            base_tag
        };

        format!("{}:{}", self.image_name, tag)
    }

    pub fn set_toolchain(&mut self, toolchain: Arc<ToolchainContext>) {
        debug!("Setting toolchain");
        self.toolchain = Some(toolchain);
    }

    pub fn set_vault(&mut self, vault: Arc<Mutex<dyn Vault + Send>>) {
        debug!("Setting vault");
        self.vault = Some(vault);
    }

    /*
    pub fn set_services(&mut self, services: Arc<ServicesSpec>) {
        self.spec.set_services(services);
    }
    */

    pub fn generate_build_context(&self, secrets: HashMap<String, String>) -> BuildContext {
        debug!("Generating build context");
        self.spec
            .lock()
            .unwrap()
            .generate_build_context(self.toolchain.clone(), secrets)
    }

    pub fn build_script(&self, ctx: &BuildContext) -> Option<String> {
        let ret = self.spec.lock().unwrap().build_script(ctx);

        if ret.is_empty() {
            debug!("No build script generated");
            None
        } else {
            debug!("Build script generated");
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
            None => match &self.spec.lock().unwrap().build_type {
                BuildType::PureDockerImage {
                    image_name_with_tag,
                    ..
                } => image_name_with_tag.clone(),
                _ => self.tagged_image_name(),
            },
        }
    }

    pub fn launch(
        &mut self,
        max_label_length: usize,
        mut terminate_receiver: BroadcastReceiver<()>,
        status_sender: Sender<Status>,
    ) -> tokio::task::JoinHandle<()> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };

        let _ = status_sender.send(Status::Awaiting);

        let task = self.clone();
        let network_name = self.network_name.clone().expect("Network name not set");

        let (command, entrypoint) = match &self.spec.lock().unwrap().build_type {
            BuildType::PureDockerImage {
                command,
                entrypoint,
                ..
            } => (command.clone(), entrypoint.clone()),
            _ => (None, None),
        };

        debug!("Launching docker image: {}", self.identifier());
        let silent = self.silence_output;
        tokio::spawn(async move {
            let spec = task.spec.lock().unwrap().clone();
            let env_guard = DockerImage::create_cross_compile_guard(&spec.build_type, &toolchain);

            let show_arch = false; // TODO: Make a config parameter
            let formatted_label = if show_arch {
                format!("{} [{}]", spec.component_name, env_guard.target())
            } else {
                spec.component_name.to_string()
            };

            //task.clean().await;
            let mut args = vec![
                "run".to_string(),
                "--name".to_string(),
                spec.docker_local_name(),
                "--network".to_string(),
                network_name,
            ];

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

            for (key, value) in &spec.dotenv {
                args.push("-e".to_string());
                args.push(format!("{}={}", key, value));
            }

            for (key, value) in &spec.dotenv_secrets {
                args.push("-e".to_string());
                args.push(format!("{}={}", key, value));
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

            debug!(
                "Running docker for {}: {}",
                spec.component_name,
                args.join(" ")
            );
            let mut child_process_result = Command::new(toolchain.docker())
                .args(args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            let _ = status_sender.send(Status::InProgress);
            match child_process_result {
                Err(_) => {
                    error!("Failed to launch {}.", task.tagged_image_name());
                    eprintln!("Failed to launch {}.", task.tagged_image_name());
                    // let _ = status_sender.send(Status::Failed);
                }
                Ok(ref mut child) => {
                    let (stdout, stderr) =
                        (child.stdout.take().unwrap(), child.stderr.take().unwrap());

                    let formatted_label =
                        format!("{:width$}", formatted_label, width = max_label_length)
                            .color(spec.color.as_str())
                            .bold();
                    let (tx, rx) = mpsc::channel();

                    let stdout_task = tokio::spawn(handle_stream(stdout, tx.clone()));
                    let stderr_task = tokio::spawn(handle_stream(stderr, tx));

                    let lines = Arc::new(Mutex::new(Vec::new()));
                    let lines_clone = lines.clone();
                    let formatted_label_clone = formatted_label.clone();

                    // TODO: Make startupcompleted depend on observed output
                    let _ = status_sender.send(Status::StartupCompleted);
                    tokio::spawn(async move {
                        loop {
                            match rx.try_recv() {
                                Ok(line) => {
                                    let mut lines = lines_clone.lock().unwrap();
                                    lines.push(line.trim_end().to_string());
                                    let clean_line = line.trim_end().replace(['\r', '\n'], "");
                                    if !silent {
                                        println!("{} |   {}", formatted_label_clone, clean_line);
                                        std::io::stdout().flush().unwrap();
                                    }
                                }
                                Err(mpsc::TryRecvError::Empty) => {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(10))
                                        .await;
                                }
                                Err(mpsc::TryRecvError::Disconnected) => {
                                    break;
                                }
                            }
                        }
                    });
                    println!("Waiting for process '{}' to finish", spec.component_name);
                    tokio::select! {
                        _ = futures::future::join_all(vec![stdout_task, stderr_task]) => {
                            println!(
                                "{} |   {}",
                                formatted_label,
                                "Exit reason: Process finished".bold().white()
                            );
                        }
                        _ = child.wait() => {
                            println!(
                                "{} |   {}",
                                formatted_label,
                                "Exit reason: Process finished".bold().white()
                            );
                        }
                        _ =  terminate_receiver.recv() => {
                            println!(
                                "{} |   {}",
                                formatted_label,
                                "Exit reason: Received terminate signal".bold().white()
                            );
                            let _ = status_sender.send(Status::Terminate);
                            debug!("Received termination signal for {}", spec.component_name);
                            // TODO: See you can find something more cross-platform friendly
                            let child_id = child.id().unwrap().to_string();
                            debug!("Attempting to kill process with ID: {}", child_id);
                            let mut kill = Command::new("kill")
                                .args(["-s", "TERM", &child_id])
                                .spawn()
                                .expect("Failed to kill process");
                            debug!("Waiting for kill command to complete");
                            kill.wait().await.unwrap();
                            //let _ = status_sender.send(Status::Terminate);
                            debug!("Kill command completed");
                            let _ = child.kill();
                            debug!("Sent termination status for {}", spec.component_name);
                        }
                    }

                    println!(
                        "{} |   {}",
                        formatted_label,
                        "Waiting for process to finish".bold().white()
                    );
                    if let Some(code) = child.wait().await.unwrap().code() {
                        let _ = status_sender.send(Status::Finished(code));
                        let message = format!("Process exited with code: {}", code);
                        println!("{} |   {}", formatted_label, message.bold().white());
                    } else {
                        eprintln!(
                            "{}",
                            format!("Terminating {}.", spec.component_name)
                                .bold()
                                .white()
                        );
                    }
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
        let local_container_name = self.spec.lock().unwrap().docker_local_name().clone();

        // Check if the container is running
        let component_arg = format!("name={}", local_container_name);
        let check_args = vec!["ps", "-q", "-f", &component_arg];
        match run_command("check".white().bold(), toolchain.docker(), check_args).await {
            Ok(output) => {
                let output = output.trim();
                if !output.is_empty() {
                    // Container is running, proceed with kill
                    let _ = run_command(
                        "kill".white().bold(),
                        toolchain.docker(),
                        vec!["kill", &output],
                    )
                    .await;
                    log::info!("Killed Docker container for {}", local_container_name);
                } else {
                    trace!(
                        "No running container found for {}. Skipping kill.",
                        local_container_name
                    );
                }
            }
            Err(e) => warn!(
                "Failed to check if container {} is running: {}",
                local_container_name, e
            ),
        }
    }

    pub async fn clean(&self) {
        debug!("Starting clean process for Docker image");
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => {
                error!("Cannot clean docker image without a toolchain");
                panic!("Cannot clean docker image without a toolchain");
            }
        };
        let local_image_name = self.spec.lock().unwrap().docker_local_name();
        info!(
            "Cleaning Docker container for component: {}",
            local_image_name
        );

        // Check if the container exists before attempting to remove it
        let component_arg = format!("name={}", local_image_name);
        let check_args = vec!["ps", "-a", "-q", "-f", &component_arg];
        match run_command("check".white().bold(), toolchain.docker(), check_args).await {
            Ok(output) => {
                if !output.trim().is_empty() {
                    // Container exists, proceed with removal
                    let remove_args = vec!["rm", "-f", &local_image_name];
                    match run_command("clean".white().bold(), toolchain.docker(), remove_args).await
                    {
                        Ok(_) => trace!(
                            "Successfully removed Docker container for {}",
                            local_image_name
                        ),
                        Err(e) => warn!(
                            "Failed to remove Docker container for {}: {}",
                            local_image_name, e
                        ),
                    }
                } else {
                    trace!(
                        "No container found for {}. Skipping removal.",
                        local_image_name
                    );
                }
            }
            Err(e) => warn!(
                "Failed to check for existing container {}: {}",
                local_image_name, e
            ),
        }

        // TODO: Remove artefacts
        debug!("Clean process completed for Docker image");
    }

    pub async fn kill_and_clean(&self) {
        self.kill().await;
        self.clean().await;
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
        if let BuildType::KubernetesInstallation { .. } = spec.build_type {
            return Ok(());
        }

        let tag = self.tagged_image_name();
        let docker_registry = self.config.docker_registry();
        let docker_tag = format!("{}/{}", docker_registry, tag);
        match run_command(
            "tag".white().bold(),
            toolchain.docker(),
            vec!["tag", &tag, &docker_tag],
        )
        .await
        {
            Ok(_) => (),
            Err(e) => return Err(e),
        }

        match run_command(
            "push".white().bold(),
            toolchain.docker(),
            vec!["push", &docker_tag],
        )
        .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn build_and_push(&self) -> Result<(), String> {
        self.build().await?;
        self.push().await
    }

    fn docker_path_from_spec(spec: &ComponentBuildSpec) -> Option<PathBuf> {
        match &spec.build_type {
            BuildType::TrunkWasm {
                dockerfile_path, ..
            }
            | BuildType::DixiousWasm {
                dockerfile_path, ..
            }
            | BuildType::RustBinary {
                dockerfile_path, ..
            }
            | BuildType::Book {
                dockerfile_path, ..
            }
            | BuildType::Zola {
                dockerfile_path, ..
            }
            | BuildType::Script {
                dockerfile_path, ..
            }
            | BuildType::Ingress {
                dockerfile_path, ..
            } => Some(
                std::fs::canonicalize(dockerfile_path).expect(
                    format!(
                        "Failed to get absolute dockerfile path for {:?}",
                        dockerfile_path
                    )
                    .as_str(),
                ),
            ),
            _ => None,
        }
    }

    fn get_context_path(&self) -> Option<(PathBuf, PathBuf)> {
        let dockerfile_path = match self.docker_file.clone() {
            Some(path) => path,
            None => return None,
        };

        let dockerfile_dir = dockerfile_path
            .parent()
            .expect("Failed to get dockerfile directory");

        let context_dir = match &self.context_dir {
            Some(context_dir) => std::fs::canonicalize(dockerfile_dir.join(context_dir))
                .expect("Failed to get absolute context directory path"),
            None => dockerfile_dir.to_path_buf(),
        };

        Some((dockerfile_dir.to_path_buf(), context_dir))
    }

    pub fn is_any_file_in_context(&self, file_paths: &Vec<PathBuf>) -> bool {
        let spec = self.spec.lock().unwrap();

        if let Some(watch) = &spec.watch {
            for file in file_paths {
                if watch.matches(file) {
                    return true;
                }
            }
        }

        let (dockerfile_dir, context_dir) = match self.get_context_path() {
            Some(paths) => paths,
            None => return false,
        };

        file_paths.iter().any(|file_path| {
            if let Ok(absolute_file_path) = std::fs::canonicalize(file_path) {
                absolute_file_path.starts_with(&context_dir)
                    || absolute_file_path.starts_with(&dockerfile_dir)
            } else {
                false
            }
        })
    }

    pub async fn build(&self) -> Result<(), String> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain.clone(),
            None => panic!("Cannot launch docker image without a toolchain"),
        };

        let tag = self.tagged_image_name();

        // Check if image exists
        let image_exists = match Command::new(toolchain.docker())
            .args(["image", "inspect", &tag])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
        {
            Ok(status) => status.success(),
            Err(_) => false,
        };

        if !image_exists {
            let spec = self.spec.lock().unwrap().clone();

            let dockerfile_path = match &spec.build_type {
                BuildType::TrunkWasm {
                    dockerfile_path, ..
                } => dockerfile_path.clone(),
                BuildType::DixiousWasm {
                    dockerfile_path, ..
                } => dockerfile_path.clone(),
                BuildType::RustBinary {
                    dockerfile_path, ..
                } => dockerfile_path.clone(),
                BuildType::Zola {
                    dockerfile_path, ..
                } => dockerfile_path.clone(),
                BuildType::Book {
                    dockerfile_path, ..
                } => dockerfile_path.clone(),
                BuildType::Script {
                    dockerfile_path, ..
                } => dockerfile_path.clone(),
                BuildType::Ingress {
                    dockerfile_path, ..
                } => dockerfile_path.clone(),
                _ => return Ok(()),
            };
            let context_dir = match &self.context_dir {
                Some(context_dir) => context_dir.clone(),
                None => ".".to_string(),
            };

            let _env_guard = DockerImage::create_cross_compile_guard(
                &self.spec.lock().unwrap().build_type,
                &toolchain,
            );

            let dockerfile_path = std::path::Path::new(&dockerfile_path);
            let dockerfile_dir = dockerfile_path
                .parent()
                .expect("Failed to get dockerfile directory");
            let dockerfile_name = dockerfile_path
                .file_name()
                .expect("Failed to get dockerfile name")
                .to_str()
                .expect("Failed to convert dockerfile name to str");

            let secrets = self
                .vault
                .as_ref()
                .expect("Vault not set")
                .lock()
                .unwrap()
                .get(
                    &spec.product_name,
                    &spec.component_name,
                    &spec.config.environment().to_string(),
                )
                .await
                .unwrap_or_default();
            let ctx = self.generate_build_context(secrets);

            // Creating artefacts if needed
            let artefacts = spec.build_artefacts();
            if !artefacts.is_empty() {
                let artefact_output_dir = Path::new(&spec.artefact_output_dir);
                std::fs::create_dir_all(artefact_output_dir)
                    .expect("Failed to create artefact output directory");

                let _dir_raii = Directory::chpath(artefact_output_dir);
                for (_k, artefact) in artefacts {
                    artefact.render_to_file(&ctx);
                }
            }

            // Cross compiling if needed
            if let Some(build_command) = &self.build_script(&ctx) {
                let start_time = std::time::Instant::now();
                match run_command_in_window(10, "build", "sh", vec!["-c", build_command]).await {
                    Ok(_) => {
                        let duration = start_time.elapsed();
                        info!("Build command completed in {:?}", duration);
                    }
                    Err(e) => {
                        let duration = start_time.elapsed();
                        debug!("Build command failed after {:?}", duration);
                        return Err(e);
                    }
                }
            }

            let _dir_raii = Directory::chpath(dockerfile_dir);

            let build_command_args = vec!["build", "-t", &tag, "-f", dockerfile_name, &context_dir];
            match run_command_in_window(10, "docker", toolchain.docker(), build_command_args).await
            {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            }
        } else {
            debug!("Image {} already exists, skipping build", tag);
            Ok(())
        }
    }
}
