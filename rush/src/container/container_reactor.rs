use super::docker::DockerImage;
use super::status::Status;
use crate::builder::BuildType;
use crate::builder::ComponentBuildSpec;
use crate::builder::Config;
use crate::builder::Variables;
use crate::cluster::InfrastructureRepo;
use crate::cluster::K8ClusterManifests;
use crate::cluster::K8Encoder;
use crate::container::service_spec::{ServiceSpec, ServicesSpec};
use crate::gitignore::GitIgnore;
use crate::toolchain::ToolchainContext;
use crate::utils::run_command;
use crate::utils::Directory;
use crate::vault::EncodeSecrets;
use crate::vault::Vault;
use colored::Colorize;
use glob::glob;
use log::{debug, error, trace, warn};
use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc::{self, Receiver},
};
use tokio::sync::broadcast;
use tokio::sync::broadcast::{Receiver as BroadcastReceiver, Sender as BroadcastSender};

// TODO: This ought to split into a spec and a reactor
pub struct ContainerReactor {
    config: Arc<Config>,
    product_directory: String,
    images: Vec<DockerImage>,
    handles: HashMap<usize, tokio::task::JoinHandle<()>>,
    images_by_id: HashMap<usize, DockerImage>,
    statuses_receivers: HashMap<usize, Receiver<Status>>,
    statuses: HashMap<String, Status>,
    terminate_sender: BroadcastSender<()>,
    terminate_receiver: BroadcastReceiver<()>,
    toolchain: Option<Arc<ToolchainContext>>,
    services: Arc<ServicesSpec>,

    secrets_encoder: Arc<dyn EncodeSecrets>,
    cluster_manifests: K8ClusterManifests,
    infrastructure_repo: InfrastructureRepo,
    vault: Arc<Mutex<dyn Vault + Send>>,
}

