//! Namespace physical directory management
//!
//! Handles creation, deletion (soft-delete to trash), and background cleanup
//! of namespace directories on the filesystem.

use std::path::PathBuf;
use std::time::Duration;

use chrono::{NaiveDateTime, Utc};
use tokio::fs;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::domain::UuidSimple;

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
    pub async fn create_namespace_dir(&self, tenant_id: Uuid) -> std::io::Result<PathBuf> {
        let dir = self.namespaces_dir.join(tenant_id.simple_string());
        fs::create_dir_all(&dir).await?;
        info!("Created namespace directory: {}", dir.display());
        Ok(dir)
    }

    /// Soft-delete a namespace directory by moving it to trash
    pub async fn delete_namespace_dir(&self, tenant_id: Uuid) -> std::io::Result<()> {
        let dir = self.namespaces_dir.join(tenant_id.simple_string());
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
    pub fn namespace_path(&self, tenant_id: Uuid) -> PathBuf {
        self.namespaces_dir.join(tenant_id.simple_string())
    }

    /// Get a sub-path within a namespace
    pub fn namespace_subpath(&self, tenant_id: Uuid, subpath: &str) -> PathBuf {
        let base = self.namespaces_dir.join(tenant_id.simple_string());
        let trimmed = subpath.trim_start_matches('/');
        if trimmed.is_empty() {
            base
        } else {
            base.join(trimmed)
        }
    }

    /// Check if a path exists within a namespace
    pub async fn path_exists(&self, tenant_id: Uuid, subpath: &str) -> bool {
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

    /// Parse timestamp from trash directory name (format: `{tenant_id}_{timestamp}`).
    ///
    /// The timestamp component is formatted as `%Y%m%d%H%M%S` (e.g. `20260311153045`).
    /// Returns `None` if the name does not match the expected format.
    fn parse_trash_timestamp(dir_name: &str) -> Option<chrono::DateTime<Utc>> {
        // Find the last underscore separator between UUID and timestamp
        let underscore_pos = dir_name.rfind('_')?;
        let timestamp_str = &dir_name[underscore_pos + 1..];
        NaiveDateTime::parse_from_str(timestamp_str, "%Y%m%d%H%M%S")
            .ok()
            .map(|dt| dt.and_utc())
    }

    /// Clean up expired trash directories
    async fn cleanup_trash(&self) -> std::io::Result<()> {
        let retention_secs = self.trash_retention_days * 86400;
        let mut entries = fs::read_dir(&self.trash_dir).await?;
        let mut cleaned = 0u32;
        let now = Utc::now();

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if !metadata.is_dir() {
                continue;
            }

            let dir_name = entry.file_name();
            let name_str = dir_name.to_string_lossy();

            // Parse the creation timestamp from the directory name rather than
            // relying on filesystem mtime, which may be unreliable across mounts.
            let age: Duration = match Self::parse_trash_timestamp(&name_str) {
                Some(created_at) => {
                    let diff_secs = (now - created_at).num_seconds().max(0) as u64;
                    Duration::from_secs(diff_secs)
                }
                None => {
                    // Directory name does not match expected pattern; fall back to
                    // filesystem mtime so we don't silently skip unknown entries.
                    warn!(
                        "Trash directory '{}' has unexpected name format, falling back to mtime",
                        name_str
                    );
                    metadata
                        .modified()
                        .ok()
                        .and_then(|m| m.elapsed().ok())
                        .unwrap_or(Duration::ZERO)
                }
            };

            if age.as_secs() >= retention_secs {
                let path = entry.path();
                info!("Removing expired trash directory: {}", path.display());
                if let Err(e) = fs::remove_dir_all(&path).await {
                    error!("Failed to remove trash directory {}: {}", path.display(), e);
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
