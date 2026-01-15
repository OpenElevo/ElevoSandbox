//! Docker management layer

use std::collections::HashMap;

use bollard::{
    container::{
        Config, CreateContainerOptions, ListContainersOptions, LogOutput, LogsOptions,
        RemoveContainerOptions, StartContainerOptions, Stats, StatsOptions, StopContainerOptions,
    },
    image::CreateImageOptions,
    models::HostConfig,
    Docker,
};
use futures::StreamExt;
use tracing::{debug, info};

use crate::error::{Error, Result};

/// Container creation options
#[derive(Debug, Clone)]
pub struct CreateContainerOpts {
    /// Container name
    pub name: String,
    /// Image to use
    pub image: String,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Volumes to mount (host_path -> container_path)
    pub volumes: HashMap<String, String>,
    /// Working directory
    pub working_dir: Option<String>,
    /// Command to run
    pub cmd: Option<Vec<String>>,
    /// Labels
    pub labels: HashMap<String, String>,
    /// Network mode
    pub network_mode: Option<String>,
    /// Memory limit in bytes
    pub memory_limit: Option<i64>,
    /// CPU quota (100000 = 1 CPU)
    pub cpu_quota: Option<i64>,
    /// Extra hosts to add (e.g., "host.docker.internal:host-gateway")
    pub extra_hosts: Vec<String>,
}

/// Container statistics
#[derive(Debug, Clone)]
pub struct ContainerStats {
    pub cpu_percent: f64,
    pub memory_usage: u64,
    pub memory_limit: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
}

/// Docker manager for container operations
pub struct DockerManager {
    client: Docker,
    default_image: String,
}

impl DockerManager {
    /// Create a new Docker manager
    pub fn new(socket_path: Option<&str>, default_image: &str) -> Result<Self> {
        let client = match socket_path {
            Some(path) => {
                Docker::connect_with_socket(path, 120, bollard::API_DEFAULT_VERSION)
                    .map_err(|e| Error::DockerError(e.to_string()))?
            }
            None => {
                Docker::connect_with_local_defaults()
                    .map_err(|e| Error::DockerError(e.to_string()))?
            }
        };

        Ok(Self {
            client,
            default_image: default_image.to_string(),
        })
    }

    /// Get the Docker client
    pub fn client(&self) -> &Docker {
        &self.client
    }

    /// Check if Docker daemon is accessible
    pub async fn ping(&self) -> Result<()> {
        self.client
            .ping()
            .await
            .map_err(|e| Error::DockerError(format!("Docker ping failed: {}", e)))?;
        Ok(())
    }

