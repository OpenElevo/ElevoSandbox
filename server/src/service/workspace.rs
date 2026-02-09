//! Workspace service

use std::collections::HashSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::{error, info, warn};

use crate::domain::workspace::{CreateWorkspaceParams, Workspace};
use crate::error::{Error, Result};
use crate::infra::nfs::NfsManager;
use crate::infra::storage::lease::{LeaseRenewalTask, WorkspaceLeaseManager};
use crate::infra::storage::StorageBackend;
use crate::infra::workspace_repository::WorkspaceRepository;

/// File information (HTTP API DTO)
#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub size: u64,
    pub modified_at: Option<DateTime<Utc>>,
}

/// Workspace service for managing workspace lifecycle and file operations
pub struct WorkspaceService {
    repository: Arc<WorkspaceRepository>,
    nfs_manager: Arc<NfsManager>,
    storage: Arc<dyn StorageBackend>,
    lease_manager: Arc<dyn WorkspaceLeaseManager>,
    holder_id: String,
    active_leases: Arc<tokio::sync::RwLock<HashSet<String>>>,
}

impl WorkspaceService {
    /// Create a new workspace service
    pub fn new(
        repository: Arc<WorkspaceRepository>,
        nfs_manager: Arc<NfsManager>,
        storage: Arc<dyn StorageBackend>,
        lease_manager: Arc<dyn WorkspaceLeaseManager>,
        holder_id: String,
    ) -> Self {
        let renewal_task = LeaseRenewalTask::new(lease_manager.clone(), holder_id.clone());
        let active_leases = renewal_task.active_leases();
        renewal_task.start();

        Self {
            repository,
            nfs_manager,
            storage,
            lease_manager,
            holder_id,
            active_leases,
        }
    }

    /// Check if this server instance holds the lease for a workspace.
    /// Used to ensure write operations only proceed if we have the lease.
    async fn ensure_lease_held(&self, workspace_id: &str) -> Result<()> {
        // Fast path: check our local active leases set
        {
            let leases = self.active_leases.read().await;
            if leases.contains(workspace_id) {
                return Ok(());
            }
        }

        // Slow path: check the database and try to acquire if needed
        match self.lease_manager.check(workspace_id).await {
            Ok(Some(lease)) if lease.holder_id == self.holder_id => {
                // We hold the lease but it's not in our active set (maybe server restarted)
                // Add it back to the active set for renewal
                let mut leases = self.active_leases.write().await;
                leases.insert(workspace_id.to_string());
                Ok(())
            }
            Ok(Some(lease)) => {
                // Another server holds the lease
                Err(Error::Internal(format!(
                    "Workspace '{}' is currently held by another server instance (holder: {})",
                    workspace_id, lease.holder_id
                )))
            }
            Ok(None) => {
                // No active lease - try to acquire one
                match self
                    .lease_manager
                    .acquire(workspace_id, &self.holder_id)
                    .await
                {
                    Ok(_) => {
                        let mut leases = self.active_leases.write().await;
                        leases.insert(workspace_id.to_string());
                        Ok(())
                    }
                    Err(e) => Err(Error::Internal(format!(
                        "Failed to acquire lease for workspace '{}': {}",
                        workspace_id, e
                    ))),
                }
            }
            Err(e) => Err(Error::Internal(format!(
                "Failed to check lease for workspace '{}': {}",
                workspace_id, e
            ))),
        }
    }

    /// Create a new workspace
    pub async fn create(&self, params: CreateWorkspaceParams) -> Result<Workspace> {
        info!("Creating workspace with name: {:?}", params.name);

        // Create database record first
        let workspace = self.repository.create(params).await?;
        let workspace_id = workspace.id.clone();

        // Acquire lease on the workspace
        if let Err(e) = self
            .lease_manager
            .acquire(&workspace_id, &self.holder_id)
            .await
        {
            error!("Failed to acquire lease for workspace {}: {}", workspace_id, e);
            let _ = self.repository.delete(&workspace_id).await;
            return Err(Error::Internal(format!(
                "Failed to acquire workspace lease: {}",
                e
            )));
        }

        // Add to active leases for background renewal
        {
            let mut leases = self.active_leases.write().await;
            leases.insert(workspace_id.clone());
        }

        // Create workspace directory via storage backend (async, non-blocking)
        if let Err(e) = self.storage.create_workspace_root(&workspace_id).await {
            error!("Failed to create workspace directory: {}", e);
            // Clean up lease and database record
            let _ = self.lease_manager.release(&workspace_id, &self.holder_id).await;
            self.remove_from_active_leases(&workspace_id).await;
            let _ = self.repository.delete(&workspace_id).await;
            return Err(Error::Internal(format!(
                "Failed to create workspace directory: {}",
                e
            )));
        }

        // Export workspace via NFS
        match self
            .nfs_manager
            .export(&workspace_id)
            .await
        {
            Ok(nfs_url) => {
                info!(
                    "NFS export created for workspace {}: {}",
                    workspace_id, nfs_url
                );
                // Update NFS URL in database
                if let Err(e) = self
                    .repository
                    .update_nfs_url(&workspace_id, &nfs_url)
                    .await
                {
                    warn!("Failed to update NFS URL in database: {}", e);
                }
            }
            Err(e) => {
                warn!(
                    "Failed to export NFS for workspace {}: {}",
                    workspace_id, e
                );
                // Non-fatal error, continue
            }
        }

        // Fetch and return updated workspace
        self.repository.get(&workspace_id).await
    }

