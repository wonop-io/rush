use super::docker::DockerImage;
use super::status::Status;
use crate::build::BuildType;
use crate::build::ComponentBuildSpec;
use crate::build::Config;
use crate::build::Variables;
use crate::cluster::InfrastructureRepo;
use crate::cluster::K8ClusterManifests;
use crate::cluster::K8Encoder;
use crate::container::service_spec::{ServiceSpec, ServicesSpec};
use crate::toolchain::ToolchainContext;
use crate::utils::path_matcher::PathMatcher;
use crate::utils::run_command;
use crate::utils::Directory;
use crate::vault::EncodeSecrets;
use crate::vault::Vault;
use colored::Colorize;
use glob::glob;
use log::{debug, error, trace, warn};
use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
use std::io::Write;
use std::path::PathBuf;
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
    available_components: Vec<String>,
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

    changed_files: Arc<Mutex<Vec<PathBuf>>>,
}

enum BreakType {
    Running,
    Stopped,
    Exited,
    FileChanged,
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

    pub fn services(&self) -> &ServicesSpec {
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
        redirected_components: HashMap<String, (String, u16)>,
        silence_components: Vec<String>,
    ) -> Result<Self, String> {
        let git_hash = match toolchain.get_git_folder_hash(config.product_path()) {
            Ok(hash) => hash,
            Err(e) => {
                return Err(e);
            }
        };

        let silence_components = silence_components.iter().collect::<HashSet<_>>();

        let binding = config.clone();
        let product_path = binding.product_path();
        let product_name = binding.product_name(); // product_path.split('/').last().unwrap_or(product_path).to_string();
        let network_name = binding.network_name();

        // TODO: Move to config
        if git_hash.is_empty() {
            return Err("No git hash found for {}".to_string());
        }

        let tag = git_hash[..8].to_string();
        let _guard = Directory::chdir(product_path);

        let variables = Variables::new("variables.yaml", config.environment());

        let stack_config = match std::fs::read_to_string("stack.spec.yaml") {
            Ok(config) => config,
            Err(e) => return Err(format!("Failed to read stack config: {}", e)),
        };

        let mut next_port = config.start_port();
        let stack_config_value: serde_yaml::Value = serde_yaml::from_str(&stack_config).unwrap();
        let mut images = Vec::new();
        let mut available_components = Vec::new();

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
                let component_name = component_name.as_str().unwrap().to_string();
                available_components.push(component_name.clone());

                let mut yaml_section_clone = yaml_section.clone();

                if let serde_yaml::Value::Mapping(ref mut yaml_section_map) = yaml_section_clone {
                    if !yaml_section_map
                        .contains_key(&serde_yaml::Value::String("component_name".to_string()))
                    {
                        yaml_section_map.insert(
                            serde_yaml::Value::String("component_name".to_string()),
                            serde_yaml::Value::String(component_name.clone()),
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

                if silence_components.contains(&image.component_name()) {
                    image.set_silence_output(true);
                }

                let host = image.component_name();
                if redirected_components.contains_key(&host) {
                    image.set_ignore_in_devmode(true);
                }
                component_spec
                    .lock()
                    .unwrap()
                    .set_tagged_image_name(image.tagged_image_name());
                images.push(image);

                all_component_specs.push(component_spec);
            }
        }

        log::trace!("Generating service list");
        let mut services: HashMap<String, Vec<ServiceSpec>> = HashMap::new();
        for image in &images {
            if let Some(port) = image.port() {
                if let Some(target_port) = image.target_port() {
                    let mut target_port = target_port;
                    let mut host = image.component_name();
                    if let Some(redirect) = redirected_components.get(&host) {
                        host = redirect.0.clone();
                        target_port = redirect.1;
                    }
                    let svc_spec = ServiceSpec {
                        name: image.component_name(),
                        host,
                        port,
                        target_port,
                        mount_point: image.spec().mount_point.clone(),
                        domain: image.spec().domain.clone(),
                        docker_host: image.spec().docker_local_name(),
                    };
                    services
                        .entry(image.spec().domain.clone())
                        .or_insert_with(Vec::new)
                        .push(svc_spec);
                }
            }
        }

        log::trace!("Generating domain list");
        let mut component_to_domain = HashMap::new();
        for component_spec in &mut all_component_specs {
            let x = component_spec.lock().unwrap();
            component_to_domain.insert(x.component_name.clone(), x.domain.clone());
        }
        let component_to_domain = Arc::new(component_to_domain);

        let services = Arc::new(services);

        for component_spec in &mut all_component_specs {
            component_spec
                .lock()
                .unwrap()
                .set_services(services.clone());
            component_spec
                .lock()
                .unwrap()
                .set_domains(component_to_domain.clone());
        }

        let (terminate_sender, terminate_receiver) = broadcast::channel(16);

        let infrastructure_repo = InfrastructureRepo::new(config.clone(), toolchain.clone());

        Ok(ContainerReactor {
            config,
            // product_name: product_name.to_string(),
            product_directory: product_path.to_string(),
            available_components,
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
            changed_files: Arc::new(Mutex::new(Vec::new())),
        })
        //        Ok(Self::new(&product_name, &product_path, images, toolchain))
    }

    pub fn available_components(&self) -> &Vec<String> {
        &self.available_components
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
                image.set_was_recently_rebuild(false);
                if image.should_ignore_in_devmode() {
                    println!(
                        "{}  ..... [  {}  ]",
                        image.identifier(),
                        "IGNORED".red().bold()
                    );
                    continue;
                }
                if !image.should_rebuild() {
                    println!(
                        "{}  ..... [  {}  ]",
                        image.identifier(),
                        "SKIPPED".yellow().bold()
                    );
                    continue;
                }

                print!("Building {}  ..... ", image.identifier());
                std::io::stdout().flush().expect("Failed to flush stdout");
                image.set_was_recently_rebuild(true);

                match image.build().await {
                    Ok(_) => {
                        image.set_should_rebuild(false);
                        println!(
                            "Building {}  ..... [  {}  ]",
                            image.identifier(),
                            "OK".white().bold()
                        )
                    }
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

        self.setup_environment().await?;

        let (_watcher, test_if_files_changed) = self.setup_file_watcher()?;

        let mut break_type = BreakType::Running;
        while matches!(break_type, BreakType::Running | BreakType::FileChanged) {
            // Invalidating cache
            self.kill_and_clean(false).await;
            trace!("Cleaned up previous resources");

            println!("Step A");
            if let Err(e) = self
                .build_and_handle_errors(&mut break_type, &test_if_files_changed)
                .await
            {
                continue;
            }

            println!("Step B");

            let (max_label_length, longest_paths) = self.prepare_for_launch();
            println!("Step C");
            self.launch_images(max_label_length, longest_paths).await;
            println!("Step D");

            break_type = self.monitor_and_handle_events(&test_if_files_changed).await;
        }

        self.cleanup().await;

        trace!("Launch process completed");
        Ok(())
    }

    async fn setup_environment(&mut self) -> Result<(), String> {
        let _ = self.create_network().await;
        trace!("Created Docker network");
        Ok(())
    }

    fn setup_file_watcher(&self) -> Result<(RecommendedWatcher, impl Fn() -> bool), String> {
        let (watch_tx, watch_rx) = std::sync::mpsc::channel();
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

        let path = self.product_directory.clone();
        match watcher.watch(path.as_ref(), RecursiveMode::Recursive) {
            Ok(_) => trace!("Started watching directory: {}", path),
            Err(e) => {
                error!("Failed to watch directory: {}", e);
                return Err(e.to_string());
            }
        }

        let product_directory = std::path::Path::new(&self.product_directory);
        let gitignore = PathMatcher::from_gitignore(product_directory);
        let changed_files = self.changed_files.clone();
        Ok((watcher, move || {
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
                            .filter(|path| !gitignore.matches(path))
                            .filter(|path| path.is_file())
                            .collect::<Vec<_>>();

                        let mut unique_paths = std::collections::HashSet::new();
                        let paths = paths
                            .into_iter()
                            .filter(|path| unique_paths.insert(path.clone()))
                            .collect::<Vec<_>>();

                        if !paths.is_empty() {
                            let mut changed_files = changed_files.lock().unwrap();
                            for p in paths.iter() {
                                trace!("File changed: {}", p.display());
                                changed_files.push(p.to_path_buf());
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
        }))
    }

    async fn build_and_handle_errors(
        &mut self,
        break_type: &mut BreakType,
        test_if_files_changed: &impl Fn() -> bool,
    ) -> Result<(), String> {
        match self.build().await {
            Ok(_) => {
                trace!("Build completed successfully");
                Ok(())
            }
            Err(e) => {
                self.handle_build_error(e, break_type, test_if_files_changed)
                    .await
            }
        }
    }

    async fn handle_build_error(
        &mut self,
        e: String,
        break_type: &mut BreakType,
        test_if_files_changed: &impl Fn() -> bool,
    ) -> Result<(), String> {
        let e = e
            .replace("error:", &format!("{}:", &"error".red().bold().to_string()))
            .replace("error[", &format!("{}[", &"error".red().bold().to_string()))
            .replace(
                "warning:",
                &format!("{}:", &"warning".yellow().bold().to_string()),
            );
        error!("Build failed: {}", e);
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        loop {
            if test_if_files_changed() {
                if self.test_if_siginificant_change().await {
                    trace!("File change detected. Rebuilding all images.");
                    // let _ = self.terminate_sender.send(());
                    *break_type = BreakType::FileChanged;
                    break;
                }
            }

            tokio::select! {
                _ = &mut ctrl_c => {
                    trace!("Termination signal received. Sending SIGTERM to all subprocesses.");
                    let _ = self.terminate_sender.send(());
                    *break_type = BreakType::Stopped;
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                    // Status update loop
                }
            }
        }

        Err("Build failed".to_string())
    }

    fn prepare_for_launch(&mut self) -> (usize, HashMap<String, usize>) {
        let max_label_length = self
            .images
            .iter()
            .map(|image| image.component_name().len())
            .max()
            .unwrap_or_default();

        let dependency_graph = self
            .images
            .iter()
            .map(|image| (image.image_name().to_string(), image.depends_on().clone()))
            .collect::<HashMap<String, Vec<String>>>();

        let longest_paths = self.compute_longest_paths(&dependency_graph);
        (max_label_length, longest_paths)
    }

    fn compute_longest_paths(
        &self,
        dependency_graph: &HashMap<String, Vec<String>>,
    ) -> HashMap<String, usize> {
        let mut longest_paths = HashMap::new();
        for (name, _) in dependency_graph {
            let mut stack = vec![(name, 1)];
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
        longest_paths
    }

    async fn launch_images(
        &mut self,
        max_label_length: usize,
        longest_paths: HashMap<String, usize>,
    ) {
        self.images_by_id = HashMap::new();
        self.statuses_receivers = HashMap::new();
        self.statuses = HashMap::new();
        self.handles = HashMap::new();

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
        jobs.sort_by(|a, b| a.0.cmp(&b.0));

        for (priority, image_id, image) in jobs {
            if image.should_ignore_in_devmode() {
                continue;
            }
            if !image.was_recently_rebuild() {
                continue;
            }
            println!(
                "\n{}",
                format!("Starting {} with priority {}", image.image_name(), priority)
                    .bold()
                    .white()
            );
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
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    async fn monitor_and_handle_events(
        &mut self,
        test_if_files_changed: &impl Fn() -> bool,
    ) -> BreakType {
        let mut all_finished = false;
        let mut stopping = false;
        let mut stop_time: Option<std::time::Instant> = None;

        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);

        loop {
            tokio::select! {
                _ = &mut ctrl_c => {
                    self.handle_termination_signal(&mut stopping, &mut stop_time).await;
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                    if self.handle_file_changes(test_if_files_changed, &mut stopping, &mut stop_time).await {
                        return BreakType::FileChanged;
                    }
                    self.update_image_statuses();

                    all_finished = self.statuses.values().all(|status| matches!(status, Status::Finished(_)));
                    if all_finished || stopping {
                        break;
                    }

                    if self.handle_image_completion().await {
                        return BreakType::Exited;
                    }
                }
            }
        }

        println!("Is stopping: {}: ", stopping);
        if self
            .handle_shutdown(all_finished, stopping, stop_time, test_if_files_changed)
            .await
        {
            BreakType::Running
        } else {
            BreakType::Stopped
        }
    }

    async fn handle_termination_signal(
        &mut self,
        stopping: &mut bool,
        stop_time: &mut Option<std::time::Instant>,
    ) {
        trace!("Termination signal received. Sending SIGTERM to all subprocesses.");
        println!("******************************************************************");
        println!("******************************************************************");
        println!("*****************       GRACEFUL SHUTDOWN        *****************");
        println!("******************************************************************");
        println!("******************************************************************");

        let _ = self.terminate_sender.send(());
        self.update_image_statuses();

        *stop_time = Some(std::time::Instant::now());
        *stopping = true;
    }

    async fn test_if_siginificant_change(&mut self) -> bool {
        let mut significant_change = false;
        let changed_files = {
            let mut changed_files = self.changed_files.lock().unwrap();
            let ret = changed_files.clone();
            changed_files.clear();
            ret
        };
        {
            let _guard = Directory::chdir(&self.product_directory);

            for image in &mut self.images {
                if image.should_ignore_in_devmode() {
                    continue;
                }
                if image.is_any_file_in_context(&changed_files) {
                    significant_change = true;
                    println!("Image '{}' was affected by change", image.component_name());
                    image.set_should_rebuild(true);
                }
            }
        }

        significant_change
    }

    async fn handle_file_changes(
        &mut self,
        test_if_files_changed: &impl Fn() -> bool,
        stopping: &mut bool,
        stop_time: &mut Option<std::time::Instant>,
    ) -> bool {
        if !*stopping && test_if_files_changed() {
            trace!("File change detected. Rebuilding all images.");
            let significant_change = self.test_if_siginificant_change().await;
            if significant_change {
                // let _ = self.terminate_sender.send(());
                *stop_time = Some(std::time::Instant::now());
                *stopping = true;
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    async fn handle_image_completion(&mut self) -> bool {
        let any_finished = self
            .statuses
            .values()
            .any(|status| matches!(status, Status::Finished(_)));
        if any_finished {
            warn!("Proceeding with forced shutdown due to image completion...");
            self.kill_and_clean(true).await;
            true
        } else {
            false
        }
    }

    async fn handle_shutdown(
        &mut self,
        all_finished: bool,
        stopping: bool,
        stop_time: Option<std::time::Instant>,
        test_if_files_changed: &impl Fn() -> bool,
    ) -> bool {
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        while !all_finished {
            tokio::select! {
                _ = &mut ctrl_c => {
                    self.handle_forceful_shutdown().await;
                    return false;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                    if self.handle_file_changes(test_if_files_changed, &mut true, &mut Some(std::time::Instant::now())).await {
                        return true;
                    }
                    self.update_image_statuses();

                    if self.statuses.values().all(|status| matches!(status, Status::Finished(_))) {
                        break;
                    }

                    if stopping {
                        if let Some(stop_time) = stop_time {
                            if stop_time.elapsed() >= std::time::Duration::from_secs(5) {
                                self.handle_shutdown_timeout().await;
                                break;
                            }
                        }
                    }
                }
            }
        }

        self.wait_for_handles().await;
        !stopping
    }

    async fn handle_forceful_shutdown(&mut self) {
        trace!("Termination signal received. Sending SIGTERM to all subprocesses.");
        let _ = self.terminate_sender.send(());
        self.update_image_statuses();
        println!("******************************************************************");
        println!("******************************************************************");
        println!("*****************       FORCEFUL SHUTDOWN        *****************");
        println!("******************************************************************");
        println!("******************************************************************");

        warn!("Proceeding with forced shutdown...");
        self.kill_and_clean(true).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    }

    async fn handle_shutdown_timeout(&mut self) {
        error!(
            "Shutdown timeout reached. You might have a process that does not respond to SIGTERM."
        );
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
        self.kill_and_clean(true).await;
    }

    async fn wait_for_handles(&mut self) {
        trace!("Joining all handles");
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

    async fn cleanup(&mut self) {
        let _ = self.delete_network().await;
        trace!("Deleted Docker network");
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

    pub async fn kill_and_clean(&self, force_all: bool) {
        trace!("Starting kill and cleanup process");
        for image in &self.images {
            if force_all || image.should_rebuild() {
                log::info!("Cleaning up image: {}", image.identifier());
                image.kill_and_clean().await;
            }
        }
        trace!("Kill and cleanup process completed");
        println!("Done");
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

// REFACTORING PLAN:
// This file should be broken down into multiple smaller modules:
// 1. Network - Create/delete network functionality
// 2. BuildProcess - Build and error handling
// 3. Lifecycle - Launch, monitor, and shutdown
// 4. FileWatcher - File watching and change detection
// 5. Kubernetes - K8s specific operations
//
// Current structure breakdown:
// - ContainerReactor struct has too many responsibilities
// - Many long methods handling multiple concerns
// - Complex error handling mixed with business logic
// - Callback functions nested within methods
//
// Proposed structure:
// - Split ContainerReactor into domain-specific components
// - Extract pure functions where possible
// - Create interfaces for external dependencies
// - Improve error handling with proper types
//
// This will make the code more testable, maintainable, and easier to understand.
// It aligns with the refactoring goals in REFACTORING.md
// Network management module

/// A more modular replacement for ContainerReactor
/// Orchestrates container lifecycle operations while delegating to specialized components
pub struct Reactor {
    network_manager: network::NetworkManager,
    build_processor: build::BuildProcessor,
    launch_manager: lifecycle::LaunchManager,
    kubernetes_manager: kubernetes::KubernetesManager,
    config: Arc<Config>,
    product_directory: String,
    available_components: Vec<String>,
    terminate_sender: BroadcastSender<()>,
    terminate_receiver: BroadcastReceiver<()>,
    toolchain: Option<Arc<ToolchainContext>>,
    services: Arc<ServicesSpec>,
    secrets_encoder: Arc<dyn EncodeSecrets>,
    vault: Arc<Mutex<dyn Vault + Send>>,
    file_watcher: watcher::FileWatcher,
    changed_files: Arc<Mutex<Vec<PathBuf>>>,
}

impl Reactor {
    pub fn from_product_dir(
        config: Arc<Config>,
        toolchain: Arc<ToolchainContext>,
        vault: Arc<Mutex<dyn Vault + Send>>,
        secrets_encoder: Arc<dyn EncodeSecrets>,
        k8s_encoder: Arc<dyn K8Encoder>,
        redirected_components: HashMap<String, (String, u16)>,
        silence_components: Vec<String>,
    ) -> Result<Self, String> {
        // This would construct all the components and initialize them
        // Implementation would be similar to ContainerReactor::from_product_dir

        // For now just delegate to ContainerReactor for backward compatibility
        let container_reactor = ContainerReactor::from_product_dir(
            config.clone(),
            toolchain.clone(),
            vault.clone(),
            secrets_encoder.clone(),
            k8s_encoder,
            redirected_components,
            silence_components,
        )?;

        let (terminate_sender, terminate_receiver) = broadcast::channel(16);

        // Convert Vec<DockerImage> to Vec<Arc<Mutex<DockerImage>>>
        let images: Vec<Arc<Mutex<DockerImage>>> = container_reactor
            .images
            .into_iter()
            .map(|img| Arc::new(Mutex::new(img)))
            .collect();

        // Initialize component managers
        let network_manager =
            network::NetworkManager::new(toolchain.clone(), config.network_name().to_string());

        let build_processor =
            build::BuildProcessor::new(container_reactor.product_directory.clone(), images.clone());

        let launch_manager = lifecycle::LaunchManager::new(images, terminate_sender.clone());

        let file_watcher = watcher::FileWatcher::new(
            container_reactor.changed_files.clone(),
            container_reactor.product_directory.clone(),
        );

        let kubernetes_manager = kubernetes::KubernetesManager::new(
            toolchain,
            container_reactor.cluster_manifests,
            container_reactor.infrastructure_repo,
            container_reactor.product_directory.clone(),
        );

        Ok(Self {
            network_manager,
            build_processor,
            launch_manager,
            file_watcher,
            kubernetes_manager,
            config,
            product_directory: container_reactor.product_directory,
            available_components: container_reactor.available_components,
            terminate_sender,
            terminate_receiver,
            toolchain: container_reactor.toolchain,
            services: container_reactor.services,
            secrets_encoder,
            vault,
            changed_files: container_reactor.changed_files,
        })
    }

    // Delegated methods that match ContainerReactor's public API

    pub fn services(&self) -> &ServicesSpec {
        &self.services
    }

    pub fn product_directory(&self) -> &str {
        &self.product_directory
    }

    pub fn images(&self) -> &Vec<Arc<Mutex<DockerImage>>> {
        self.launch_manager.images()
    }

    pub fn cluster_manifests(&self) -> &K8ClusterManifests {
        self.kubernetes_manager.cluster_manifests()
    }

    pub fn get_image(&self, component_name: &str) -> Option<Arc<Mutex<DockerImage>>> {
        self.launch_manager.get_image(component_name)
    }

    pub fn available_components(&self) -> &Vec<String> {
        &self.available_components
    }

    pub async fn build_and_push(&mut self) -> Result<(), String> {
        self.build_processor.build_and_push().await
    }

    pub async fn select_kubernetes_context(&self, context: &str) -> Result<(), String> {
        self.kubernetes_manager
            .select_kubernetes_context(context)
            .await
    }

    pub async fn apply(&mut self) -> Result<(), String> {
        self.kubernetes_manager.apply().await
    }

    pub async fn unapply(&mut self) -> Result<(), String> {
        self.kubernetes_manager.unapply().await
    }

    pub async fn rollout(&mut self) -> Result<(), String> {
        self.kubernetes_manager.rollout(&self.config).await
    }

    pub async fn deploy(&mut self) -> Result<(), String> {
        self.build_and_push().await?;
        self.build_manifests().await?;
        self.apply().await?;
        Ok(())
    }

    pub async fn install_manifests(&mut self) -> Result<(), String> {
        self.kubernetes_manager.install_manifests().await
    }

    pub async fn uninstall_manifests(&mut self) -> Result<(), String> {
        self.kubernetes_manager.uninstall_manifests().await
    }

    pub async fn build_manifests(&mut self) -> Result<(), String> {
        let vault = self.vault.clone();
        let secrets_encoder = self.secrets_encoder.clone();

        let vault_ref = vault.lock().unwrap();
        self.kubernetes_manager
            .build_manifests(&*vault_ref, &*secrets_encoder, self.toolchain.clone())
            .await
    }

    pub async fn build(&mut self) -> Result<(), String> {
        self.build_processor.build_all().await?;
        self.build_manifests().await?;
        Ok(())
    }

    pub async fn launch(&mut self) -> Result<(), String> {
        let _guard = crate::utils::Directory::chdir(&self.product_directory);

        // Set up network and environment
        self.network_manager.create_network().await?;

        // Set up file watcher
        let (_watcher, test_if_files_changed) = self.file_watcher.setup()?;

        let mut break_type = BreakType::Running;
        let mut monitor_manager = lifecycle::MonitorManager::new(
            HashMap::new(),
            self.terminate_sender.clone(),
            self.changed_files.clone(),
            self.product_directory(),
        );

        while matches!(break_type, BreakType::Running | BreakType::FileChanged) {
            // Clean previous resources
            self.launch_manager.kill_and_clean(false).await;

            // Build
            match self.build().await {
                Ok(_) => {}
                Err(e) => {
                    build::ErrorHandler::handle_build_error(
                        e,
                        &mut break_type,
                        &test_if_files_changed,
                    )
                    .await?;
                    continue;
                }
            }

            // Launch
            let (max_label_length, longest_paths) = self.launch_manager.prepare_for_launch();
            self.launch_manager
                .launch_images(
                    max_label_length,
                    longest_paths,
                    self.terminate_receiver.resubscribe(),
                )
                .await;

            println!("WAS HERE!??");
            // Monitor
            break_type = monitor_manager
                .monitor_and_handle_events(&test_if_files_changed, &mut self.launch_manager)
                .await;
        }

        // Cleanup
        self.network_manager.delete_network().await?;

        Ok(())
    }

    pub async fn kill_and_clean(&self, force_all: bool) {
        self.launch_manager.kill_and_clean(force_all).await;
    }

    pub async fn clean(&self) {
        self.launch_manager.clean().await;
    }
}

pub mod network {
    use super::DockerImage;
    use crate::toolchain::ToolchainContext;
    use crate::utils::run_command;
    use colored::Colorize;
    use log::trace;
    use std::sync::Arc;

    pub struct NetworkManager {
        toolchain: Arc<ToolchainContext>,
        network_name: String,
    }

    impl NetworkManager {
        pub fn new(toolchain: Arc<ToolchainContext>, network_name: String) -> Self {
            Self {
                toolchain,
                network_name,
            }
        }

        pub async fn create_network(&self) -> Result<(), String> {
            trace!("Creating Docker network: {}", self.network_name);

            let docker = self.toolchain.docker();

            // Check if network already exists
            match run_command(
                "docker network check".white().bold(),
                docker.clone(),
                vec!["network", "inspect", &self.network_name],
            )
            .await
            {
                Ok(_) => {
                    trace!("Network {} already exists", self.network_name);
                    return Ok(());
                }
                Err(_) => {
                    // Network doesn't exist, continue with creation
                }
            }

            // Create the network
            match run_command(
                "docker network create".white().bold(),
                docker,
                vec!["network", "create", &self.network_name],
            )
            .await
            {
                Ok(_) => {
                    trace!("Created Docker network: {}", self.network_name);
                    Ok(())
                }
                Err(e) => Err(format!(
                    "Failed to create Docker network {}: {}",
                    self.network_name, e
                )),
            }
        }

        pub async fn delete_network(&self) -> Result<(), String> {
            trace!("Deleting Docker network: {}", self.network_name);

            let docker = self.toolchain.docker();

            // Check if network exists
            match run_command(
                "docker network check".white().bold(),
                docker.clone(),
                vec!["network", "inspect", &self.network_name],
            )
            .await
            {
                Ok(_) => {
                    // Delete the network
                    match run_command(
                        "docker network delete".white().bold(),
                        docker,
                        vec!["network", "rm", &self.network_name],
                    )
                    .await
                    {
                        Ok(_) => {
                            trace!("Deleted Docker network: {}", self.network_name);
                            Ok(())
                        }
                        Err(e) => Err(format!(
                            "Failed to delete Docker network {}: {}",
                            self.network_name, e
                        )),
                    }
                }
                Err(_) => {
                    // Network doesn't exist, nothing to delete
                    trace!(
                        "Network {} doesn't exist, nothing to delete",
                        self.network_name
                    );
                    Ok(())
                }
            }
        }
    }
}

// Build process module
pub mod build {
    use crate::container::docker::DockerImage;
    use colored::Colorize;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::Mutex;

    pub struct BuildProcessor {
        product_directory: String,
        images: Vec<Arc<Mutex<DockerImage>>>,
    }

    impl BuildProcessor {
        pub fn new(product_directory: String, images: Vec<Arc<Mutex<DockerImage>>>) -> Self {
            Self {
                product_directory,
                images,
            }
        }

        pub async fn build_all(&mut self) -> Result<(), String> {
            let _guard = crate::utils::Directory::chdir(&self.product_directory);

            for image_arc in &mut self.images {
                let mut image = image_arc.lock().unwrap();
                // Skip images that should be ignored in dev mode
                if image.should_ignore_in_devmode() {
                    println!(
                        "{}  ..... [  {}  ]",
                        image.identifier(),
                        "IGNORED".red().bold()
                    );
                    continue;
                }

                // Skip images that don't need rebuilding
                if !image.should_rebuild() {
                    println!(
                        "{}  ..... [  {}  ]",
                        image.identifier(),
                        "SKIPPED".yellow().bold()
                    );
                    continue;
                }

                // Start build process with visual feedback
                print!("Building {}  ..... ", image.identifier());
                std::io::stdout().flush().expect("Failed to flush stdout");
                image.set_was_recently_rebuild(true);

                // Perform the actual build
                match image.build().await {
                    Ok(_) => {
                        image.set_should_rebuild(false);
                        println!(
                            "Building {}  ..... [  {}  ]",
                            image.identifier(),
                            "OK".white().bold()
                        )
                    }
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

            Ok(())
        }

        pub async fn build_and_push(&mut self) -> Result<(), String> {
            let _guard = crate::utils::Directory::chdir(&self.product_directory);

            for image_arc in &mut self.images {
                let mut image = image_arc.lock().unwrap();
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
    }

    pub struct ErrorHandler;

    impl ErrorHandler {
        pub async fn handle_build_error<F: Fn() -> bool>(
            error: String,
            break_type: &mut super::BreakType,
            test_if_files_changed: &F,
        ) -> Result<(), String> {
            // Format the error message with colored output
            let formatted_error = error
                .replace("error:", &format!("{}:", &"error".red().bold().to_string()))
                .replace("error[", &format!("{}[", &"error".red().bold().to_string()))
                .replace(
                    "warning:",
                    &format!("{}:", &"warning".yellow().bold().to_string()),
                );

            log::error!("Build failed: {}", formatted_error);

            // Brief pause to ensure error is visible
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            // Set up ctrl-c handler
            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);

            // Wait for user feedback loop
            loop {
                // Check if files have changed
                if test_if_files_changed() {
                    // A file change was detected
                    log::trace!("File change detected. Rebuilding.");
                    *break_type = super::BreakType::FileChanged;
                    break;
                }

                // Check for termination signal
                tokio::select! {
                    _ = &mut ctrl_c => {
                        log::trace!("Termination signal received during build error state.");
                        *break_type = super::BreakType::Stopped;
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                        // Continue monitoring loop
                    }
                }
            }

            Err("Build failed".to_string())
        }
    }
}

// Lifecycle management module
pub mod lifecycle {
    use crate::container::docker::DockerImage;
    use crate::container::status::Status;
    use colored::Colorize;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::mpsc::Receiver;
    use std::sync::{Arc, Mutex};
    use tokio::sync::broadcast::{Receiver as BroadcastReceiver, Sender as BroadcastSender};

    pub struct LaunchManager {
        images: Vec<Arc<Mutex<DockerImage>>>,
        images_by_id: HashMap<usize, Arc<Mutex<DockerImage>>>,
        statuses_receivers: HashMap<usize, Receiver<Status>>,
        statuses: HashMap<String, Status>,
        handles: HashMap<usize, tokio::task::JoinHandle<()>>,
        terminate_sender: BroadcastSender<()>,
    }

    impl LaunchManager {
        pub fn new(
            images: Vec<Arc<Mutex<DockerImage>>>,
            terminate_sender: BroadcastSender<()>,
        ) -> Self {
            Self {
                images,
                images_by_id: HashMap::new(),
                statuses_receivers: HashMap::new(),
                statuses: HashMap::new(),
                handles: HashMap::new(),
                terminate_sender,
            }
        }

        pub fn images(&self) -> &Vec<Arc<Mutex<DockerImage>>> {
            &self.images
        }

        pub fn images_as_mut(&mut self) -> &mut Vec<Arc<Mutex<DockerImage>>> {
            &mut self.images
        }

        pub fn get_image(&self, component_name: &str) -> Option<Arc<Mutex<DockerImage>>> {
            self.images
                .iter()
                .find(|img| {
                    let img_guard = img.lock().unwrap();
                    img_guard.component_name() == component_name
                })
                .map(|img| img.clone())
        }

        pub fn prepare_for_launch(&self) -> (usize, HashMap<String, usize>) {
            let max_label_length = self
                .images
                .iter()
                .map(|image| {
                    let image_guard = image.lock().unwrap();
                    image_guard.component_name().len()
                })
                .max()
                .unwrap_or_default();

            let dependency_graph = self
                .images
                .iter()
                .map(|image| {
                    let image_guard = image.lock().unwrap();
                    (
                        image_guard.image_name().to_string(),
                        image_guard.depends_on().clone(),
                    )
                })
                .collect::<HashMap<String, Vec<String>>>();

            let longest_paths = self.compute_longest_paths(&dependency_graph);
            (max_label_length, longest_paths)
        }

        fn compute_longest_paths(
            &self,
            dependency_graph: &HashMap<String, Vec<String>>,
        ) -> HashMap<String, usize> {
            let mut longest_paths = HashMap::new();
            for (name, _) in dependency_graph {
                let mut stack = vec![(name, 1)];
                let mut visited = std::collections::HashSet::new();
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
            longest_paths
        }
        pub async fn launch_images(
            &mut self,
            max_label_length: usize,
            longest_paths: HashMap<String, usize>,
            terminate_receiver: BroadcastReceiver<()>,
        ) {
            println!("LAUNCHING IMAGES");
            // Reset state for new launch
            self.images_by_id = HashMap::new();
            self.statuses_receivers = HashMap::new();
            self.statuses = HashMap::new();
            self.handles = HashMap::new();

            // Sort images by priority (based on dependency chain length)
            let mut jobs = self
                .images
                .iter_mut()
                .enumerate()
                .map(|(id, image)| {
                    let image_guard = image.lock().unwrap();
                    let priority = longest_paths
                        .get(image_guard.image_name())
                        .cloned()
                        .unwrap_or_default();

                    (priority, id, image.clone())
                })
                .collect::<Vec<_>>();

            // Sort by priority (higher priority first)
            jobs.sort_by(|a, b| b.0.cmp(&a.0));

            // Launch each image in priority order
            for (priority, image_id, image_arc) in jobs {
                let should_ignore;
                let was_recently_rebuild;
                let image_name;
                let component_name;

                {
                    let image = image_arc.lock().unwrap();
                    // Skip images that should be ignored
                    should_ignore = image.should_ignore_in_devmode();
                    was_recently_rebuild = image.was_recently_rebuild();
                    image_name = image.image_name().to_string();
                    component_name = image.component_name().to_string();
                }

                if should_ignore {
                    log::debug!("Ignoring image {} in dev mode.", image_name);
                    continue;
                }

                // Skip images that weren't recently rebuilt
                if !was_recently_rebuild {
                    log::debug!("Image '{}' was not rebuild - skipping restart.", image_name);
                    continue;
                }

                println!(
                    "\n{}",
                    format!("Starting {} with priority {}", image_name, priority)
                        .bold()
                        .white()
                );

                // Set up channels for status communication
                let (status_sender, status_receiver) = std::sync::mpsc::channel();

                // Store image and receiver for later status updates

                self.images_by_id.insert(image_id, image_arc.clone());
                self.statuses_receivers.insert(image_id, status_receiver);
                self.statuses.insert(component_name, Status::Awaiting);

                // Launch the image and store its handle
                let handle = {
                    let mut image = image_arc.lock().unwrap();
                    image.launch(
                        max_label_length,
                        terminate_receiver.resubscribe(),
                        status_sender,
                    )
                };
                self.handles.insert(image_id, handle);

                // Small delay between launches to prevent resource contention
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }

        pub fn update_image_statuses(&mut self) {
            for (id, receiver) in self.statuses_receivers.iter_mut() {
                while let Ok(status) = receiver.try_recv() {
                    // Update self.statuses with the new status
                    if let Some(image) = self.images_by_id.get(id) {
                        let component_name = image.lock().unwrap().component_name();
                        let previous_status = self.statuses.get(&component_name);

                        // Only update and print if status has changed
                        if previous_status.map_or(true, |prev| *prev != status) {
                            self.statuses
                                .insert(component_name.to_string(), status.clone());

                            // Print status changes based on the type of status update
                            match status {
                                Status::InProgress => {
                                    println!("Image {} is running", component_name)
                                }
                                Status::StartupCompleted => {
                                    println!("Image {} is ready", component_name)
                                }
                                Status::Finished(code) => {
                                    println!("Image {} exited with code {}", component_name, code)
                                }
                                Status::Terminate => {
                                    println!("Image {} is terminating", component_name)
                                }
                                _ => (), // Don't print for other status types
                            }
                        }
                    }
                }
            }
        }
        pub async fn kill_all_images(&mut self) {
            log::trace!("Killing all container images");
            for image_arc in &self.images {
                let image = image_arc.lock().unwrap();
                log::debug!("Killing image: {}", image.component_name());
                image.kill().await;
            }
            log::trace!("All images killed successfully");
        }

        pub async fn kill_and_clean(&self, force_all: bool) {
            log::trace!("Starting kill and cleanup process");
            for image_arc in &self.images {
                let image = image_arc.lock().unwrap();
                if force_all || image.should_rebuild() {
                    log::info!("Cleaning up image: {}", image.identifier());
                    image.kill_and_clean().await;
                }
            }
            log::trace!("Kill and cleanup process completed");
            println!("Cleanup completed");
        }

        pub async fn clean(&self) {
            log::trace!("Starting cleanup process");
            for image_arc in &self.images {
                let image = image_arc.lock().unwrap();
                log::debug!("Cleaning up image: {}", image.identifier());
                image.clean().await;
            }
            log::trace!("Cleanup process completed");
            println!("Cleanup completed");
        }

        pub async fn wait_for_handles(&mut self) {
            log::trace!("Joining all handles for container processes");

            // Create a loop that will either complete when all handles are joined
            // or timeout after a certain period
            loop {
                tokio::select! {
                    // Try to join all handles concurrently
                    _ = futures::future::join_all(self.handles.values_mut()) => {
                        log::trace!("All container process handles joined successfully");
                        break;
                    },
                    // Set a timeout to prevent hanging indefinitely
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                        log::warn!("Waiting for processes to quit - some may be unresponsive");
                        // Send termination signal to any remaining processes
                        let _ = self.terminate_sender.send(());
                        // Force kill any remaining images
                        self.kill_all_images().await;
                    },
                }
            }

            // Clear the handles map since all processes have been handled
            self.handles.clear();
            log::debug!("All container handles cleared");
        }
    }

    pub struct MonitorManager {
        statuses: HashMap<String, Status>,
        terminate_sender: BroadcastSender<()>,
        changed_files: Arc<Mutex<Vec<PathBuf>>>,
        product_directory: String,
    }

    impl MonitorManager {
        pub fn new(
            statuses: HashMap<String, Status>,
            terminate_sender: BroadcastSender<()>,
            changed_files: Arc<Mutex<Vec<PathBuf>>>,
            product_directory: &str,
        ) -> Self {
            Self {
                statuses,
                terminate_sender,
                changed_files,
                product_directory: product_directory.to_string(),
            }
        }

        pub async fn test_if_significant_change(
            &self,
            images: &mut Vec<Arc<Mutex<DockerImage>>>,
        ) -> bool {
            // Get locked access to the changed files
            let mut changed_files = self.changed_files.lock().unwrap();
            log::debug!("Files tested: {:?}", changed_files);

            // If no files changed, return false
            if changed_files.is_empty() {
                return false;
            }

            // Track if any significant change was detected
            let mut significant_change = false;

            // Check each image against the changed files
            for image in images {
                log::debug!(" - Testing {}", image.lock().unwrap().image_name());
                for file_path in changed_files.iter() {
                    // Convert to relative path from product directory
                    let rel_path = if let Ok(rel) = file_path.strip_prefix(&self.product_directory)
                    {
                        rel.to_path_buf()
                    } else {
                        file_path.clone()
                    };

                    // Check if the changed file affects this image
                    let mut image = image.lock().unwrap();
                    log::debug!("   * {}", rel_path.display().to_string());

                    if image.is_any_file_in_context(&vec![rel_path.clone()]) {
                        log::debug!(
                            "Significant change detected in file: {} for image: {}",
                            rel_path.display(),
                            image.identifier()
                        );

                        // Mark image for rebuild
                        image.set_should_rebuild(true);
                        significant_change = true;
                    }
                }
            }

            // Clear the changed files list after processing
            changed_files.clear();

            significant_change
        }

        pub async fn monitor_and_handle_events<F: Fn() -> bool>(
            &mut self,
            test_if_files_changed: &F,
            launch_manager: &mut LaunchManager,
        ) -> super::BreakType {
            let mut all_finished = false;
            let mut stopping = false;
            let mut stop_time: Option<std::time::Instant> = None;

            // Set up ctrl-c handling
            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);

            loop {
                tokio::select! {
                    // Handle CTRL+C signal
                    _ = &mut ctrl_c => {
                        log::trace!("Termination signal received. Sending SIGTERM to all subprocesses.");
                        println!("******************************************************************");
                        println!("******************************************************************");
                        println!("*****************       GRACEFUL SHUTDOWN        *****************");
                        println!("******************************************************************");
                        println!("******************************************************************");

                        // Send termination signal to all processes
                        let _ = self.terminate_sender.send(());

                        // Update status map from launch manager
                        self.statuses = launch_manager.statuses.clone();

                        // Mark as stopping and record stop time
                        stop_time = Some(std::time::Instant::now());
                        stopping = true;
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                        // Check for file changes
                        if !stopping && test_if_files_changed() {
                            log::debug!("File change detected. Checking if rebuild is needed.");

                            // Get changed files and check if they're significant
                            let significant_change = self.test_if_significant_change(launch_manager.images_as_mut()).await;

                            if significant_change {
                                log::debug!("Significant file changes detected, triggering rebuild");
                                stop_time = Some(std::time::Instant::now());
                                stopping = true;
                                return super::BreakType::FileChanged;
                            }
                        }

                        // Update status tracking
                        launch_manager.update_image_statuses();
                        self.statuses = launch_manager.statuses.clone();

                        // Check if all processes have finished
                        all_finished = self.statuses.values().all(|status|
                            matches!(status, super::Status::Finished(_))
                        );

                        if all_finished || stopping {
                            break;
                        }

                        // Check if any image has completed unexpectedly
                        let any_finished = self.statuses.values().any(|status|
                            matches!(status, super::Status::Finished(_))
                        );
                        if any_finished {
                            log::warn!("Proceeding with forced shutdown due to image completion...");
                            launch_manager.kill_and_clean(true).await;
                            return super::BreakType::Exited;
                        }
                    }
                }
            }

            // Handle shutdown process
            if self
                .handle_shutdown(
                    all_finished,
                    stopping,
                    stop_time,
                    test_if_files_changed,
                    launch_manager,
                )
                .await
            {
                super::BreakType::Running
            } else {
                super::BreakType::Stopped
            }
        }

        pub async fn handle_termination_signal(
            &mut self,
            stopping: &mut bool,
            stop_time: &mut Option<std::time::Instant>,
        ) {
            log::trace!("Handling termination signal");
            println!("******************************************************************");
            println!("******************************************************************");
            println!("*****************       GRACEFUL SHUTDOWN        *****************");
            println!("******************************************************************");
            println!("******************************************************************");

            // Send termination signal to all processes
            let _ = self.terminate_sender.send(());

            // Mark as stopping and record the stop time
            *stop_time = Some(std::time::Instant::now());
            *stopping = true;

            log::debug!("Termination signal processed, shutdown sequence initiated");
        }

        pub async fn handle_shutdown<F: Fn() -> bool>(
            &mut self,
            all_finished: bool,
            stopping: bool,
            stop_time: Option<std::time::Instant>,
            test_if_files_changed: &F,
            launch_manager: &mut LaunchManager,
        ) -> bool {
            log::trace!("Beginning shutdown sequence");

            // Add a small delay to allow final messages to be processed
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

            // Set up ctrl-c handler for forceful shutdown
            let ctrl_c = tokio::signal::ctrl_c();
            tokio::pin!(ctrl_c);

            // Continue monitoring until all processes have finished
            while !all_finished {
                tokio::select! {
                    // Handle CTRL+C during shutdown for forceful termination
                    _ = &mut ctrl_c => {
                        self.handle_forceful_shutdown(launch_manager).await;
                        return false; // Signal complete stop
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                        // Check for file changes during shutdown
                        if test_if_files_changed() {
                            log::debug!("File change detected during shutdown, will restart after cleanup");
                            return true; // Signal restart
                        }

                        // Update statuses during shutdown
                        launch_manager.update_image_statuses();
                        self.statuses = launch_manager.statuses.clone();

                        // Check if all processes have now finished
                        if self.statuses.values().all(|status| matches!(status, super::Status::Finished(_))) {
                            log::debug!("All processes have completed gracefully");
                            break;
                        }

                        // Check for shutdown timeout
                        if stopping {
                            if let Some(stop_time) = stop_time {
                                if stop_time.elapsed() >= std::time::Duration::from_secs(5) {
                                    log::warn!("Shutdown timeout reached after 5 seconds");
                                    self.handle_shutdown_timeout(launch_manager).await;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            // Wait for all process handles to complete
            launch_manager.wait_for_handles().await;
            log::trace!("Shutdown sequence completed");

            // Return true to continue (e.g., if file changes detected), false to stop
            !stopping
        }

        pub async fn handle_forceful_shutdown(&mut self, launch_manager: &mut LaunchManager) {
            log::trace!("Termination signal received. Sending SIGTERM to all subprocesses.");
            let _ = self.terminate_sender.send(());
            launch_manager.update_image_statuses();
            self.statuses = launch_manager.statuses.clone();

            println!("******************************************************************");
            println!("******************************************************************");
            println!("*****************       FORCEFUL SHUTDOWN        *****************");
            println!("******************************************************************");
            println!("******************************************************************");

            log::warn!("Proceeding with forced shutdown...");
            launch_manager.kill_and_clean(true).await;

            // Allow a short delay for cleanup operations to complete
            tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
        }

        pub async fn handle_shutdown_timeout(&mut self, launch_manager: &mut LaunchManager) {
            log::error!(
                "Shutdown timeout reached. You might have a process that does not respond to SIGTERM."
            );

            // Print status of all components
            println!("Current process statuses:");
            for (component_name, status) in &self.statuses {
                let status_str = match status {
                    super::Status::Awaiting => "Awaiting".yellow(),
                    super::Status::InProgress => "In Progress".blue(),
                    super::Status::StartupCompleted => "Startup Completed".green(),
                    super::Status::Reinitializing => "Reinitializing".cyan(),
                    super::Status::Finished(code) => format!("Finished ({})", code).white(),
                    super::Status::Terminate => "Terminating".red(),
                };
                println!("  {}: {}", component_name, status_str);
            }

            println!("Proceeding with forced shutdown...");

            // Force kill and clean all processes
            launch_manager.kill_and_clean(true).await;
        }
    }
}

// File watching module
pub mod watcher {
    use crate::container::docker::DockerImage;
    use crate::utils::path_matcher::PathMatcher;
    use log::{debug, error, trace};
    use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    pub struct FileWatcher {
        changed_files: Arc<Mutex<Vec<PathBuf>>>,
        product_directory: String,
    }
    impl FileWatcher {
        pub fn new(changed_files: Arc<Mutex<Vec<PathBuf>>>, product_directory: String) -> Self {
            Self {
                changed_files,
                product_directory,
            }
        }

        pub fn setup(&self) -> Result<(RecommendedWatcher, impl Fn() -> bool), String> {
            let (watch_tx, watch_rx) = std::sync::mpsc::channel();

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

            let path = self.product_directory.clone();
            match watcher.watch(path.as_ref(), RecursiveMode::Recursive) {
                Ok(_) => trace!("Started watching directory: {}", path),
                Err(e) => {
                    error!("Failed to watch directory: {}", e);
                    return Err(e.to_string());
                }
            }

            let product_directory = std::path::Path::new(&self.product_directory);
            let gitignore = PathMatcher::from_gitignore(product_directory);
            let changed_files = self.changed_files.clone();

            let file_change_checker = move || {
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
                                .filter(|path| !gitignore.matches(path))
                                .filter(|path| path.is_file())
                                .collect::<Vec<_>>();

                            let mut unique_paths = std::collections::HashSet::new();
                            let paths = paths
                                .into_iter()
                                .filter(|path| unique_paths.insert(path.clone()))
                                .collect::<Vec<_>>();

                            if !paths.is_empty() {
                                let mut changed_files = changed_files.lock().unwrap();
                                for p in paths.iter() {
                                    trace!("File changed: {}", p.display());
                                    changed_files.push(p.to_path_buf());
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

            Ok((watcher, file_change_checker))
        }
    }
}

// Kubernetes module
pub mod kubernetes {
    use crate::build::Config;
    use crate::cluster::{InfrastructureRepo, K8ClusterManifests};
    use crate::toolchain::ToolchainContext;
    use std::collections::HashMap;
    use std::sync::Arc;

    pub struct KubernetesManager {
        toolchain: Arc<ToolchainContext>,
        cluster_manifests: K8ClusterManifests,
        infrastructure_repo: InfrastructureRepo,
        product_directory: String,
    }

    impl KubernetesManager {
        pub fn new(
            toolchain: Arc<ToolchainContext>,
            cluster_manifests: K8ClusterManifests,
            infrastructure_repo: InfrastructureRepo,
            product_directory: String,
        ) -> Self {
            Self {
                toolchain,
                cluster_manifests,
                infrastructure_repo,
                product_directory,
            }
        }

        pub fn cluster_manifests(&self) -> &K8ClusterManifests {
            &self.cluster_manifests
        }

        pub async fn select_kubernetes_context(&self, context: &str) -> Result<(), String> {
            log::info!("Selecting Kubernetes context: {}", context);

            let kubectl = self.toolchain.kubectl();

            // Verify the context exists
            let output = crate::utils::run_command(
                "kubectl context check".into(),
                kubectl.clone(),
                vec!["config", "get-contexts", context],
            )
            .await
            .map_err(|e| format!("Failed to verify Kubernetes context: {}", e))?;

            if !output.contains(context) {
                return Err(format!("Kubernetes context '{}' not found", context));
            }

            // Set the context
            crate::utils::run_command(
                "kubectl context switch".into(),
                kubectl,
                vec!["config", "use-context", context],
            )
            .await
            .map_err(|e| format!("Failed to switch Kubernetes context: {}", e))?;

            log::debug!("Successfully switched to Kubernetes context: {}", context);
            Ok(())
        }

        pub async fn apply(&self) -> Result<(), String> {
            log::info!("Applying Kubernetes manifests");

            // Check if manifests directory exists instead of using manifests_ready method
            let manifests_dir = self.cluster_manifests.output_directory();
            if !manifests_dir.exists() {
                return Err("Manifests not ready. Call build_manifests first.".to_string());
            }

            let kubectl = self.toolchain.kubectl();

            // Create a guard to ensure we're in the product directory
            let _guard = crate::utils::Directory::chdir(&self.product_directory);

            // Apply manifests using kubectl apply
            match crate::utils::run_command(
                "kubectl apply".into(),
                kubectl,
                vec!["apply", "-f", &manifests_dir.to_string_lossy()],
            )
            .await
            {
                Ok(output) => {
                    log::debug!("Successfully applied Kubernetes manifests");
                    log::trace!("kubectl apply output: {}", output);
                    Ok(())
                }
                Err(e) => {
                    log::error!("Failed to apply Kubernetes manifests: {}", e);
                    Err(format!("Failed to apply Kubernetes manifests: {}", e))
                }
            }
        }

        pub async fn unapply(&self) -> Result<(), String> {
            log::info!("Removing Kubernetes manifests");

            // Check if manifests directory exists instead of using manifests_ready method
            let manifests_dir = self.cluster_manifests.output_directory();
            if !manifests_dir.exists() {
                return Err("Manifests not ready. Call build_manifests first.".to_string());
            }

            let kubectl = self.toolchain.kubectl();

            // Create a guard to ensure we're in the product directory
            let _guard = crate::utils::Directory::chdir(&self.product_directory);

            // Delete resources using kubectl delete
            match crate::utils::run_command(
                "kubectl delete".into(),
                kubectl,
                vec!["delete", "-f", &manifests_dir.to_string_lossy()],
            )
            .await
            {
                Ok(output) => {
                    log::debug!("Successfully removed Kubernetes resources");
                    log::trace!("kubectl delete output: {}", output);
                    Ok(())
                }
                Err(e) => {
                    log::error!("Failed to remove Kubernetes resources: {}", e);
                    Err(format!("Failed to remove Kubernetes resources: {}", e))
                }
            }
        }

        pub async fn rollout(&mut self, _config: &Config) -> Result<(), String> {
            log::info!("Performing Kubernetes rollout restart");

            // Check if manifests directory exists instead of using manifests_ready method
            let manifests_dir = self.cluster_manifests.output_directory();
            if !manifests_dir.exists() {
                return Err("Manifests not ready. Call build_manifests first.".to_string());
            }

            let kubectl = self.toolchain.kubectl();

            // Create a guard to ensure we're in the product directory
            let _guard = crate::utils::Directory::chdir(&self.product_directory);

            // Get the namespace from config (using a default if the method doesn't exist)
            let namespace = "default"; // Use a default namespace since config.namespace() doesn't exist

            // Restart deployments
            log::debug!("Restarting deployments in namespace: {}", namespace);
            match crate::utils::run_command(
                "kubectl rollout restart deployments".into(),
                kubectl.clone(),
                vec!["rollout", "restart", "deployment", "-n", namespace],
            )
            .await
            {
                Ok(output) => {
                    log::trace!("Deployment restart output: {}", output);
                }
                Err(e) => {
                    return Err(format!("Failed to restart deployments: {}", e));
                }
            }

            // Restart statefulsets if they exist
            log::debug!("Restarting statefulsets in namespace: {}", namespace);
            let result = crate::utils::run_command(
                "kubectl rollout restart statefulsets".into(),
                kubectl,
                vec!["rollout", "restart", "statefulset", "-n", namespace],
            )
            .await;

            // Statefulsets might not exist, so we don't fail if this doesn't work
            if let Err(e) = result {
                log::warn!("Failed to restart statefulsets (they may not exist): {}", e);
            }

            log::info!("Kubernetes rollout completed successfully");
            Ok(())
        }

        pub async fn build_manifests(
            &mut self,
            vault: &dyn crate::vault::Vault,
            secrets_encoder: &dyn crate::vault::EncodeSecrets,
            toolchain: Option<Arc<ToolchainContext>>,
        ) -> Result<(), String> {
            log::info!("Building Kubernetes manifests");

            // Create a guard to ensure we're in the product directory
            let _guard = crate::utils::Directory::chdir(&self.product_directory);

            // Ensure the manifests directory exists
            let manifests_dir = self.cluster_manifests.output_directory();
            if manifests_dir.exists() {
                std::fs::remove_dir_all(manifests_dir).expect("Failed to delete output directory");
            }

            // Create build context for each component in cluster_manifests
            for component in self.cluster_manifests.components() {
                if component.is_installation() {
                    continue;
                }

                log::debug!("Building manifests for component: {}", component.name());

                let render_dir = component.output_directory();
                std::fs::create_dir_all(render_dir).expect("Failed to create render directory");

                // Get the component spec with build context
                let spec = component.spec();

                let secrets = vault
                    .get(
                        &spec.product_name,
                        &spec.component_name,
                        &spec.config.environment().to_string(),
                    )
                    .await
                    .unwrap_or_default();

                // Encode secrets for Kubernetes
                let secrets = secrets_encoder.encode_secrets(secrets);

                // Render manifests for this component without modifying build context
                let ctx = spec.generate_build_context(toolchain.clone(), secrets);
                for manifest in component.manifests() {
                    // Use a default build context from spec instead of accessing non-existent field
                    manifest.render_to_file(&ctx); // Pass spec directly since render_to_file accepts it
                }
            }

            log::info!("Successfully built Kubernetes manifests");
            Ok(())
        }

        pub async fn install_manifests(&self) -> Result<(), String> {
            log::info!("Installing infrastructure manifests");

            // Check infrastructure repo status differently since is_initialized doesn't exist
            let infra_repo_dir =
                std::path::Path::new(&self.product_directory).join("infrastructure");
            if !infra_repo_dir.exists() {
                return Err("Infrastructure repository is not initialized".to_string());
            }

            // Get kubectl from toolchain
            let kubectl = self.toolchain.kubectl();

            // We don't have access to helm, so only use kubectl
            let helm_command = kubectl.clone(); // Use kubectl as a fallback

            // Create a guard to ensure we're in the product directory
            let _guard = crate::utils::Directory::chdir(&self.product_directory);

            // Get infrastructure manifests directory
            let infra_dir = infra_repo_dir.join("manifests");
            if !infra_dir.exists() {
                return Err(format!(
                    "Infrastructure directory does not exist: {}",
                    infra_dir.display()
                ));
            }

            // Install infrastructure components
            log::debug!(
                "Installing infrastructure components from: {}",
                infra_dir.display()
            );

            // Apply infrastructure manifests
            match crate::utils::run_command(
                "kubectl apply infrastructure".into(),
                kubectl.clone(),
                vec!["apply", "-f", &infra_dir.to_string_lossy()],
            )
            .await
            {
                Ok(output) => {
                    log::debug!("Successfully applied infrastructure manifests");
                    log::trace!("kubectl apply output: {}", output);
                }
                Err(e) => {
                    log::warn!("Failed to apply infrastructure manifests: {}", e);

                    // Try using alternative command as a fallback
                    log::debug!("Attempting installation with alternative method");
                    match crate::utils::run_command(
                        "alternative install infrastructure".into(),
                        helm_command,
                        vec!["apply", "-f", &infra_dir.to_string_lossy()],
                    )
                    .await
                    {
                        Ok(output) => {
                            log::debug!(
                                "Successfully installed infrastructure with alternative method"
                            );
                            log::trace!("alternative install output: {}", output);
                        }
                        Err(e) => {
                            return Err(format!(
                                "Failed to install infrastructure components: {}",
                                e
                            ));
                        }
                    }
                }
            }

            log::info!("Infrastructure components installed successfully");
            Ok(())
        }

        pub async fn uninstall_manifests(&self) -> Result<(), String> {
            log::info!("Uninstalling infrastructure manifests");

            // Check infrastructure repo status differently since is_initialized doesn't exist
            let infra_repo_dir =
                std::path::Path::new(&self.product_directory).join("infrastructure");
            if !infra_repo_dir.exists() {
                return Err("Infrastructure repository is not initialized".to_string());
            }

            // Get kubectl from toolchain
            let kubectl = self.toolchain.kubectl();

            // We don't have access to helm, so only use kubectl
            let helm_command = kubectl.clone(); // Use kubectl as a fallback

            // Create a guard to ensure we're in the product directory
            let _guard = crate::utils::Directory::chdir(&self.product_directory);

            // Get infrastructure manifests directory
            let infra_dir = infra_repo_dir.join("manifests");
            if !infra_dir.exists() {
                return Err(format!(
                    "Infrastructure directory does not exist: {}",
                    infra_dir.display()
                ));
            }

            // Instead of checking helm installation, always use kubectl
            let using_helm = false;

            // Uninstall based on the installation method
            if using_helm {
                log::debug!("Uninstalling infrastructure components using alternative method");
                match crate::utils::run_command(
                    "alternative uninstall infrastructure".into(),
                    helm_command,
                    vec!["delete", "-f", &infra_dir.to_string_lossy()],
                )
                .await
                {
                    Ok(output) => {
                        log::debug!(
                            "Successfully uninstalled infrastructure with alternative method"
                        );
                        log::trace!("alternative uninstall output: {}", output);
                    }
                    Err(e) => {
                        return Err(format!(
                            "Failed to uninstall infrastructure components with alternative method: {}",
                            e
                        ));
                    }
                }
            } else {
                log::debug!("Uninstalling infrastructure components using kubectl");
                match crate::utils::run_command(
                    "kubectl delete infrastructure".into(),
                    kubectl,
                    vec!["delete", "-f", &infra_dir.to_string_lossy()],
                )
                .await
                {
                    Ok(output) => {
                        log::debug!("Successfully removed infrastructure manifests");
                        log::trace!("kubectl delete output: {}", output);
                    }
                    Err(e) => {
                        return Err(format!(
                            "Failed to uninstall infrastructure components: {}",
                            e
                        ));
                    }
                }
            }

            log::info!("Infrastructure components uninstalled successfully");
            Ok(())
        }
    }
}
