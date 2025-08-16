use crate::docker::{BuildConfig, ContainerStatus, DockerClient, RunConfig};
use crate::error::{Error, Result};
use async_trait::async_trait;
use log::{debug, error, info};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Docker executor that implements DockerClient using command-line interface
#[derive(Debug, Clone)]
pub struct DockerExecutor {
    /// Whether to use sudo for docker commands
    use_sudo: bool,
    /// Default timeout for operations in seconds
    timeout: u64,
}

impl Default for DockerExecutor {
    fn default() -> Self {
        Self {
            use_sudo: false,
            timeout: 300,
        }
    }
}

impl DockerExecutor {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_sudo(mut self) -> Self {
        self.use_sudo = true;
        self
    }
    
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout = timeout;
        self
    }
    
    /// Execute a docker command with arguments
    async fn execute(&self, args: Vec<String>) -> Result<String> {
        let program = if self.use_sudo { "sudo" } else { "docker" };
        let mut cmd = Command::new(program);
        
        if self.use_sudo {
            cmd.arg("docker");
        }
        
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        debug!("Executing: {} {}", program, args.join(" "));
        
        let output = cmd.output().await.map_err(|e| {
            Error::Docker(format!("Failed to execute docker command: {}", e))
        })?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Check for common errors
            if stderr.contains("No such container") || stdout.contains("No such container") {
                return Err(Error::Docker("Container not found".to_string()));
            }
            if stderr.contains("No such image") || stdout.contains("No such image") {
                return Err(Error::Docker("Image not found".to_string()));
            }
            
            error!("Docker command failed: {}", stderr);
            return Err(Error::Docker(format!("Docker command failed: {}", stderr)));
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    
    /// Execute a docker command that streams output
    #[allow(dead_code)]
    async fn execute_streaming<F>(&self, args: Vec<String>, mut handler: F) -> Result<()>
    where
        F: FnMut(String) + Send + 'static,
    {
        let program = if self.use_sudo { "sudo" } else { "docker" };
        let mut cmd = Command::new(program);
        
        if self.use_sudo {
            cmd.arg("docker");
        }
        
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        debug!("Executing streaming: {} {}", program, args.join(" "));
        
        let mut child = cmd.spawn().map_err(|e| {
            Error::Docker(format!("Failed to spawn docker command: {}", e))
        })?;
        
        let stdout = child.stdout.take().ok_or_else(|| {
            Error::Docker("Failed to capture stdout".to_string())
        })?;
        
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            handler(line.clone());
            line.clear();
        }
        
        let status = child.wait().await.map_err(|e| {
            Error::Docker(format!("Failed to wait for docker command: {}", e))
        })?;
        
        if !status.success() {
            return Err(Error::Docker("Docker command failed".to_string()));
        }
        
        Ok(())
    }
}

#[async_trait]
impl DockerClient for DockerExecutor {
    // Network operations
    async fn create_network(&self, name: &str) -> Result<()> {
        let args = vec!["network".to_string(), "create".to_string(), name.to_string()];
        self.execute(args).await?;
        info!("Created Docker network: {}", name);
        Ok(())
    }
    
    async fn delete_network(&self, name: &str) -> Result<()> {
        let args = vec!["network".to_string(), "rm".to_string(), name.to_string()];
        self.execute(args).await?;
        info!("Deleted Docker network: {}", name);
        Ok(())
    }
    
    async fn network_exists(&self, name: &str) -> Result<bool> {
        let args = vec!["network".to_string(), "ls".to_string(), "--format".to_string(), "{{.Name}}".to_string()];
        let output = self.execute(args).await?;
        Ok(output.lines().any(|line| line.trim() == name))
    }
    
    // Image operations
    async fn pull_image(&self, image: &str) -> Result<()> {
        let args = vec!["pull".to_string(), image.to_string()];
        self.execute(args).await?;
        info!("Pulled Docker image: {}", image);
        Ok(())
    }
    
    async fn build_image(&self, config: BuildConfig) -> Result<()> {
        let mut args = vec!["build".to_string()];
        
        args.push("-t".to_string());
        args.push(config.tag.clone());
        
        args.push("-f".to_string());
        args.push(config.dockerfile.clone());
        
        if let Some(platform) = config.platform {
            args.push("--platform".to_string());
            args.push(platform);
        }
        
        if let Some(target) = config.target {
            args.push("--target".to_string());
            args.push(target);
        }
        
        for (key, value) in config.build_args {
            args.push("--build-arg".to_string());
            args.push(format!("{}={}", key, value));
        }
        
        args.push(config.context);
        
        self.execute(args).await?;
        info!("Built Docker image: {}", config.tag);
        Ok(())
    }
    
    async fn image_exists(&self, image: &str) -> Result<bool> {
        let args = vec!["images".to_string(), "-q".to_string(), image.to_string()];
        let output = self.execute(args).await?;
        Ok(!output.trim().is_empty())
    }
    
    async fn remove_image(&self, image: &str) -> Result<()> {
        let args = vec!["rmi".to_string(), image.to_string()];
        self.execute(args).await?;
        info!("Removed Docker image: {}", image);
        Ok(())
    }
    