impl ContainerReactor {
    async fn delete_network(&self) -> Result<(), String> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain,
            None => return Err("Toolchain not found".to_string()),
        };

        let network_name = self.config.network_name();

        // Check if the network exists
        let check_args = vec!["network", "inspect", network_name];
        match run_command("check".white().bold(), toolchain.docker(), check_args).await {
            Ok(_) => {
                // Network exists, proceed with removal
                if let Err(e) = run_command(
                    "docker".into(),
                    toolchain.docker(),
                    vec!["network", "rm", network_name],
                )
                .await
                {
                    return Err(format!("Failed to delete Docker network: {}", e));
                }
                trace!("Successfully deleted Docker network: {}", network_name);
            }
            Err(_) => {
                // Network doesn't exist
                trace!(
                    "Docker network '{}' does not exist. Skipping deletion.",
                    network_name
                );
            }
        }
        Ok(())
    }

    async fn create_network(&self) -> Result<(), String> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain,
            None => return Err("Toolchain not found".to_string()),
        };

        let network_name = self.config.network_name();

        // Check if the network exists
        let check_args = vec!["network", "inspect", network_name];
        match crate::utils::run_command("check".white().bold(), toolchain.docker(), check_args)
            .await
        {
            Ok(_) => {
                // Network already exists
                trace!(
                    "Docker network '{}' already exists. Skipping creation.",
                    network_name
                );
                Ok(())
            }
            Err(_) => {
                // Network doesn't exist, create it
                match crate::utils::run_command(
                    "docker".into(),
                    toolchain.docker(),
                    vec!["network", "create", "-d", "bridge", network_name],
                )
                .await
                {
                    Ok(_) => {
                        trace!("Successfully created Docker network: {}", network_name);
                        Ok(())
                    }
                    Err(e) => Err(format!("Failed to create Docker network: {}", e)),
                }
            }
        }
    }

    pub fn services(&self) -> &HashMap<String, ServiceSpec> {
        &self.services
    }

    pub fn product_directory(&self) -> &str {
        &self.product_directory
    }

    pub fn images(&self) -> &Vec<DockerImage> {
        &self.images
    }

    pub fn cluster_manifests(&self) -> &K8ClusterManifests {
        &self.cluster_manifests
    }

    pub fn get_image(&self, component_name: &str) -> Option<&DockerImage> {
        self.images
            .iter()
            .find(|image| image.component_name() == component_name)
    }

    pub fn from_product_dir(
        config: Arc<Config>,
        toolchain: Arc<ToolchainContext>,
        vault: Arc<Mutex<dyn Vault + Send>>,
        secrets_encoder: Arc<dyn EncodeSecrets>,
        k8s_encoder: Arc<dyn K8Encoder>,
    ) -> Result<Self, String> {
        let git_hash = match toolchain.get_git_folder_hash(config.product_path()) {
            Ok(hash) => hash,
            Err(e) => {
                return Err(e);
            }
        };

        let binding = config.clone();
        let product_path = binding.product_path();
        let product_name = binding.product_name(); // product_path.split('/').last().unwrap_or(product_path).to_string();
        let network_name = binding.network_name();

        // TODO: Move to config
        if git_hash.is_empty() {
            return Err("No git hash found for {}".to_string());
        }

        let tag = git_hash[..8].to_string();
        let tag = match toolchain.get_git_wip(product_path) {
            Ok(wip) => format!("{}{}", tag, wip),
            Err(_e) => tag,
        };

        let _guard = Directory::chdir(product_path);

        let variables = Variables::new("variables.yaml", config.environment());

        let stack_config = match std::fs::read_to_string("stack.spec.yaml") {
            Ok(config) => config,
            Err(e) => return Err(format!("Failed to read stack config: {}", e)),
        };

        let mut next_port = 8000;
        let stack_config_value: serde_yaml::Value = serde_yaml::from_str(&stack_config).unwrap();
        let mut images = Vec::new();

        let mut cluster_manifests = {
            let product_directory = std::path::Path::new("./target"); // TODO: Hardcoded
            K8ClusterManifests::new(
                product_directory.join("k8s"),
                Some(toolchain.clone()),
                k8s_encoder,
            )
        };

        let mut all_component_specs = Vec::new();

        if let serde_yaml::Value::Mapping(config_map) = stack_config_value {
            for (component_name, yaml_section) in config_map {
                let mut yaml_section_clone = yaml_section.clone();

                if let serde_yaml::Value::Mapping(ref mut yaml_section_map) = yaml_section_clone {
                    if !yaml_section_map
                        .contains_key(&serde_yaml::Value::String("component_name".to_string()))
                    {
                        yaml_section_map.insert(
                            serde_yaml::Value::String("component_name".to_string()),
                            serde_yaml::Value::String(component_name.as_str().unwrap().to_string()),
                        );
                    }
                }

                let component_spec = Arc::new(Mutex::new(ComponentBuildSpec::from_yaml(
                    config.clone(),
                    variables.clone(),
                    &yaml_section_clone,
                )));

                let build_type = {
                    let (k8s, priority, build_type) = {
                        let spec = component_spec.lock().unwrap();
                        (spec.k8s.clone(), spec.priority, spec.build_type.clone())
                    };
                    match k8s {
                        Some(ref path) => {
                            let k8spath = std::path::Path::new(path).into();
                            let component_name: String = match component_name.as_str() {
                                Some(name) => name.to_string(),
                                None => {
                                    return Err(
                                        "Could not convert component name to string".to_string()
                                    )
                                }
                            };
                            let component_name = format!("{}_{}", priority, component_name);
                            cluster_manifests.add_component(
                                &component_name,
                                component_spec.clone(),
                                k8spath,
                            );
                        }
                        _ => (),
                    };

                    build_type
                };

                let mut image: DockerImage = component_spec.clone().try_into()?;
                match build_type {
                    BuildType::PureDockerImage { .. } => (),
                    _ => {
                        image.set_tag(tag.clone());

                        // We only set the port if it is not specified in the spec
                        if image.spec().port.is_none() {
                            image.set_port(next_port);
                            next_port += 1;
                        }
                    }
                }
                image.set_toolchain(toolchain.clone());
                image.set_vault(vault.clone());
                image.set_network_name(network_name.to_string());
                component_spec
                    .lock()
                    .unwrap()
                    .set_tagged_image_name(image.tagged_image_name());
                images.push(image);

                all_component_specs.push(component_spec);
            }
        }

        let mut services = HashMap::new();
        for image in &images {
            if let Some(port) = image.port() {
                if let Some(target_port) = image.target_port() {
                    let svc_spec = ServiceSpec {
                        name: image.component_name(),
                        port,
                        target_port,
                        mount_point: image.spec().mount_point.clone(),
                    };
                    services.insert(image.component_name(), svc_spec);
                }
            }
        }
        let services = Arc::new(services);

        for component_spec in &mut all_component_specs {
            component_spec
                .lock()
                .unwrap()
                .set_services(services.clone());
        }

        let (terminate_sender, terminate_receiver) = broadcast::channel(16);

        let infrastructure_repo = InfrastructureRepo::new(config.clone(), toolchain.clone());

        Ok(ContainerReactor {
            config,
            // product_name: product_name.to_string(),
            product_directory: product_path.to_string(),
            images,
            images_by_id: HashMap::new(),
            statuses_receivers: HashMap::new(),
            statuses: HashMap::new(),
            handles: HashMap::new(),
            terminate_sender,
            terminate_receiver,
            toolchain: Some(toolchain),
            services,
            secrets_encoder,
            cluster_manifests,
            infrastructure_repo,
            vault,
        })
        //        Ok(Self::new(&product_name, &product_path, images, toolchain))
    }

    pub async fn build_and_push(&mut self) -> Result<(), String> {
        let _guard = Directory::chdir(&self.product_directory);

        for image in &mut self.images {
            print!("Build & push {}  ..... ", image.identifier());
            std::io::stdout().flush().expect("Failed to flush stdout");
            match image.build_and_push().await {
                Ok(_) => println!(
                    "Build & push {}  ..... [  {}  ]",
                    image.identifier(),
                    "OK".white().bold()
                ),
                Err(e) => {
                    println!(
                        "Build & push {}  ..... [ {} ]",
                        image.identifier(),
                        "FAIL".red().bold()
                    );
                    println!();
                    println!("{}", e);
                    println!();
                    println!("{}", "Build was unsuccessful".red().bold());
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub async fn select_kubernetes_context(&self, context: &str) -> Result<(), String> {
        let toolchain = match &self.toolchain {
            Some(toolchain) => toolchain,
            None => return Err("Toolchain not found".to_string()),
        };

        let kubectx = toolchain.kubectx();

        match run_command(
            "Selecting Kubernetes context".white().bold(),
            kubectx,
            vec![context],
        )
        .await
        {
            Ok(_) => {
                println!("Kubernetes context set to: {}", context);
                Ok(())
            }
            Err(e) => {
                eprintln!("Failed to set Kubernetes context: {}", e);
                Err(e.to_string())
            }
        }
    }

    pub async fn apply(&mut self) -> Result<(), String> {
        let toolchain = match self.toolchain.clone() {
            Some(toolchain) => toolchain,
            None => return Err("Toolchain not found".to_string()),
        };

        let _guard = Directory::chdir(&self.product_directory);

        let kubectl = toolchain.kubectl();
        let output_dir = self
            .cluster_manifests
            .output_directory()
            .display()
            .to_string();
        let output_dir = if output_dir.ends_with('/') {
            &output_dir[..output_dir.len() - 1]
        } else {
            &output_dir
        };

        match run_command(
            "apply".white().bold(),
            kubectl,
            vec!["apply", "-R", "-f", &output_dir],
        )
        .await
        {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Failed to apply manifests: {}", e);
                return Err(e.to_string());
            }
        }

        Ok(())
    }

    pub async fn unapply(&mut self) -> Result<(), String> {
        let toolchain = match self.toolchain.clone() {
            Some(toolchain) => toolchain,
            None => return Err("Toolchain not found".to_string()),
        };
        let _guard = Directory::chdir(&self.product_directory);

        let kubectl = toolchain.kubectl();
        let output_dir = self
            .cluster_manifests
            .output_directory()
            .display()
            .to_string();
        let output_dir = if output_dir.ends_with('/') {
            &output_dir[..output_dir.len() - 1]
        } else {
            &output_dir
        };

        let mut args = glob(&format!("{}/**/*.yaml", output_dir))
            .expect("Failed to read glob pattern")
            .filter_map(|e| match e {
                Ok(e) => {
                    if e.extension().and_then(std::ffi::OsStr::to_str) == Some("yaml") {
                        Some(e.display().to_string())
                    } else {
                        None
                    }
                }
                Err(_) => None,
            })
            .collect::<Vec<_>>();
        args.sort();
        args.reverse();

        for arg in &args {
            match run_command(
                "delete".white().bold(),
                kubectl,
                vec!["delete", "-f", &**arg],
            )
            .await
            {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Failed to apply manifests: {}", e);
                    // Keep going to delete all possible resources
                    // return Err(e.to_string());
                }
            }
        }

        Ok(())
    }

    pub async fn rollout(&mut self) -> Result<(), String> {
        self.build_and_push().await?;
        self.build_manifests().await?;

        let _guard = Directory::chdir(&self.product_directory);
        self.infrastructure_repo.checkout().await?;

        let source_directory = self.cluster_manifests.output_directory();
        self.infrastructure_repo
            .copy_manifests(source_directory)
            .await?;

        self.infrastructure_repo
            .commit_and_push(&format!(
                "Deploying {} for {}",
                self.config.environment(),
                self.config.product_name()
            ))
            .await?;

        Ok(())
    }

    pub async fn deploy(&mut self) -> Result<(), String> {
        self.build_and_push().await?;
        self.build_manifests().await?;
        self.apply().await?;

        Ok(())
    }

    pub async fn install_manifests(&mut self) -> Result<(), String> {
        let toolchain = match self.toolchain.clone() {
            Some(toolchain) => toolchain,
            None => return Err("Toolchain not found".to_string()),
        };
        let _guard = Directory::chdir(&self.product_directory);

        let kubectl = toolchain.kubectl();
        for component in self.cluster_manifests.components() {
            if !component.is_installation() {
                continue;
            }

            let name = component.name();
            let namespace = component.namespace();
            print!("Installing {} in {}  ..... ", name, namespace);

            match run_command(
                "install".white().bold(),
                kubectl,
                vec!["create", "namespace", namespace],
            )
            .await
            {
                Ok(_) => (),
                Err(e) => {
                    // eprintln!("Failed to create namespace: {}", e);
                    // This may just be due to a reinstall or because the it is the default namespace
                    //return Err(e.to_string());
                }
            }

            for manifest in component.manifests() {
                match run_command(
                    "install".white().bold(),
                    kubectl,
                    vec![
                        "apply",
                        "-n",
                        namespace,
                        "-f",
                        &manifest.artefact.input_path,
                    ],
                )
                .await
                {
                    Ok(_) => (),
                    Err(e) => {
                        eprintln!("Failed to installing manifests: {}", e);
                        return Err(e.to_string());
                    }
                }
            }

            println!(
                "\rInstalling {} in {}  ..... [  {}  ]",
                name,
                namespace,
                "OK".white().bold()
            );
        }

        Ok(())
    }

    pub async fn uninstall_manifests(&mut self) -> Result<(), String> {
        let toolchain = match self.toolchain.clone() {
            Some(toolchain) => toolchain,
            None => return Err("Toolchain not found".to_string()),
        };
        let _guard = Directory::chdir(&self.product_directory);

        let kubectl = toolchain.kubectl();
        for component in self.cluster_manifests.components().iter().rev() {
            if !component.is_installation() {
                continue;
            }

            let name = component.name();
            let namespace = component.namespace();

            print!("Uninstalling {} in {}  ..... ", name, namespace);

            for manifest in component.manifests() {
                match run_command(
                    "uninstall".white().bold(),
                    kubectl,
                    vec![
                        "delete",
                        "-n",
                        namespace,
                        "-f",
                        &manifest.artefact.input_path,
                    ],
                )
                .await
                {
                    Ok(_) => (),
                    Err(e) => {
                        eprintln!("Failed to uninstalling manifests: {}", e);
                    }
                }
            }

            match run_command(
                "uninstall".white().bold(),
                kubectl,
                vec!["delete", "namespace", namespace],
            )
            .await
            {
                Ok(_) => (),
                Err(e) => {
                    eprintln!("Failed to delete namespace: {}", e);
                }
            }

            println!(
                "\rUninstalling {} in {}  ..... [  {}  ]",
                name,
                namespace,
                "OK".white().bold()
            );
        }

        Ok(())
    }

    pub async fn build_manifests(&mut self) -> Result<(), String> {
        let _guard = Directory::chdir(&self.product_directory);
        let output_dir = self.cluster_manifests.output_directory();
        if output_dir.exists() {
            std::fs::remove_dir_all(output_dir).expect("Failed to delete output directory");
        }

        for component in self.cluster_manifests.components() {
            if component.is_installation() {
                continue;
            }

            let render_dir = component.output_directory();
            std::fs::create_dir_all(render_dir).expect("Failed to create render directory");
            print!("Creating K8s {}  ..... ", render_dir.display());
            let current_dir = std::env::current_dir().unwrap();
            let spec = component.spec();

            let secrets = {
                let vault = self.vault.lock().unwrap();
                vault
                    .get(
                        &spec.product_name,
                        &spec.component_name,
                        &spec.config.environment().to_string(),
                    )
                    .await
                    .unwrap_or_default()
            };
            // Encoding secrets
            let secrets = self.secrets_encoder.encode_secrets(secrets);

            let ctx = spec.generate_build_context(self.toolchain.clone(), secrets);
            for manifest in component.manifests() {
                manifest.render_to_file(&ctx);
            }

            println!(
                "\rCreating K8s {}  ..... [  {}  ]",
                render_dir.display(),
                "OK".white().bold()
            );
        }

        Ok(())
    }

    pub async fn build(&mut self) -> Result<(), String> {
        {
            let _guard = Directory::chdir(&self.product_directory);

            for image in &mut self.images {
                print!("Building {}  ..... ", image.identifier());
                std::io::stdout().flush().expect("Failed to flush stdout");
                match image.build().await {
                    Ok(_) => println!(
                        "Building {}  ..... [  {}  ]",
                        image.identifier(),
                        "OK".white().bold()
                    ),
                    Err(e) => {
                        println!(
                            "Building {}  ..... [ {} ]",
                            image.identifier(),
                            "FAIL".red().bold()
                        );
                        println!();
                        println!("{}", e);
                        println!();
                        println!("{}", "Build was unsuccessful".red().bold());
                        return Err(e);
                    }
                }
            }
        }

        self.build_manifests().await?;

        Ok(())
    }

    pub async fn launch(&mut self) -> Result<(), String> {
        trace!("Starting launch process");

        let _ = self.create_network().await;
        trace!("Created Docker network");

        let mut running = true;
        let (watch_tx, watch_rx) = std::sync::mpsc::channel();
        // Automatically select the best implementation for your platform.
        // You can also access each implementation directly e.g. INotifyWatcher.
        let mut watcher = match RecommendedWatcher::new(watch_tx, NotifyConfig::default()) {
            Ok(w) => {
                trace!("Created file watcher");
                w
            }
            Err(e) => {
                error!("Failed to create file watcher: {}", e);
                return Err(e.to_string());
            }
        };

        // Add a path to be watched. All files and directories at that path and
        // below will be monitored for changes.
        let path = self.product_directory.clone();
        match watcher.watch(path.as_ref(), RecursiveMode::Recursive) {
            Ok(_) => trace!("Started watching directory: {}", path),
            Err(e) => {
                error!("Failed to watch directory: {}", e);
                return Err(e.to_string());
            }
        }

        let product_directory = std::path::Path::new(&self.product_directory);
        let gitignore = GitIgnore::new(product_directory);
        let test_if_files_changed = move || {
            if let Ok(event) = watch_rx.try_recv() {
                match event {
                    Ok(event) => {
                        let other_events = watch_rx.try_iter();

                        let all_events = std::iter::once(Ok(event)).chain(other_events);
                        let paths = all_events
                            .filter_map(|event| {
                                if let Ok(event) = event {
                                    if event.paths.is_empty() {
                                        None
                                    } else {
                                        Some(event.paths)
                                    }
                                } else {
                                    None
                                }
                            })
                            .flatten()
                            .filter(|path| !gitignore.ignores(path))
                            .collect::<Vec<_>>();

                        let paths = paths
                            .into_iter()
                            .filter(|path| path.is_file())
                            .collect::<Vec<_>>();

                        let mut unique_paths = std::collections::HashSet::new();
                        let paths = paths
                            .into_iter()
                            .filter(|path| unique_paths.insert(path.clone()))
                            .collect::<Vec<_>>();

                        if !paths.is_empty() {
                            for p in paths.iter() {
                                trace!("File changed: {}", p.display());
                            }
                            debug!("Detected file changes: {:#?}", paths);
                            return true;
                        }
                    }
                    Err(e) => {
                        error!("Watch error: {:?}", e);
                    }
                }
            }
            false
        };

        while running {
            self.kill_and_clean().await;
            trace!("Cleaned up previous resources");

            match self.build().await {
                Ok(_) => trace!("Build completed successfully"),
                Err(e) => {
                    let e = e
                        .replace("error:", &format!("{}:", &"error".red().bold().to_string()))
                        .replace("error[", &format!("{}[", &"error".red().bold().to_string()))
                        .replace(
                            "warning:",
                            &format!("{}:", &"warning".yellow().bold().to_string()),
                        );
                    error!("Build failed: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                    // Waiting for error to get fixed
                    let ctrl_c = tokio::signal::ctrl_c();
                    tokio::pin!(ctrl_c);

                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                    loop {
                        if test_if_files_changed() {
                            trace!("File change detected. Rebuilding all images.");
                            let _ = self.terminate_sender.send(());
                            break;
                        }

                        tokio::select! {
                            _ = &mut ctrl_c => {
                                trace!("Termination signal received. Sending SIGTERM to all subprocesses.");
                                let _ = self.terminate_sender.send(());
                                running = false;
                                break;
                            }
                            _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                                // Status update loop
                            }
                        }
                    }

                    continue;
                }
            }

            let max_label_length = self
                .images
                .iter()
                .map(|image| image.component_name().len())
                .max()
                .unwrap_or_default();
            self.images_by_id = HashMap::new();
            self.statuses_receivers = HashMap::new();
            self.statuses = HashMap::new();
            self.handles = HashMap::new();

            let dependency_graph = self
                .images
                .iter()
                .map(|image| (image.image_name().to_string(), image.depends_on().clone()))
                .collect::<HashMap<String, Vec<String>>>();

            // Computing the deploy priority
            // TODO: Suboptimal algorithm - can be improved
            let mut longest_paths = HashMap::new();
            for (name, _) in &dependency_graph {
                let mut stack = vec![(name, 1)]; // (current node, current path length)
                let mut visited = HashSet::new();
                let mut max_length = 1;

                while let Some((current, path_len)) = stack.pop() {
                    visited.insert(current);
                    max_length = max_length.max(path_len);

                    if let Some(deps) = dependency_graph.get(current) {
                        for dep in deps {
                            if !visited.contains(dep) {
                                stack.push((dep, path_len + 1));
                            }
                        }
                    }
                }

                longest_paths.insert(name.clone(), max_length);
            }

            let mut jobs = self
                .images
                .iter_mut()
                .enumerate()
                .map(move |(id, image)| {
                    let priority = longest_paths
                        .get(image.image_name())
                        .cloned()
                        .unwrap_or_default();

                    (priority, id, image)
                })
                .collect::<Vec<_>>();
            jobs.sort_by(|a, b| a.0.cmp(&b.0)); // Sort jobs by priority in descending order

            for (priority, image_id, image) in jobs {
                trace!("Starting {} with priority {}", image.image_name(), priority);
                let (status_sender, status_receiver) = mpsc::channel();
                self.images_by_id.insert(image_id, image.clone());
                self.statuses_receivers.insert(image_id, status_receiver);
                self.statuses
                    .insert(image.component_name(), Status::Awaiting);
                let handle = image.launch(
                    max_label_length,
                    self.terminate_receiver.resubscribe(),
                    status_sender,
                );
                self.handles.insert(image_id, handle);

                // TODO: Hack instead of waiting for the image to declare ready
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }

            let mut all_finished = false;
            let mut stopping = false;
            let break_calls = 0;
            let mut stop_time: Option<std::time::Instant> = None;

            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);

            loop {
                tokio::select! {
                    _ = &mut ctrl_c => {
                        trace!("Termination signal received. Sending SIGTERM to all subprocesses.");
                        println!("******************************************************************");
                        println!("******************************************************************");
                        println!("*****************       GRACEFUL SHUTDOWN        *****************");
                        println!("******************************************************************");
                        println!("******************************************************************");


                        let _ = self.terminate_sender.send(());
                        self.update_image_statuses();

                        stop_time = Some(std::time::Instant::now());
                        running = false;
                        stopping = true;
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                        if !stopping && test_if_files_changed() {
                            trace!("File change detected. Rebuilding all images.");
                            let _ = self.terminate_sender.send(());
                            stop_time = Some(std::time::Instant::now());
                            stopping = true;
                        }
                        self.update_image_statuses();

                        all_finished = self.statuses.values().all(|status| matches!(status, Status::Finished(_)));
                        if all_finished || stopping {
                            break;
                        }

                        let any_finished = self.statuses.values().any(|status| matches!(status, Status::Finished(_)));
                        if any_finished {
                            running = false;
                            stopping = true;
                            warn!("Proceeding with forced shutdown due to image completion...");
                            self.kill_and_clean().await;
                            break;
                        }
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(1000));
            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);
            while running && !all_finished {
                tokio::select! {
                    _ = &mut ctrl_c => {
                        trace!("Termination signal received. Sending SIGTERM to all subprocesses.");
                        let _ = self.terminate_sender.send(());
                        self.update_image_statuses();
                        println!("******************************************************************");
                        println!("******************************************************************");
                        println!("*****************       FORCEFUL SHUTDOWN        *****************");
                        println!("******************************************************************");
                        println!("******************************************************************");

                        warn!("Proceeding with forced shutdown...");
                        self.kill_and_clean().await;

                        tokio::time::sleep(tokio::time::Duration::from_millis(2000));
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                        if !stopping && test_if_files_changed() {
                            trace!("File change detected. Rebuilding all images.");
                            let _ = self.terminate_sender.send(());
                            stop_time = Some(std::time::Instant::now());
                            stopping = true;
                        }
                        self.update_image_statuses();

                        all_finished = self.statuses.values().all(|status| matches!(status, Status::Finished(_)));

                        if all_finished {
                            break;
                        }


                        if stopping {
                            if let Some(stop_time) = stop_time {
                                if stop_time.elapsed() >= std::time::Duration::from_secs(5) {
                                    error!("Shutdown timeout reached. You might have a process that does not respond to SIGTERM.");
                                    println!("Current process statuses:");
                                    for (component_name, status) in &self.statuses {
                                        let status_str = match status {
                                            Status::Awaiting => "Awaiting".yellow(),
                                            Status::InProgress => "In Progress".blue(),
                                            Status::StartupCompleted => "Startup Completed".green(),
                                            Status::Reinitializing => "Reinitializing".cyan(),
                                            Status::Finished(code) => format!("Finished ({})", code).white(),
                                            Status::Terminate => "Terminating".red(),
                                        };
                                        println!("  {}: {}", component_name, status_str);
                                    }
                                    println!("Proceeding with forced shutdown...");
                                    self.kill_and_clean().await;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            trace!("Joining all handles");
            // Wait for all images to complete concurrently
            loop {
                tokio::select! {
                    _ = futures::future::join_all(self.handles.values_mut()) => {
                        trace!("All handles joined successfully");
                        break;
                    },
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                        warn!("Waiting for processes to quit ...");
                        let _ = self.terminate_sender.send(());
                        self.kill_all_images().await;
                    },
                }
            }
            self.handles.clear();
        }

        let _ = self.delete_network().await;
        trace!("Deleted Docker network");

        trace!("Launch process completed");
        Ok(())
    }

    async fn kill_all_images(&mut self) {
        for image in &mut self.images {
            image.kill().await;
        }
    }

    fn update_image_statuses(&mut self) {
        for (id, receiver) in self.statuses_receivers.iter_mut() {
            while let Ok(status) = receiver.try_recv() {
                // Update self.statuses with the new status
                if let Some(image) = self.images_by_id.get(id) {
                    let component_name = image.component_name();
                    let previous_status = self.statuses.get(&component_name);

                    if previous_status.map_or(true, |prev| *prev != status) {
                        self.statuses
                            .insert(component_name.to_string(), status.clone());

                        match status {
                            Status::InProgress => println!("Image {} is running", id),
                            Status::StartupCompleted => println!("Image {} is ready", id),
                            Status::Finished(code) => {
                                println!(
                                    "Image {} ({}) exited with code {}",
                                    id, component_name, code
                                )
                            }
                            _ => (),
                        }
                    }
                }
            }
        }
    }

    pub async fn kill_and_clean(&self) {
        trace!("Starting kill and cleanup process");
        for image in &self.images {
            debug!("Cleaning up image: {}", image.identifier());
            image.kill_and_clean().await;
        }
        trace!("Kill and cleanup process completed");
    }

    pub async fn clean(&self) {
        trace!("Starting cleanup process");
        for image in &self.images {
            debug!("Cleaning up image: {}", image.identifier());
            image.clean().await;
        }
        trace!("Cleanup process completed");
    }
}
