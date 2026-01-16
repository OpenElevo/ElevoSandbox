//! Workspace service

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::fs;
use tracing::{error, info, warn};

use crate::domain::workspace::{CreateWorkspaceParams, Workspace};
use crate::error::{Error, Result};
use crate::infra::nfs::NfsManager;
use crate::infra::workspace_repository::WorkspaceRepository;
use crate::Config;

/// File information
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
    config: Arc<Config>,
}

impl WorkspaceService {
    /// Create a new workspace service
    pub fn new(
        repository: Arc<WorkspaceRepository>,
        nfs_manager: Arc<NfsManager>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            repository,
            nfs_manager,
            config,
        }
    }

    /// Create a new workspace
    pub async fn create(&self, params: CreateWorkspaceParams) -> Result<Workspace> {
        info!("Creating workspace with name: {:?}", params.name);

        // Create database record first
        let workspace = self.repository.create(params).await?;
        let workspace_id = workspace.id.clone();

        // Create workspace directory
        let workspace_dir = self.get_workspace_dir(&workspace_id);
        if let Err(e) = std::fs::create_dir_all(&workspace_dir) {
            error!("Failed to create workspace directory: {}", e);
            // Clean up database record
            let _ = self.repository.delete(&workspace_id).await;
            return Err(Error::Internal(format!(
                "Failed to create workspace directory: {}",
                e
            )));
        }

        // Export workspace via NFS
        match self.nfs_manager.export(&workspace_id, &workspace_dir).await {
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
                warn!("Failed to export NFS for workspace {}: {}", workspace_id, e);
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

        // Remove workspace directory
        let workspace_dir = self.get_workspace_dir(id);
        if workspace_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&workspace_dir) {
                warn!("Failed to remove workspace directory: {}", e);
            }
        }

        // Delete from database
        self.repository.delete(id).await?;

        info!("Workspace {} deleted", id);
        Ok(())
    }

    /// Get workspace directory path
    pub fn get_workspace_dir(&self, workspace_id: &str) -> PathBuf {
        PathBuf::from(&self.config.workspace_dir).join(workspace_id)
    }

    /// Get NFS URL for a workspace
    pub async fn get_nfs_url(&self, workspace_id: &str) -> Option<String> {
        self.nfs_manager.get_nfs_url(workspace_id).await
    }

    // ==================== File Operations ====================

    /// Validate and resolve file path within workspace
    fn resolve_path(&self, workspace_id: &str, path: &str) -> Result<PathBuf> {
        let workspace_dir = self.get_workspace_dir(workspace_id);

        // Normalize the path and prevent directory traversal
        let normalized = Path::new(path)
            .components()
            .filter(|c| !matches!(c, std::path::Component::ParentDir))
            .collect::<PathBuf>();

        let full_path = workspace_dir.join(&normalized);

        // Ensure the resolved path is within the workspace directory
        if !full_path.starts_with(&workspace_dir) {
            return Err(Error::PathNotAllowed(path.to_string()));
        }

        Ok(full_path)
    }

    /// Read file content as bytes
    pub async fn read_file(&self, workspace_id: &str, path: &str) -> Result<Vec<u8>> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;

        let full_path = self.resolve_path(workspace_id, path)?;

        if !full_path.exists() {
            return Err(Error::FileNotFound(path.to_string()));
        }

        if full_path.is_dir() {
            return Err(Error::NotADirectory(path.to_string()));
        }

        fs::read(&full_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to read file: {}", e)))
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

        let full_path = self.resolve_path(workspace_id, path)?;

        // Create parent directories if they don't exist
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Internal(format!("Failed to create directories: {}", e)))?;
        }

        fs::write(&full_path, content)
            .await
            .map_err(|e| Error::Internal(format!("Failed to write file: {}", e)))
    }

    /// List directory contents
    pub async fn list_files(&self, workspace_id: &str, path: &str) -> Result<Vec<FileInfo>> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;

        let full_path = self.resolve_path(workspace_id, path)?;

        if !full_path.exists() {
            return Err(Error::FileNotFound(path.to_string()));
        }

        if !full_path.is_dir() {
            return Err(Error::NotADirectory(path.to_string()));
        }

        let mut entries = fs::read_dir(&full_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to read directory: {}", e)))?;

        let mut files = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| Error::Internal(format!("Failed to read entry: {}", e)))?
        {
            let metadata = entry
                .metadata()
                .await
                .map_err(|e| Error::Internal(format!("Failed to get metadata: {}", e)))?;

            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = Path::new(path).join(&name);

            let modified_at = metadata.modified().ok().map(|t| DateTime::<Utc>::from(t));

            files.push(FileInfo {
                name,
                path: entry_path.to_string_lossy().to_string(),
                file_type: if metadata.is_dir() {
                    "directory".to_string()
                } else {
                    "file".to_string()
                },
                size: metadata.len(),
                modified_at,
            });
        }

        // Sort by name
        files.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(files)
    }

    /// Create directory
    pub async fn mkdir(&self, workspace_id: &str, path: &str) -> Result<()> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;

        let full_path = self.resolve_path(workspace_id, path)?;

        fs::create_dir_all(&full_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to create directory: {}", e)))
    }

    /// Delete file or directory
    pub async fn delete_file(&self, workspace_id: &str, path: &str, recursive: bool) -> Result<()> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;

        let full_path = self.resolve_path(workspace_id, path)?;

        if !full_path.exists() {
            return Err(Error::FileNotFound(path.to_string()));
        }

        if full_path.is_dir() {
            if recursive {
                fs::remove_dir_all(&full_path)
                    .await
                    .map_err(|e| Error::Internal(format!("Failed to remove directory: {}", e)))?;
            } else {
                fs::remove_dir(&full_path).await.map_err(|e| {
                    if e.kind() == std::io::ErrorKind::Other {
                        Error::DirectoryNotEmpty(path.to_string())
                    } else {
                        Error::Internal(format!("Failed to remove directory: {}", e))
                    }
                })?;
            }
        } else {
            fs::remove_file(&full_path)
                .await
                .map_err(|e| Error::Internal(format!("Failed to remove file: {}", e)))?;
        }

        Ok(())
    }

    /// Move file or directory
    pub async fn move_file(&self, workspace_id: &str, src: &str, dst: &str) -> Result<()> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;

        let src_path = self.resolve_path(workspace_id, src)?;
        let dst_path = self.resolve_path(workspace_id, dst)?;

        if !src_path.exists() {
            return Err(Error::FileNotFound(src.to_string()));
        }

        // Create parent directories for destination
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Internal(format!("Failed to create directories: {}", e)))?;
        }

        fs::rename(&src_path, &dst_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to move file: {}", e)))
    }

    /// Copy file or directory
    pub async fn copy_file(&self, workspace_id: &str, src: &str, dst: &str) -> Result<()> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;

        let src_path = self.resolve_path(workspace_id, src)?;
        let dst_path = self.resolve_path(workspace_id, dst)?;

        if !src_path.exists() {
            return Err(Error::FileNotFound(src.to_string()));
        }

        // Create parent directories for destination
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Internal(format!("Failed to create directories: {}", e)))?;
        }

        if src_path.is_dir() {
            // Recursive copy for directories
            copy_dir_recursive(&src_path, &dst_path).await?;
        } else {
            fs::copy(&src_path, &dst_path)
                .await
                .map_err(|e| Error::Internal(format!("Failed to copy file: {}", e)))?;
        }

        Ok(())
    }

    /// Get file info
    pub async fn get_file_info(&self, workspace_id: &str, path: &str) -> Result<FileInfo> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;

        let full_path = self.resolve_path(workspace_id, path)?;

        if !full_path.exists() {
            return Err(Error::FileNotFound(path.to_string()));
        }

        let metadata = fs::metadata(&full_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to get metadata: {}", e)))?;

        let name = full_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let modified_at = metadata.modified().ok().map(|t| DateTime::<Utc>::from(t));

        Ok(FileInfo {
            name,
            path: path.to_string(),
            file_type: if metadata.is_dir() {
                "directory".to_string()
            } else {
                "file".to_string()
            },
            size: metadata.len(),
            modified_at,
        })
    }

    /// Check if file exists
    pub async fn exists(&self, workspace_id: &str, path: &str) -> Result<bool> {
        // Verify workspace exists
        self.repository.get(workspace_id).await?;

        let full_path = self.resolve_path(workspace_id, path)?;
        Ok(full_path.exists())
    }
}

/// Recursively copy directory
async fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .await
        .map_err(|e| Error::Internal(format!("Failed to create directory: {}", e)))?;

    let mut entries = fs::read_dir(src)
        .await
        .map_err(|e| Error::Internal(format!("Failed to read directory: {}", e)))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| Error::Internal(format!("Failed to read entry: {}", e)))?
    {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dst_path)).await?;
        } else {
            fs::copy(&src_path, &dst_path)
                .await
                .map_err(|e| Error::Internal(format!("Failed to copy file: {}", e)))?;
        }
    }

    Ok(())
}
