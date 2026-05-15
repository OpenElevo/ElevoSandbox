//! FUSE mount lifecycle management
//!
//! Manages FUSE mount points for remote workspaces on the server side.
//! Each remote workspace gets a FUSE mount backed by its `RemoteStorageBackend`,
//! making the Client's files accessible as a local directory tree.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use fuse_core::filesystem::{FuseFilesystemWrapper, WorkspaceFuse};
use fuser::MountOption;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use super::backend::ServerFuseBackend;
use crate::infra::storage::remote::RemoteStorageBackend;

/// Default FUSE read block size (128KB)
const DEFAULT_BLOCK_SIZE: u32 = 128 * 1024;

/// Default FUSE read cache size (32MB)
const DEFAULT_READ_CACHE_SIZE: u64 = 32 * 1024 * 1024;

/// Active FUSE mount entry
struct MountEntry {
    /// Mount point path
    mount_point: PathBuf,
    /// Reference to the filesystem for cache invalidation
    fuse: Arc<WorkspaceFuse<ServerFuseBackend>>,
    /// Handle to the spawn_blocking task running fuser::mount2
    task: JoinHandle<()>,
}

/// Manages FUSE mounts for remote workspaces.
///
/// Provides idempotent mount/umount, health checking, and cache invalidation.
pub struct FuseMountManager {
    /// Active mounts: workspace_id → MountEntry
    mounts: DashMap<String, MountEntry>,
    /// Per-workspace mutex to prevent concurrent mount/umount operations
    mount_locks: DashMap<String, Arc<Mutex<()>>>,
    /// Base directory for mount points
    workspace_dir: PathBuf,
    /// Cache TTL for FUSE entry/attribute metadata
    cache_ttl: Duration,
}

impl FuseMountManager {
    pub fn new(workspace_dir: PathBuf, cache_ttl: Duration) -> Self {
        Self {
            mounts: DashMap::new(),
            mount_locks: DashMap::new(),
            workspace_dir,
            cache_ttl,
        }
    }

    /// Remount a FUSE filesystem for a remote workspace.
    ///
    /// Unmounts any existing mount first, then creates a fresh one with the
    /// new backend.  Used when a remote StorageProvider reconnects so that
    /// the FUSE filesystem talks to the live backend instead of the stale,
    /// disconnected one.
    pub async fn remount(
        &self,
        workspace_id: &str,
        backend: Arc<RemoteStorageBackend>,
    ) -> Result<(), String> {
        let lock = self.get_mount_lock(workspace_id);
        let _guard = lock.lock().await;

        // Unmount first if one exists (handles stale mounts from previous
        // connections).  We inline the removal instead of calling
        // `self.umount()` to avoid a deadlock on the per-workspace mutex.
        if let Some((_, entry)) = self.mounts.remove(workspace_id) {
            let mp_str = entry.mount_point.display().to_string();
            let ws_id = workspace_id.to_string();

            // Best-effort unmount — ignore errors since the old mount may
            // already have been torn down by the OS or an earlier restart.
            let result = tokio::process::Command::new("fusermount")
                .args(["-u", &mp_str])
                .output()
                .await;
            match result {
                Ok(output) if output.status.success() => {
                    info!(workspace_id = %ws_id, "Stale FUSE unmounted for remount");
                }
                _ => {
                    // Fall back to lazy unmount
                    warn!(workspace_id = %ws_id, "fusermount -u failed during remount, trying lazy unmount");
                    let _ = tokio::process::Command::new("fusermount")
                        .args(["-uz", &mp_str])
                        .output()
                        .await;
                }
            }
            // Wait for the old mount task to finish (with timeout).
            let _ = tokio::time::timeout(Duration::from_secs(5), entry.task).await;
        }

        self.mount_inner(workspace_id, backend).await
    }

