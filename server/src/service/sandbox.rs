//! Sandbox service

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info, warn};

use crate::domain::sandbox::{CreateSandboxParams, Sandbox, SandboxState};
use crate::error::{Error, Result};
use crate::infra::agent_pool::AgentConnPool;
use crate::infra::docker::{CreateContainerOpts, DockerManager};
use crate::infra::sqlite::SandboxRepository;
use crate::infra::workspace_repository::WorkspaceRepository;
use crate::Config;

/// Label key for identifying workspace sandboxes
const SANDBOX_LABEL_KEY: &str = "workspace.sandbox.id";

/// Sandbox service for managing sandbox lifecycle
pub struct SandboxService {
    repository: Arc<SandboxRepository>,
    workspace_repo: Arc<WorkspaceRepository>,
    docker: Arc<DockerManager>,
    agent_pool: Arc<AgentConnPool>,
    config: Arc<Config>,
}

impl SandboxService {
    /// Create a new sandbox service
    pub fn new(
        repository: Arc<SandboxRepository>,
        workspace_repo: Arc<WorkspaceRepository>,
        docker: Arc<DockerManager>,
        agent_pool: Arc<AgentConnPool>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            repository,
            workspace_repo,
            docker,
            agent_pool,
            config,
        }
    }

    /// Create a new sandbox
    pub async fn create(&self, params: CreateSandboxParams) -> Result<Sandbox> {
        info!(
            "Creating sandbox with template: {:?}, workspace_id: {}",
            params.template, params.workspace_id
        );

        // Verify workspace exists
        let workspace = self.workspace_repo.get(&params.workspace_id).await?;

        // Create database record first
        let sandbox = self.repository.create(params.clone()).await?;
        let sandbox_id = sandbox.id.clone();

        // Use workspace directory (already created by WorkspaceService)
        let workspace_dir = self.get_workspace_dir(&workspace.id);
        if !workspace_dir.exists() {
            error!("Workspace directory does not exist: {:?}", workspace_dir);
            self.repository
                .update_state(
                    &sandbox_id,
                    SandboxState::Error,
                    Some("Workspace directory not found"),
                )
                .await?;
            return Err(Error::Internal("Workspace directory not found".to_string()));
        }

        // Build container options
        let template = params
            .template
            .unwrap_or_else(|| self.config.base_image.clone());
        let mut env = params.env.unwrap_or_default();

        // Add sandbox ID and server address to environment
        env.insert("WORKSPACE_SANDBOX_ID".to_string(), sandbox_id.clone());
        env.insert("WORKSPACE_WORKSPACE_ID".to_string(), workspace.id.clone());
        env.insert(
            "WORKSPACE_SERVER_ADDR".to_string(),
            self.config.agent_server_addr.clone(),
        );

        // Use host path for volume mounting if configured (for Docker-in-Docker scenarios)
        let volume_host_path = self.config.get_sandbox_workspace_host_path(&workspace.id);
        let mut volumes = HashMap::new();
        volumes.insert(volume_host_path, "/workspace".to_string());

        let mut labels = HashMap::new();
        labels.insert(SANDBOX_LABEL_KEY.to_string(), sandbox_id.clone());
        labels.insert("workspace.workspace.id".to_string(), workspace.id.clone());

        // Determine network mode
        let network_mode = self
            .config
            .docker_network
            .clone()
            .or_else(|| Some("bridge".to_string()));

        let container_opts = CreateContainerOpts {
            name: format!("workspace-{}", &sandbox_id[..8]),
            image: template,
            env,
            volumes,
            working_dir: Some("/workspace".to_string()),
            cmd: None, // Let the image decide the entrypoint
            labels,
            network_mode,
            memory_limit: None,
            cpu_quota: None,
            extra_hosts: self.config.sandbox_extra_hosts.clone(),
        };

        // Create container
        let container_id = match self.docker.create_container(container_opts).await {
            Ok(id) => id,
            Err(e) => {
                error!("Failed to create container: {}", e);
                self.repository
                    .update_state(&sandbox_id, SandboxState::Error, Some(&e.to_string()))
                    .await?;
                return Err(e);
            }
        };

        // Update container ID in database
        self.repository
            .update_container_id(&sandbox_id, &container_id)
            .await?;

        // Start container
        if let Err(e) = self.docker.start_container(&container_id).await {
            error!("Failed to start container: {}", e);
            // Try to remove the container
            let _ = self.docker.remove_container(&container_id, true).await;
            self.repository
                .update_state(&sandbox_id, SandboxState::Error, Some(&e.to_string()))
                .await?;
            return Err(e);
        }

        // Wait for agent to connect
        let agent_timeout = Duration::from_secs(self.config.agent_timeout);
        match self
            .agent_pool
            .wait_for_connection(&sandbox_id, agent_timeout)
            .await
        {
            Ok(_) => {
                info!("Agent connected for sandbox: {}", sandbox_id);
                self.repository
                    .update_state(&sandbox_id, SandboxState::Running, None)
                    .await?;
            }
            Err(_e) => {
                warn!("Agent connection timeout for sandbox: {}", sandbox_id);
                // Container is running but agent didn't connect
                // We'll still mark it as running, agent might connect later
                self.repository
                    .update_state(&sandbox_id, SandboxState::Running, None)
                    .await?;
            }
        }

        // Fetch and return updated sandbox
        self.repository.get(&sandbox_id).await
    }

    /// Get a sandbox by ID
    pub async fn get(&self, id: &str) -> Result<Sandbox> {
        self.repository.get(id).await
    }

    /// List all sandboxes with optional state filter
    pub async fn list(&self, state: Option<SandboxState>) -> Result<Vec<Sandbox>> {
        self.repository.list(state).await
    }

    /// Delete a sandbox
    pub async fn delete(&self, id: &str, force: bool) -> Result<()> {
        let sandbox = self.repository.get(id).await?;

        // Check state
        if !force && sandbox.state == SandboxState::Running {
            return Err(Error::InvalidSandboxState {
                expected: "stopped".to_string(),
                actual: sandbox.state.as_str().to_string(),
            });
        }

        // Update state to stopping
        self.repository
            .update_state(id, SandboxState::Stopping, None)
            .await?;

        // Stop and remove container if exists
        if let Some(container_id) = &sandbox.container_id {
            // Try to stop first if not forcing
            if !force {
                if let Err(e) = self.docker.stop_container(container_id, Some(10)).await {
                    warn!("Failed to stop container gracefully: {}", e);
                }
            }

            // Remove container
            if let Err(e) = self.docker.remove_container(container_id, force).await {
                error!("Failed to remove container: {}", e);
                // Continue with deletion anyway
            }
        }

        // Unregister agent connection
        self.agent_pool.unregister(id);

        // NOTE: We do NOT delete the workspace directory here.
        // The workspace is managed separately by WorkspaceService.

        // Delete from database
        self.repository.delete(id).await?;

        info!("Sandbox {} deleted", id);
        Ok(())
    }

    /// Check if agent is connected for a sandbox
    pub fn is_agent_connected(&self, id: &str) -> bool {
        self.agent_pool.is_connected(id)
    }

    /// Get workspace directory for a workspace
    fn get_workspace_dir(&self, workspace_id: &str) -> PathBuf {
        PathBuf::from(&self.config.workspace_dir).join(workspace_id)
    }

    /// Cleanup expired sandboxes
    pub async fn cleanup_expired(&self) -> Result<Vec<String>> {
        let expired = self.repository.get_expired_sandboxes().await?;
        let mut deleted = Vec::new();

        for sandbox in expired {
            info!("Cleaning up expired sandbox: {}", sandbox.id);
            if let Err(e) = self.delete(&sandbox.id, true).await {
                error!("Failed to delete expired sandbox {}: {}", sandbox.id, e);
            } else {
                deleted.push(sandbox.id);
            }
        }

        Ok(deleted)
    }

    /// Get sandbox statistics
    pub async fn get_stats(&self, id: &str) -> Result<SandboxStats> {
        let sandbox = self.repository.get(id).await?;

        if sandbox.state != SandboxState::Running {
            return Err(Error::InvalidSandboxState {
                expected: "running".to_string(),
                actual: sandbox.state.as_str().to_string(),
            });
        }

        let container_id = sandbox
            .container_id
            .ok_or_else(|| Error::Internal("No container ID".to_string()))?;

        let stats = self.docker.get_container_stats(&container_id).await?;

        Ok(SandboxStats {
            sandbox_id: id.to_string(),
            cpu_percent: stats.cpu_percent,
            memory_usage: stats.memory_usage,
            memory_limit: stats.memory_limit,
            network_rx_bytes: stats.network_rx_bytes,
            network_tx_bytes: stats.network_tx_bytes,
            agent_connected: self.agent_pool.is_connected(id),
        })
    }
}

/// Sandbox statistics
#[derive(Debug, Clone)]
pub struct SandboxStats {
    pub sandbox_id: String,
    pub cpu_percent: f64,
    pub memory_usage: u64,
    pub memory_limit: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub agent_connected: bool,
}