    /// Get a workspace by ID
    pub async fn get(&self, id: &str) -> Result<Workspace> {
        self.repository.get(id).await
    }

    /// List all workspaces
    pub async fn list(&self) -> Result<Vec<Workspace>> {
        self.repository.list().await
    }

    /// Delete a workspace
    pub async fn delete(&self, id: &str) -> Result<()> {
        // Check if workspace has any sandboxes
        if self.repository.has_sandboxes(id).await? {
            return Err(Error::WorkspaceHasActiveSandboxes);
        }

        // Unexport NFS
        self.nfs_manager.unexport(id).await;

        // Remove workspace directory via storage backend (async, non-blocking)
        if let Err(e) = self.storage.delete_workspace_root(id).await {
            warn!("Failed to remove workspace directory: {}", e);
        }

        // Release lease and remove from active renewal list
        if let Err(e) = self.lease_manager.release(id, &self.holder_id).await {
            warn!("Failed to release lease for workspace {}: {}", id, e);
        }
        self.remove_from_active_leases(id).await;

        // Delete from database
        self.repository.delete(id).await?;

        info!("Workspace {} deleted", id);
        Ok(())
    }

    /// Remove a workspace ID from the active leases renewal set
    async fn remove_from_active_leases(&self, workspace_id: &str) {
        let mut leases = self.active_leases.write().await;
        leases.remove(workspace_id);
    }

    /// Get NFS URL for a workspace
    pub async fn get_nfs_url(&self, workspace_id: &str) -> Option<String> {
        self.nfs_manager.get_nfs_url(workspace_id).await
    }

    // ==================== File Operations ====================

    /// Read file content as bytes
    pub async fn read_file(&self, workspace_id: &str, path: &str) -> Result<Vec<u8>> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;
        Ok(self.storage.read_file(workspace_id, path).await?)
    }

    /// Read file content as string
    pub async fn read_file_string(&self, workspace_id: &str, path: &str) -> Result<String> {
        let bytes = self.read_file(workspace_id, path).await?;
        String::from_utf8(bytes).map_err(|e| Error::Internal(format!("Invalid UTF-8: {}", e)))
    }

    /// Write content to file
    pub async fn write_file(&self, workspace_id: &str, path: &str, content: &[u8]) -> Result<()> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;
        // Ensure we hold the lease for write operations
        self.ensure_lease_held(workspace_id).await?;
        Ok(self.storage.write_file(workspace_id, path, content).await?)
    }

    /// List directory contents
    pub async fn list_files(&self, workspace_id: &str, path: &str) -> Result<Vec<FileInfo>> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;
        let entries = self.storage.list_dir(workspace_id, path).await?;
        Ok(entries.into_iter().map(FileInfo::from).collect())
    }

    /// Create directory
    pub async fn mkdir(&self, workspace_id: &str, path: &str) -> Result<()> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;
        // Ensure we hold the lease for write operations
        self.ensure_lease_held(workspace_id).await?;
        Ok(self.storage.mkdir(workspace_id, path, true).await?)
    }

    /// Delete file or directory
    pub async fn delete_file(
        &self,
        workspace_id: &str,
        path: &str,
        recursive: bool,
    ) -> Result<()> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;
        // Ensure we hold the lease for write operations
        self.ensure_lease_held(workspace_id).await?;

        // Check what we're deleting to dispatch correctly
        let stat = self.storage.stat(workspace_id, path).await?;
        if stat.file_type == crate::infra::storage::FileType::Directory {
            Ok(self
                .storage
                .remove_dir(workspace_id, path, recursive)
                .await?)
        } else {
            Ok(self.storage.remove_file(workspace_id, path).await?)
        }
    }

    /// Move file or directory
    pub async fn move_file(&self, workspace_id: &str, src: &str, dst: &str) -> Result<()> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;
        // Ensure we hold the lease for write operations
        self.ensure_lease_held(workspace_id).await?;

        // Ensure destination parent directory exists (user-facing convenience).
        // The storage backend rename() follows POSIX semantics and does not
        // auto-create parent directories (NFS requires this strict behavior).
        if let Some(parent) = std::path::Path::new(dst).parent() {
            let parent_str = parent.to_string_lossy();
            if !parent_str.is_empty() && parent_str != "." {
                self.storage
                    .mkdir(workspace_id, &parent_str, true)
                    .await?;
            }
        }

        Ok(self.storage.rename(workspace_id, src, dst).await?)
    }

    /// Copy file or directory
    pub async fn copy_file(&self, workspace_id: &str, src: &str, dst: &str) -> Result<()> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;
        // Ensure we hold the lease for write operations
        self.ensure_lease_held(workspace_id).await?;
        Ok(self.storage.copy(workspace_id, src, dst).await?)
    }

    /// Get file info
    pub async fn get_file_info(&self, workspace_id: &str, path: &str) -> Result<FileInfo> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;
        let stat = self.storage.stat(workspace_id, path).await?;
        Ok(FileInfo::from(stat))
    }

    /// Check if file exists
    pub async fn exists(&self, workspace_id: &str, path: &str) -> Result<bool> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;
        Ok(self.storage.exists(workspace_id, path).await?)
    }
}