    /// Pull an image if not present
    pub async fn ensure_image(&self, image: &str) -> Result<()> {
        let image_name = if image.contains(':') {
            image.to_string()
        } else {
            format!("{}:latest", image)
        };

        // Check if image exists
        match self.client.inspect_image(&image_name).await {
            Ok(_) => {
                debug!("Image {} already exists", image_name);
                return Ok(());
            }
            Err(_) => {
                info!("Pulling image {}...", image_name);
            }
        }

        // Pull the image
        let options = CreateImageOptions {
            from_image: image_name.clone(),
            ..Default::default()
        };

        let mut stream = self.client.create_image(Some(options), None, None);

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        debug!("Pull status: {}", status);
                    }
                }
                Err(e) => {
                    return Err(Error::DockerError(format!("Failed to pull image: {}", e)));
                }
            }
        }

        info!("Image {} pulled successfully", image_name);
        Ok(())
    }

    /// Create a container
    pub async fn create_container(&self, opts: CreateContainerOpts) -> Result<String> {
        let image = if opts.image.is_empty() {
            self.default_image.clone()
        } else {
            opts.image
        };

        // Ensure image exists
        self.ensure_image(&image).await?;

        // Build environment variables
        let env: Vec<String> = opts
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        // Build volume bindings
        let binds: Vec<String> = opts
            .volumes
            .iter()
            .map(|(host, container)| format!("{}:{}", host, container))
            .collect();

        // Build host config
        let host_config = HostConfig {
            binds: if binds.is_empty() { None } else { Some(binds) },
            network_mode: opts.network_mode,
            memory: opts.memory_limit,
            cpu_quota: opts.cpu_quota,
            extra_hosts: if opts.extra_hosts.is_empty() {
                None
            } else {
                Some(opts.extra_hosts)
            },
            ..Default::default()
        };

        // Build container config
        let config = Config {
            image: Some(image),
            env: if env.is_empty() { None } else { Some(env) },
            working_dir: opts.working_dir,
            cmd: opts.cmd,
            labels: if opts.labels.is_empty() {
                None
            } else {
                Some(opts.labels)
            },
            host_config: Some(host_config),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: opts.name.clone(),
            platform: None,
        };

        let response = self
            .client
            .create_container(Some(options), config)
            .await
            .map_err(|e| Error::DockerError(format!("Failed to create container: {}", e)))?;

        info!("Container {} created with ID: {}", opts.name, response.id);
        Ok(response.id)
    }

    /// Start a container
    pub async fn start_container(&self, id: &str) -> Result<()> {
        self.client
            .start_container(id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| Error::DockerError(format!("Failed to start container: {}", e)))?;

        info!("Container {} started", id);
        Ok(())
    }

    /// Stop a container
    pub async fn stop_container(&self, id: &str, timeout: Option<i64>) -> Result<()> {
        let options = StopContainerOptions {
            t: timeout.unwrap_or(10),
        };

        self.client
            .stop_container(id, Some(options))
            .await
            .map_err(|e| Error::DockerError(format!("Failed to stop container: {}", e)))?;

        info!("Container {} stopped", id);
        Ok(())
    }

    /// Remove a container
    pub async fn remove_container(&self, id: &str, force: bool) -> Result<()> {
        let options = RemoveContainerOptions {
            force,
            v: true, // Remove associated volumes
            ..Default::default()
        };

        self.client
            .remove_container(id, Some(options))
            .await
            .map_err(|e| Error::DockerError(format!("Failed to remove container: {}", e)))?;

        info!("Container {} removed", id);
        Ok(())
    }

    /// Get container stats
    pub async fn get_container_stats(&self, id: &str) -> Result<ContainerStats> {
        let options = StatsOptions {
            stream: false,
            one_shot: true,
        };

        let mut stream = self.client.stats(id, Some(options));

        if let Some(result) = stream.next().await {
            let stats: Stats = result.map_err(|e| Error::DockerError(e.to_string()))?;

            // Calculate CPU percentage
            let cpu_percent = calculate_cpu_percent(&stats);

            // Get memory stats
            let memory_usage = stats.memory_stats.usage.unwrap_or(0);
            let memory_limit = stats.memory_stats.limit.unwrap_or(0);

            // Get network stats
            let (rx_bytes, tx_bytes) = if let Some(networks) = stats.networks {
                networks
                    .values()
                    .fold((0u64, 0u64), |(rx, tx), net| {
                        (rx + net.rx_bytes, tx + net.tx_bytes)
                    })
            } else {
                (0, 0)
            };

            return Ok(ContainerStats {
                cpu_percent,
                memory_usage,
                memory_limit,
                network_rx_bytes: rx_bytes,
                network_tx_bytes: tx_bytes,
            });
        }

        Err(Error::DockerError("Failed to get container stats".to_string()))
    }

    /// Get container logs
    pub async fn get_container_logs(
        &self,
        id: &str,
        tail: Option<usize>,
        follow: bool,
    ) -> Result<impl futures::Stream<Item = Result<String>>> {
        let options = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: tail.map(|t| t.to_string()).unwrap_or_else(|| "all".to_string()),
            follow,
            ..Default::default()
        };

        let stream = self.client.logs(id, Some(options));

        Ok(stream.map(|result| {
            result
                .map(|output| match output {
                    LogOutput::StdOut { message } => {
                        String::from_utf8_lossy(&message).to_string()
                    }
                    LogOutput::StdErr { message } => {
                        String::from_utf8_lossy(&message).to_string()
                    }
                    _ => String::new(),
                })
                .map_err(|e| Error::DockerError(e.to_string()))
        }))
    }

    /// List containers with a specific label
    pub async fn list_containers_by_label(&self, label: &str, value: &str) -> Result<Vec<String>> {
        let mut filters = HashMap::new();
        filters.insert("label".to_string(), vec![format!("{}={}", label, value)]);

        let options = ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        };

        let containers = self
            .client
            .list_containers(Some(options))
            .await
            .map_err(|e| Error::DockerError(e.to_string()))?;

        Ok(containers
            .into_iter()
            .filter_map(|c| c.id)
            .collect())
    }

    /// Check if a container is running
    pub async fn is_container_running(&self, id: &str) -> Result<bool> {
        let info = self
            .client
            .inspect_container(id, None)
            .await
            .map_err(|e| Error::DockerError(e.to_string()))?;

        Ok(info
            .state
            .and_then(|s| s.running)
            .unwrap_or(false))
    }
}

/// Calculate CPU percentage from stats
fn calculate_cpu_percent(stats: &Stats) -> f64 {
    let cpu_stats = &stats.cpu_stats;
    let precpu_stats = &stats.precpu_stats;

    let cpu_delta = cpu_stats.cpu_usage.total_usage as f64
        - precpu_stats.cpu_usage.total_usage as f64;

    let system_delta = cpu_stats.system_cpu_usage.unwrap_or(0) as f64
        - precpu_stats.system_cpu_usage.unwrap_or(0) as f64;

    if system_delta > 0.0 && cpu_delta > 0.0 {
        let num_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;
        (cpu_delta / system_delta) * num_cpus * 100.0
    } else {
        0.0
    }
}
