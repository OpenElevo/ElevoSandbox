//! Sandbox service

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info, warn};
use uuid::Uuid;

use crate::domain::permission::PermissionLevel;
use crate::domain::sandbox::{CreateSandboxParams, Sandbox, SandboxState};
use crate::domain::UuidSimple;
use crate::error::{Error, Result};
use crate::infra::agent_pool::AgentConnPool;
use crate::infra::docker::{CreateContainerOpts, DockerManager};
use crate::infra::postgres::SandboxRepository;
use crate::infra::share_permission_repository::SharePermissionRepository;
use crate::infra::share_repository::ShareRepository;
use crate::Config;

/// Label key for identifying workspace sandboxes
const SANDBOX_LABEL_KEY: &str = "workspace.sandbox.id";

/// Sandbox service for managing sandbox lifecycle
pub struct SandboxService {
    repository: Arc<SandboxRepository>,
    share_repo: ShareRepository,
    permission_repo: SharePermissionRepository,
    docker: Arc<DockerManager>,
    agent_pool: Arc<AgentConnPool>,
    config: Arc<Config>,
}

impl SandboxService {
    /// Create a new sandbox service
    pub fn new(
        repository: Arc<SandboxRepository>,
        share_repo: ShareRepository,
        permission_repo: SharePermissionRepository,
        docker: Arc<DockerManager>,
        agent_pool: Arc<AgentConnPool>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            repository,
            share_repo,
            permission_repo,
            docker,
            agent_pool,
            config,
        }
    }

    /// Create a new sandbox
    pub async fn create(&self, params: CreateSandboxParams) -> Result<Sandbox> {
        let namespace_id = params.namespace_id;

        info!(
            "Creating sandbox with template: {:?}, namespace_id: {}, root_path: {}",
            params.template, namespace_id, params.root_path
        );

        // Verify namespace directory exists
        let namespace_dir = self.get_namespace_dir(namespace_id, &params.root_path);
        if !namespace_dir.exists() {
            error!("Namespace directory does not exist: {:?}", namespace_dir);
            return Err(Error::Internal("Namespace directory not found".to_string()));
        }

        // Resolve share mounts — check permissions and look up each share for host path
        let mut share_volumes = Vec::new();
        for mount in &params.mounts {
            let share = self.share_repo.get_share(mount.share_id).await?;

            // Permission check: caller must have at least Read on the share
            let perm_level = self.resolve_mount_permission(namespace_id, &share).await?;

            let host_path = self
                .config
                .get_share_host_path(&share.owner_tenant_id.simple_string(), &share.source_path);
            // Validate mount_path is absolute and doesn't conflict with /workspace
            if !mount.mount_path.starts_with('/') {
                return Err(Error::InvalidParameter(
                    "mount_path must be an absolute path".into(),
                ));
            }
            if mount.mount_path == "/workspace" || mount.mount_path.starts_with("/workspace/") {
                return Err(Error::InvalidParameter(
                    "mount_path cannot overlap with /workspace".into(),
                ));
            }

            // Apply :ro or :rw based on permission level
            let readonly = !perm_level.includes(&PermissionLevel::Write);
            share_volumes.push((host_path, mount.mount_path.clone(), readonly));
        }

        // Create database record (includes sandbox_mounts)
        let sandbox = self.repository.create(params.clone()).await?;
        let sandbox_id = sandbox.id;
        let sandbox_id_str = sandbox_id.simple_string();

        // Build container
        let template = params
            .template
            .unwrap_or_else(|| self.config.base_image.clone());
        let mut env = params.env.unwrap_or_default();

        env.insert("WORKSPACE_SANDBOX_ID".to_string(), sandbox_id_str.clone());
        env.insert(
            "WORKSPACE_NAMESPACE_ID".to_string(),
            namespace_id.simple_string(),
        );
        env.insert(
            "WORKSPACE_SERVER_ADDR".to_string(),
            self.config.agent_server_addr.clone(),
        );

        // Primary volume: namespace root_path → /workspace
        let volume_host_path = self
            .config
            .get_namespace_workspace_host_path(&namespace_id.simple_string(), &params.root_path);
        let mut volumes = HashMap::new();
        volumes.insert(volume_host_path, "/workspace".to_string());

        // Add share mount volumes with readonly flag
        for (host_path, mount_path, readonly) in share_volumes {
            let mount_spec = if readonly {
                format!("{}:ro", mount_path)
            } else {
                mount_path
            };
            volumes.insert(host_path, mount_spec);
        }

        let mut labels = HashMap::new();
        labels.insert(SANDBOX_LABEL_KEY.to_string(), sandbox_id_str.clone());
        labels.insert(
            "workspace.namespace.id".to_string(),
            namespace_id.simple_string(),
        );

        let network_mode = self
            .config
            .docker_network
            .clone()
            .or_else(|| Some("bridge".to_string()));

        let short_id: String = sandbox_id_str.chars().take(8).collect();
        let container_opts = CreateContainerOpts {
            name: format!("workspace-{}", short_id),
            image: template,
            env,
            volumes,
            working_dir: Some("/workspace".to_string()),
            cmd: None,
            labels,
            network_mode,
            memory_limit: None,
            cpu_quota: None,
            extra_hosts: self.config.sandbox_extra_hosts.clone(),
        };

        // Create and start container
        let container_id = match self.docker.create_container(container_opts).await {
            Ok(id) => id,
            Err(e) => {
                error!("Failed to create container: {}", e);
                self.repository
                    .update_state(sandbox_id, SandboxState::Error, Some(&e.to_string()))
                    .await?;
                return Err(e);
            }
        };

        self.repository
            .update_container_id(sandbox_id, &container_id)
            .await?;

        if let Err(e) = self.docker.start_container(&container_id).await {
            error!("Failed to start container: {}", e);
            let _ = self.docker.remove_container(&container_id, true).await;
            self.repository
                .update_state(sandbox_id, SandboxState::Error, Some(&e.to_string()))
                .await?;
            return Err(e);
        }

        // Wait for agent to connect
        let agent_timeout = Duration::from_secs(self.config.agent_timeout);
        match self
            .agent_pool
            .wait_for_connection(&sandbox_id_str, agent_timeout)
            .await
        {
            Ok(_) => {
                info!("Agent connected for sandbox: {}", sandbox_id);
            }
            Err(_e) => {
                warn!("Agent connection timeout for sandbox: {}", sandbox_id);
            }
        }
        self.repository
            .update_state(sandbox_id, SandboxState::Running, None)
            .await?;

        self.repository.get(sandbox_id).await
    }

    /// Get a sandbox by ID
    pub async fn get(&self, id: Uuid) -> Result<Sandbox> {
        self.repository.get(id).await
    }

    /// List all sandboxes with optional state filter
    pub async fn list(&self, state: Option<SandboxState>) -> Result<Vec<Sandbox>> {
        self.repository.list(state).await
    }

    /// List sandboxes by namespace ID with optional state filter
    pub async fn list_by_namespace(
        &self,
        namespace_id: Uuid,
        state: Option<SandboxState>,
    ) -> Result<Vec<Sandbox>> {
        self.repository.list_by_namespace(namespace_id, state).await
    }

    /// Delete a sandbox
    pub async fn delete(&self, id: Uuid, force: bool) -> Result<()> {
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

        let id_str = id.to_string();

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
        self.agent_pool.unregister(&id_str);

        // Delete from database
        self.repository.delete(id).await?;

        info!("Sandbox {} deleted", id);
        Ok(())
    }

    /// Check if agent is connected for a sandbox
    pub fn is_agent_connected(&self, id: &str) -> bool {
        self.agent_pool.is_connected(id)
    }

    /// Resolve the permission level for a share mount.
    ///
    /// Rules:
    /// - Share owner (namespace_id == share.owner_tenant_id) → Admin (full access)
    /// - Otherwise: look up explicit permission in DB
    /// - Public share with no explicit permission → Read
    /// - Private share with no permission → NOT_FOUND (hides existence)
    async fn resolve_mount_permission(
        &self,
        namespace_id: Uuid,
        share: &crate::domain::share::Share,
    ) -> Result<PermissionLevel> {
        // Owner of the share has implicit admin
        if namespace_id == share.owner_tenant_id {
            return Ok(PermissionLevel::Admin);
        }

        // Check explicit permission
        let perm = self
            .permission_repo
            .get_permission(share.id, namespace_id)
            .await?;

        match perm {
            Some(level) if level.includes(&PermissionLevel::Read) => Ok(level),
            Some(_) => Err(Error::PermissionDenied(
                "Insufficient permission to mount share".into(),
            )),
            None => {
                if share.visibility == crate::domain::share::Visibility::Public {
                    Ok(PermissionLevel::Read)
                } else {
                    // Return NOT_FOUND for private shares to hide their existence
                    Err(Error::WorkspaceNotFound(format!(
                        "Share {} not found",
                        share.id
                    )))
                }
            }
        }
    }

    /// Get namespace directory for a namespace + root_path
    fn get_namespace_dir(&self, namespace_id: Uuid, root_path: &str) -> PathBuf {
        let base = PathBuf::from(&self.config.workspace_dir)
            .join("namespaces")
            .join(namespace_id.simple_string());
        let trimmed = root_path.trim_start_matches('/');
        if trimmed.is_empty() {
            base
        } else {
            base.join(trimmed)
        }
    }

    /// Cleanup expired sandboxes
    pub async fn cleanup_expired(&self) -> Result<Vec<Uuid>> {
        let expired = self.repository.get_expired_sandboxes().await?;
        let mut deleted = Vec::new();

        for sandbox in expired {
            info!("Cleaning up expired sandbox: {}", sandbox.id);
            if let Err(e) = self.delete(sandbox.id, true).await {
                error!("Failed to delete expired sandbox {}: {}", sandbox.id, e);
            } else {
                deleted.push(sandbox.id);
            }
        }

        Ok(deleted)
    }

    /// Get sandbox statistics
    pub async fn get_stats(&self, id: Uuid) -> Result<SandboxStats> {
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

        let id_str = id.to_string();
        Ok(SandboxStats {
            sandbox_id: id_str.clone(),
            cpu_percent: stats.cpu_percent,
            memory_usage: stats.memory_usage,
            memory_limit: stats.memory_limit,
            network_rx_bytes: stats.network_rx_bytes,
            network_tx_bytes: stats.network_tx_bytes,
            agent_connected: self.agent_pool.is_connected(&id_str),
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