    // Container operations
    async fn run_container(&self, config: RunConfig) -> Result<String> {
        let mut args = vec!["run".to_string()];
        
        if config.detach {
            args.push("-d".to_string());
        }
        
        if config.remove {
            args.push("--rm".to_string());
        }
        
        if config.privileged {
            args.push("--privileged".to_string());
        }
        
        args.push("--name".to_string());
        args.push(config.name.clone());
        
        if let Some(network) = config.network {
            args.push("--network".to_string());
            args.push(network);
        }
        
        if let Some(working_dir) = config.working_dir {
            args.push("-w".to_string());
            args.push(working_dir);
        }
        
        for env_var in config.env_vars {
            args.push("-e".to_string());
            args.push(env_var);
        }
        
        for port in config.ports {
            args.push("-p".to_string());
            args.push(port);
        }
        
        for volume in config.volumes {
            args.push("-v".to_string());
            args.push(volume);
        }
        
        args.push(config.image.clone());
        
        if let Some(command) = config.command {
            args.extend(command);
        }
        
        let output = self.execute(args).await?;
        let container_id = output.trim().to_string();
        info!("Started container {} with ID: {}", config.name, container_id);
        Ok(container_id)
    }
    
    async fn stop_container(&self, container_id: &str) -> Result<()> {
        let args = vec!["stop".to_string(), container_id.to_string()];
        self.execute(args).await?;
        info!("Stopped container: {}", container_id);
        Ok(())
    }
    
    async fn remove_container(&self, container_id: &str) -> Result<()> {
        let args = vec!["rm".to_string(), "-f".to_string(), container_id.to_string()];
        self.execute(args).await?;
        info!("Removed container: {}", container_id);
        Ok(())
    }
    
    async fn container_status(&self, container_id: &str) -> Result<ContainerStatus> {
        let args = vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{.State.Status}}".to_string(),
            container_id.to_string(),
        ];
        
        match self.execute(args).await {
            Ok(output) => {
                let status = output.trim();
                match status {
                    "running" => Ok(ContainerStatus::Running),
                    "exited" => {
                        // Get exit code
                        if let Ok(Some(code)) = self.get_container_exit_code(container_id).await {
                            Ok(ContainerStatus::Exited(code))
                        } else {
                            Ok(ContainerStatus::Stopped)
                        }
                    }
                    _ => Ok(ContainerStatus::Unknown),
                }
            }
            Err(_) => Ok(ContainerStatus::Unknown),
        }
    }
    
    async fn container_logs(&self, container_id: &str, follow: bool, since: Option<&str>) -> Result<String> {
        let mut args = vec!["logs".to_string()];
        
        if follow {
            args.push("--follow".to_string());
        }
        
        if let Some(since) = since {
            args.push("--since".to_string());
            args.push(since.to_string());
        }
        
        args.push(container_id.to_string());
        
        self.execute(args).await
    }
    
    async fn exec_in_container(&self, container_id: &str, command: &[&str]) -> Result<String> {
        let mut args = vec!["exec".to_string(), container_id.to_string()];
        args.extend(command.iter().map(|s| s.to_string()));
        
        self.execute(args).await
    }
    
    async fn get_container_by_name(&self, name: &str) -> Result<Option<String>> {
        let args = vec![
            "ps".to_string(),
            "-a".to_string(),
            "--filter".to_string(),
            format!("name={}", name),
            "--format".to_string(),
            "{{.ID}}".to_string(),
        ];
        
        let output = self.execute(args).await?;
        let id = output.trim();
        
        if id.is_empty() {
            Ok(None)
        } else {
            Ok(Some(id.to_string()))
        }
    }
    
    async fn list_containers(&self, all: bool) -> Result<Vec<String>> {
        let mut args = vec!["ps".to_string()];
        
        if all {
            args.push("-a".to_string());
        }
        
        args.push("--format".to_string());
        args.push("{{.ID}}".to_string());
        
        let output = self.execute(args).await?;
        Ok(output.lines().map(|s| s.to_string()).collect())
    }
    
    async fn inspect_container(&self, container_id: &str) -> Result<String> {
        let args = vec!["inspect".to_string(), container_id.to_string()];
        self.execute(args).await
    }
    
    async fn get_container_exit_code(&self, container_id: &str) -> Result<Option<i32>> {
        let args = vec![
            "inspect".to_string(),
            "--format".to_string(),
            "{{.State.ExitCode}}".to_string(),
            container_id.to_string(),
        ];
        
        match self.execute(args).await {
            Ok(output) => {
                let code = output.trim().parse::<i32>().ok();
                Ok(code)
            }
            Err(_) => Ok(None),
        }
    }
    
    async fn wait_for_container(&self, container_id: &str) -> Result<i32> {
        let args = vec!["wait".to_string(), container_id.to_string()];
        let output = self.execute(args).await?;
        
        output.trim().parse::<i32>()
            .map_err(|e| Error::Docker(format!("Failed to parse exit code: {}", e)))
    }
}