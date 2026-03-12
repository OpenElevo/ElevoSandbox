//! Namespace physical directory management
//!
//! Handles creation, deletion (soft-delete to trash), and background cleanup
//! of namespace directories on the filesystem.

use std::path::PathBuf;

use chrono::Utc;
use tokio::fs;
use tracing::{error, info, warn};

use crate::config::Config;

/// Manages physical namespace directories on the filesystem
#[derive(Clone)]
pub struct NamespaceService {
    /// Root directory for all namespaces: <workspace_dir>/namespaces/
    namespaces_dir: PathBuf,
    /// Trash directory: <workspace_dir>/namespaces/.trash/
    trash_dir: PathBuf,
    /// Retention period for trashed directories (days)
    trash_retention_days: u64,
}

impl NamespaceService {
    pub fn new(config: &Config) -> Self {
        let namespaces_dir = PathBuf::from(&config.workspace_dir).join("namespaces");
        let trash_dir = namespaces_dir.join(".trash");
        Self {
            namespaces_dir,
            trash_dir,
            trash_retention_days: config.namespace_trash_retention_days,
        }
    }

    /// Ensure base directories exist
    pub async fn init(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.namespaces_dir).await?;
        fs::create_dir_all(&self.trash_dir).await?;
        info!(
            "Namespace directories initialized: {}",
            self.namespaces_dir.display()
        );
        Ok(())
    }

    /// Create a namespace directory for a tenant
    pub async fn create_namespace_dir(&self, tenant_id: &str) -> std::io::Result<PathBuf> {
        let dir = self.namespaces_dir.join(tenant_id);
        fs::create_dir_all(&dir).await?;
        info!("Created namespace directory: {}", dir.display());
        Ok(dir)
    }

    /// Soft-delete a namespace directory by moving it to trash
    pub async fn delete_namespace_dir(&self, tenant_id: &str) -> std::io::Result<()> {
        let dir = self.namespaces_dir.join(tenant_id);
        if !dir.exists() {
            warn!(
                "Namespace directory does not exist, skipping delete: {}",
                dir.display()
            );
            return Ok(());
        }

        let timestamp = Utc::now().format("%Y%m%d%H%M%S");
        let trash_name = format!("{}_{}", tenant_id, timestamp);
        let trash_path = self.trash_dir.join(&trash_name);

        fs::rename(&dir, &trash_path).await?;
        info!(
            "Moved namespace directory to trash: {} -> {}",
            dir.display(),
            trash_path.display()
        );
        Ok(())
    }

    /// Get the path for a namespace directory
    pub fn namespace_path(&self, tenant_id: &str) -> PathBuf {
        self.namespaces_dir.join(tenant_id)
    }

    /// Get a sub-path within a namespace
    pub fn namespace_subpath(&self, tenant_id: &str, subpath: &str) -> PathBuf {
        let base = self.namespaces_dir.join(tenant_id);
        let trimmed = subpath.trim_start_matches('/');
        if trimmed.is_empty() {
            base
        } else {
            base.join(trimmed)
        }
    }

    /// Check if a path exists within a namespace
    pub async fn path_exists(&self, tenant_id: &str, subpath: &str) -> bool {
        self.namespace_subpath(tenant_id, subpath).exists()
    }

    /// Start the background trash cleanup task
    pub fn start_trash_cleanup(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(3600), // every hour
            );
            loop {
                interval.tick().await;
                if let Err(e) = self.cleanup_trash().await {
                    error!("Trash cleanup error: {}", e);
                }
            }
        })
    }

    /// Clean up expired trash directories
    async fn cleanup_trash(&self) -> std::io::Result<()> {
        let retention_secs = self.trash_retention_days * 86400;
        let mut entries = fs::read_dir(&self.trash_dir).await?;
        let mut cleaned = 0u32;

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if !metadata.is_dir() {
                continue;
            }

            let modified = metadata.modified()?;
            let age = modified
                .elapsed()
                .unwrap_or(std::time::Duration::ZERO);

            if age.as_secs() >= retention_secs {
                let path = entry.path();
                info!("Removing expired trash directory: {}", path.display());
                if let Err(e) = fs::remove_dir_all(&path).await {
                    error!(
                        "Failed to remove trash directory {}: {}",
                        path.display(),
                        e
                    );
                } else {
                    cleaned += 1;
                }
            }
        }

        if cleaned > 0 {
            info!("Trash cleanup: removed {} expired directories", cleaned);
        }
        Ok(())
    }
}