    /// Get or create a per-workspace mount mutex.
    fn get_mount_lock(&self, workspace_id: &str) -> Arc<Mutex<()>> {
        self.mount_locks
            .entry(workspace_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone()
    }

    /// Mount a FUSE filesystem for a remote workspace.
    ///
    /// Creates the mount point directory and starts a `fuser::mount2` task.
    /// Uses a per-workspace mutex to prevent concurrent mount attempts.
    pub async fn mount(
        &self,
        workspace_id: &str,
        backend: Arc<RemoteStorageBackend>,
    ) -> Result<(), String> {
        let lock = self.get_mount_lock(workspace_id);
        let _guard = lock.lock().await;

        // Re-check under lock
        if self.mounts.contains_key(workspace_id) {
            return Ok(()); // Already mounted
        }

        self.mount_inner(workspace_id, backend).await
    }

    /// Idempotent mount: only mounts if not already mounted.
    /// Uses a per-workspace mutex to prevent concurrent mount attempts.
    pub async fn mount_if_not_exists(
        &self,
        workspace_id: &str,
        backend: Arc<RemoteStorageBackend>,
    ) -> Result<(), String> {
        // Fast path: already mounted (lock-free check)
        if self.mounts.contains_key(workspace_id) {
            return Ok(());
        }

        let lock = self.get_mount_lock(workspace_id);
        let _guard = lock.lock().await;

        // Re-check under lock
        if self.mounts.contains_key(workspace_id) {
            return Ok(());
        }

        self.mount_inner(workspace_id, backend).await
    }

    /// Internal mount implementation (must be called with mount lock held).
    async fn mount_inner(
        &self,
        workspace_id: &str,
        backend: Arc<RemoteStorageBackend>,
    ) -> Result<(), String> {
        let mount_point = self.workspace_dir.join(workspace_id);

        // Create mount point directory.
        // A previous server run or disconnected client may leave a stale
        // FUSE mount behind.  If creation fails, clean up and retry.
        if let Err(e) = tokio::fs::create_dir_all(&mount_point).await {
            warn!(
                workspace_id = %workspace_id,
                error = %e,
                "Failed to create mount point, attempting stale mount cleanup"
            );
            let mp_str = mount_point.display().to_string();
            let _ = tokio::process::Command::new("fusermount")
                .args(["-u", &mp_str])
                .output()
                .await;
            let _ = tokio::process::Command::new("fusermount")
                .args(["-uz", &mp_str])
                .output()
                .await;
            let _ = tokio::fs::remove_dir(&mount_point).await;
            tokio::fs::create_dir_all(&mount_point).await.map_err(|e| {
                format!(
                    "failed to create mount point {}: {}",
                    mount_point.display(),
                    e
                )
            })?;
        }

        // Build the FUSE filesystem
        let fuse_backend = ServerFuseBackend::new(workspace_id.to_string(), backend);
        let handle = tokio::runtime::Handle::current();
        let fuse = Arc::new(WorkspaceFuse::new(
            workspace_id.to_string(),
            handle,
            fuse_backend,
            self.cache_ttl,
            DEFAULT_BLOCK_SIZE,
            DEFAULT_READ_CACHE_SIZE,
        ));

        let wrapper = FuseFilesystemWrapper::new(fuse.clone());

        let mount_options = vec![
            MountOption::FSName(format!("remote:{}", workspace_id)),
            MountOption::Subtype("workspace-remote".to_string()),
            MountOption::DefaultPermissions,
            MountOption::AllowOther,
            MountOption::NoAtime,
        ];

        let mp = mount_point.clone();
        let ws_id = workspace_id.to_string();
        let task = tokio::task::spawn_blocking(move || {
            info!(workspace_id = %ws_id, mount_point = %mp.display(), "Starting FUSE mount");
            if let Err(e) = fuser::mount2(wrapper, &mp, &mount_options) {
                error!(workspace_id = %ws_id, error = %e, "FUSE mount failed");
            }
            info!(workspace_id = %ws_id, "FUSE mount exited");
        });

        // Wait for mount to become ready by polling metadata
        let ready = wait_for_mount_ready(&mount_point, Duration::from_secs(5)).await;
        if !ready {
            warn!(workspace_id = %workspace_id, "FUSE mount not ready within timeout, registering anyway");
        }

        self.mounts.insert(
            workspace_id.to_string(),
            MountEntry {
                mount_point,
                fuse,
                task,
            },
        );

        info!(workspace_id = %workspace_id, "FUSE mount registered");
        Ok(())
    }

    /// Unmount a FUSE filesystem for a remote workspace.
    pub async fn umount(&self, workspace_id: &str) {
        let lock = self.get_mount_lock(workspace_id);
        let _guard = lock.lock().await;

        if let Some((_, entry)) = self.mounts.remove(workspace_id) {
            let mp_str = entry.mount_point.display().to_string();

            // Try fusermount -u first
            let result = tokio::process::Command::new("fusermount")
                .args(["-u", &mp_str])
                .output()
                .await;

            match result {
                Ok(output) if output.status.success() => {
                    info!(workspace_id = %workspace_id, "FUSE unmounted successfully");
                }
                _ => {
                    // Fall back to lazy unmount
                    warn!(
                        workspace_id = %workspace_id,
                        "fusermount -u failed, trying lazy unmount"
                    );
                    let _ = tokio::process::Command::new("fusermount")
                        .args(["-uz", &mp_str])
                        .output()
                        .await;
                }
            }

            // Wait for the mount task to exit (with timeout)
            let _ = tokio::time::timeout(Duration::from_secs(5), entry.task).await;
        }
    }

    /// Check if a mount is healthy by stat-ing the mount point.
    pub async fn health_check(&self, workspace_id: &str) -> bool {
        if let Some(entry) = self.mounts.get(workspace_id) {
            let mp = entry.mount_point.clone();
            match tokio::time::timeout(Duration::from_secs(5), tokio::fs::metadata(&mp)).await {
                Ok(Ok(metadata)) => metadata.is_dir(),
                _ => false,
            }
        } else {
            false
        }
    }

    /// Invalidate caches for specific paths in a workspace mount.
    ///
    /// Called when receiving `FileChanged` events from the Client.
    pub fn invalidate_paths(&self, workspace_id: &str, paths: &[String]) {
        if let Some(entry) = self.mounts.get(workspace_id) {
            for path in paths {
                entry.fuse.invalidate_path(path);
            }
        }
    }

    /// Purge all caches for a workspace mount (used on Client reconnection).
    pub fn purge_all_caches(&self, workspace_id: &str) {
        if let Some(entry) = self.mounts.get(workspace_id) {
            entry.fuse.purge_all_caches();
        }
    }

    /// Check if a workspace has an active mount.
    pub fn is_mounted(&self, workspace_id: &str) -> bool {
        self.mounts.contains_key(workspace_id)
    }

    /// Get mount point path for a workspace.
    #[allow(dead_code)]
    pub fn mount_point(&self, workspace_id: &str) -> Option<PathBuf> {
        self.mounts.get(workspace_id).map(|e| e.mount_point.clone())
    }

    /// Get all mounted workspace IDs.
    pub fn mounted_workspaces(&self) -> Vec<String> {
        self.mounts.iter().map(|e| e.key().clone()).collect()
    }
}

/// Poll until a mount point becomes accessible, or timeout.
async fn wait_for_mount_ready(mount_point: &std::path::Path, timeout: Duration) -> bool {
    let start = tokio::time::Instant::now();
    let poll_interval = Duration::from_millis(50);

    while start.elapsed() < timeout {
        if let Ok(meta) = tokio::fs::metadata(mount_point).await {
            if meta.is_dir() {
                return true;
            }
        }
        tokio::time::sleep(poll_interval).await;
    }
    false
}
