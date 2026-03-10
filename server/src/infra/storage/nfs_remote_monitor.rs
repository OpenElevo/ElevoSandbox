//! NFS remote mount health monitor
//!
//! Periodically checks all NFS-transport remote workspace mounts.
//! If a mount is lost, it attempts to remount.

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use super::nfs_remote::RemoteNfsMountManager;
use crate::infra::workspace_repository::WorkspaceRepository;

/// Background monitor for NFS remote mounts.
pub struct NfsRemoteMountMonitor {
    workspace_repository: Arc<WorkspaceRepository>,
    nfs_remote: Arc<RemoteNfsMountManager>,
    check_interval: Duration,
}

impl NfsRemoteMountMonitor {
    pub fn new(
        workspace_repository: Arc<WorkspaceRepository>,
        nfs_remote: Arc<RemoteNfsMountManager>,
        check_interval: Duration,
    ) -> Self {
        Self {
            workspace_repository,
            nfs_remote,
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

    /// Check all NFS-transport remote workspaces.
    async fn check_all(&self) {
        let workspaces = match self.workspace_repository.list_remote().await {
            Ok(ws) => ws,
            Err(e) => {
                error!("NFS monitor: failed to list remote workspaces: {}", e);
                return;
            }
        };

        use crate::domain::workspace::RemoteTransport;

        for ws in &workspaces {
            // Only check workspaces using NFS transport (not in the middle of a switch)
            if ws.storage_config.transport != RemoteTransport::Nfs || ws.storage_config.is_switching()
            {
                continue;
            }

            let mounted = self.nfs_remote.is_mounted(&ws.id).await;
            crate::infra::metrics::set_nfs_remote_mount_status(&ws.id, mounted);

            if !mounted {
                warn!(
                    workspace_id = %ws.id,
                    "NFS mount lost, attempting remount"
                );
                crate::infra::metrics::increment_nfs_remote_remount(&ws.id);
                self.remount(&ws.id, &ws.storage_config).await;
            }
        }
    }

    /// Attempt to remount an NFS share for a workspace.
    async fn remount(
        &self,
        workspace_id: &str,
        config: &crate::domain::workspace::RemoteStorageConfig,
    ) {
        let nfs_url = match config.nfs_url.as_deref() {
            Some(url) => url,
            None => {
                error!(
                    workspace_id = %workspace_id,
                    "NFS transport but no nfs_url configured"
                );
                return;
            }
        };

        match self.nfs_remote.mount(workspace_id, nfs_url).await {
            Ok(_) => {
                info!(
                    workspace_id = %workspace_id,
                    "NFS remounted successfully"
                );
            }
            Err(e) => {
                error!(
                    workspace_id = %workspace_id,
                    error = %e,
                    "Failed to remount NFS"
                );
            }
        }
    }
}
