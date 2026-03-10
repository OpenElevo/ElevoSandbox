//! FUSE mount health monitor
//!
//! Periodically checks all active FUSE mounts for remote workspaces.
//! If a mount is unhealthy (unresponsive), it unmounts and remounts the
//! FUSE filesystem.

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use super::mount::FuseMountManager;
use crate::infra::storage::remote::RemoteStoragePool;

/// Background monitor that checks FUSE mount health at regular intervals.
pub struct FuseMountMonitor {
    fuse_manager: Arc<FuseMountManager>,
    remote_pool: Arc<RemoteStoragePool>,
    check_interval: Duration,
}

impl FuseMountMonitor {
    pub fn new(
        fuse_manager: Arc<FuseMountManager>,
        remote_pool: Arc<RemoteStoragePool>,
        check_interval: Duration,
    ) -> Self {
        Self {
            fuse_manager,
            remote_pool,
            check_interval,
        }
    }

    /// Start the background monitoring task.
    pub fn start(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.check_interval);
            loop {
                interval.tick().await;
                self.check_all().await;
            }
        })
    }

    /// Check all mounted FUSE filesystems for health.
    async fn check_all(&self) {
        let mounted = self.fuse_manager.mounted_workspaces();
        for workspace_id in &mounted {
            let healthy = self.fuse_manager.health_check(workspace_id).await;
            crate::infra::metrics::set_fuse_mount_status(workspace_id, healthy);

            if !healthy {
                warn!(
                    workspace_id = %workspace_id,
                    "FUSE mount unhealthy, attempting remount"
                );
                crate::infra::metrics::increment_fuse_remount(workspace_id);
                self.remount(workspace_id).await;
            }
        }
    }

    /// Unmount and remount a FUSE filesystem.
    async fn remount(&self, workspace_id: &str) {
        // Unmount the unhealthy mount
        self.fuse_manager.umount(workspace_id).await;

        // Remount if the backend is connected
        if let Some(backend) = self.remote_pool.get_backend(workspace_id) {
            if backend.is_connected() {
                if let Err(e) = self
                    .fuse_manager
                    .mount(workspace_id, backend)
                    .await
                {
                    error!(
                        workspace_id = %workspace_id,
                        error = %e,
                        "Failed to remount FUSE"
                    );
                } else {
                    info!(
                        workspace_id = %workspace_id,
                        "FUSE remounted successfully"
                    );
                }
            } else {
                info!(
                    workspace_id = %workspace_id,
                    "Skipping FUSE remount: Client not connected"
                );
            }
        }
    }
}
