//! Remote storage service
//!
//! Manages transport channel switching for remote workspaces (gRPC ↔ NFS).
//! Implements the state machine for safe, atomic channel transitions with
//! proper rollback on failure.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{error, info, warn};

use crate::config::Config;
use crate::domain::workspace::{RemoteStorageConfig, RemoteTransport, SwitchPhase};
use crate::error::{Error, Result};
use crate::infra::fuse::mount::FuseMountManager;
use crate::infra::storage::local::LocalStorageBackend;
use crate::infra::storage::nfs_remote::RemoteNfsMountManager;
use crate::infra::storage::remote::RemoteStoragePool;
use crate::infra::storage::router::StorageRouter;
use crate::infra::storage::StorageBackend;
use crate::infra::workspace_repository::WorkspaceRepository;

/// Timeout for acquiring the write lock during channel switching
const CHANNEL_SWITCH_LOCK_TIMEOUT: Duration = Duration::from_secs(60);

/// Service for managing remote storage transport channels.
pub struct RemoteStorageService {
    workspace_repository: Arc<WorkspaceRepository>,
    storage_router: Arc<StorageRouter>,
    fuse_manager: Arc<FuseMountManager>,
    nfs_remote: Arc<RemoteNfsMountManager>,
    remote_pool: Arc<RemoteStoragePool>,
    #[allow(dead_code)]
    config: Arc<Config>,
}

impl RemoteStorageService {
    pub fn new(
        workspace_repository: Arc<WorkspaceRepository>,
        storage_router: Arc<StorageRouter>,
        fuse_manager: Arc<FuseMountManager>,
        nfs_remote: Arc<RemoteNfsMountManager>,
        remote_pool: Arc<RemoteStoragePool>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            workspace_repository,
            storage_router,
            fuse_manager,
            nfs_remote,
            remote_pool,
            config,
        }
    }

    /// Register NFS transport for a remote workspace (switch from gRPC to NFS).
    ///
    /// State machine:
    /// 1. Validate: workspace exists, is remote, transport=grpc, not switching
    /// 2. Update DB: switching_to=nfs, phase=pending
    /// 3. Mount NFS to temp path (fail → rollback DB)
    /// 4. Update DB: phase=mounted
    /// 5. Acquire write lock (drain in-flight ops)
    /// 6. Unmount FUSE
    /// 7. mount --move temp → final path
    /// 8. Replace StorageRouter backend with LocalStorageBackend
    /// 9. Update DB: transport=nfs, clear switching fields
    pub async fn register_nfs_transport(&self, workspace_id: &str, nfs_url: &str) -> Result<()> {
        // Step 1: Validate
        let workspace = self.workspace_repository.get(workspace_id).await?;
        if !workspace.is_remote() {
            return Err(Error::InvalidRequest(format!(
                "workspace '{}' is not a remote workspace",
                workspace_id
            )));
        }
        if workspace.storage_config.transport != RemoteTransport::Grpc {
            return Err(Error::InvalidRequest(format!(
                "workspace '{}' is not using gRPC transport",
                workspace_id
            )));
        }
        if workspace.storage_config.is_switching() {
            return Err(Error::InvalidRequest(format!(
                "workspace '{}' has a channel switch in progress",
                workspace_id
            )));
        }

        // Validate NFS URL before starting state machine
        self.nfs_remote
            .validate_nfs_url(nfs_url)
            .await
            .map_err(|e| Error::InvalidRequest(format!("invalid NFS URL: {}", e)))?;

        let switch_start = Instant::now();

        info!(
            workspace_id = %workspace_id,
            nfs_url = %nfs_url,
            "Starting gRPC → NFS channel switch"
        );

        // Step 2: Update DB state → switching_to=nfs, phase=pending
        self.workspace_repository
            .update_storage_config(
                workspace_id,
                &RemoteStorageConfig {
                    v: 1,
                    transport: RemoteTransport::Grpc,
                    nfs_url: Some(nfs_url.to_string()),
                    switching_to: Some(RemoteTransport::Nfs),
                    switch_phase: Some(SwitchPhase::Pending),
                },
            )
            .await?;

        // Step 3: Mount NFS to temp path
        let temp_mount = match self.nfs_remote.mount_temp(workspace_id, nfs_url).await {
            Ok(mp) => mp,
            Err(e) => {
                error!(
                    workspace_id = %workspace_id,
                    error = %e,
                    "NFS mount to temp path failed, rolling back"
                );
                // Rollback DB
                self.rollback_switch_state(workspace_id).await;
                crate::infra::metrics::increment_channel_switch_error("grpc", "nfs");
                return Err(Error::Internal(format!("NFS mount failed: {}", e)));
            }
        };

        // Step 4: Update DB state → phase=mounted
        if let Err(e) = self
            .workspace_repository
            .update_storage_config(
                workspace_id,
                &RemoteStorageConfig {
                    v: 1,
                    transport: RemoteTransport::Grpc,
                    nfs_url: Some(nfs_url.to_string()),
                    switching_to: Some(RemoteTransport::Nfs),
                    switch_phase: Some(SwitchPhase::Mounted),
                },
            )
            .await
        {
            let _ = self.nfs_remote.umount_path(&temp_mount).await;
            return Err(e);
        }

        // Step 5: Acquire write lock to drain in-flight operations
        let _write_guard = match self
            .storage_router
            .write_lock(workspace_id, CHANNEL_SWITCH_LOCK_TIMEOUT)
            .await
        {
            Ok(guard) => guard,
            Err(e) => {
                error!(
                    workspace_id = %workspace_id,
                    error = %e,
                    "Failed to acquire write lock for channel switch"
                );
                let _ = self.nfs_remote.umount_path(&temp_mount).await;
                self.rollback_switch_state(workspace_id).await;
                crate::infra::metrics::increment_channel_switch_error("grpc", "nfs");
                return Err(Error::Internal(format!(
                    "channel switch lock timeout: {}",
                    e
                )));
            }
        };

        // Step 6: Unmount FUSE
        self.fuse_manager.umount(workspace_id).await;

        // Step 7: mount --move temp → final path
        let final_path = self.nfs_remote.workspace_dir().join(workspace_id);
        if let Err(e) = self.nfs_remote.mount_move(&temp_mount, &final_path).await {
            error!(
                workspace_id = %workspace_id,
                error = %e,
                "mount --move failed, attempting recovery"
            );
            // Recovery: unmount temp NFS, remount FUSE
            let _ = self.nfs_remote.umount_path(&temp_mount).await;
            if let Some(backend) = self.remote_pool.get_backend(workspace_id) {
                let _ = self.fuse_manager.mount(workspace_id, backend.clone()).await;
            }
            self.rollback_switch_state(workspace_id).await;
            crate::infra::metrics::increment_channel_switch_error("grpc", "nfs");
            return Err(Error::Internal(format!("mount --move failed: {}", e)));
        }

        // Step 8: Replace StorageRouter backend with LocalStorageBackend
        let local_backend = Arc::new(LocalStorageBackend::new(
            self.nfs_remote.workspace_dir().to_path_buf(),
        )) as Arc<dyn StorageBackend>;
        self.storage_router
            .replace_backend(workspace_id, local_backend);

        // Step 9: Update DB → transport=nfs, clear switching fields
        self.workspace_repository
            .update_storage_config(
                workspace_id,
                &RemoteStorageConfig {
                    v: 1,
                    transport: RemoteTransport::Nfs,
                    nfs_url: Some(nfs_url.to_string()),
                    switching_to: None,
                    switch_phase: None,
                },
            )
            .await?;

        crate::infra::metrics::record_channel_switch(
            "grpc",
            "nfs",
            switch_start.elapsed().as_secs_f64(),
        );

        info!(
            workspace_id = %workspace_id,
            "gRPC → NFS channel switch completed"
        );

        Ok(())
    }

    /// Unregister NFS transport (switch from NFS back to gRPC).
    pub async fn unregister_nfs_transport(&self, workspace_id: &str) -> Result<()> {
        let workspace = self.workspace_repository.get(workspace_id).await?;
        if !workspace.is_remote() {
            return Err(Error::InvalidRequest(format!(
                "workspace '{}' is not a remote workspace",
                workspace_id
            )));
        }
        if workspace.storage_config.transport != RemoteTransport::Nfs {
            return Err(Error::InvalidRequest(format!(
                "workspace '{}' is not using NFS transport",
                workspace_id
            )));
        }
        if workspace.storage_config.is_switching() {
            return Err(Error::InvalidRequest(format!(
                "workspace '{}' has a channel switch in progress",
                workspace_id
            )));
        }

        let switch_start = Instant::now();

        info!(
            workspace_id = %workspace_id,
            "Starting NFS → gRPC channel switch"
        );

        // Acquire write lock
        let _write_guard = self
            .storage_router
            .write_lock(workspace_id, CHANNEL_SWITCH_LOCK_TIMEOUT)
            .await
            .map_err(|e| Error::Internal(format!("channel switch lock timeout: {}", e)))?;

        // Unmount NFS
        if let Err(e) = self.nfs_remote.umount(workspace_id).await {
            warn!(
                workspace_id = %workspace_id,
                error = %e,
                "NFS unmount failed (non-fatal, proceeding)"
            );
        }

        // Replace backend with RemoteStorageBackend (already registered in the pool)
        if let Some(backend) = self.remote_pool.get_backend(workspace_id) {
            self.storage_router
                .replace_backend(workspace_id, backend.clone() as Arc<dyn StorageBackend>);

            // Remount FUSE if the backend is connected
            if backend.is_connected() {
                if let Err(e) = self
                    .fuse_manager
                    .mount_if_not_exists(workspace_id, backend.clone())
                    .await
                {
                    warn!(
                        workspace_id = %workspace_id,
                        error = %e,
                        "Failed to remount FUSE after NFS → gRPC switch"
                    );
                }
            }
        }

        // Update DB
        self.workspace_repository
            .update_storage_config(
                workspace_id,
                &RemoteStorageConfig {
                    v: 1,
                    transport: RemoteTransport::Grpc,
                    nfs_url: None,
                    switching_to: None,
                    switch_phase: None,
                },
            )
            .await?;

        crate::infra::metrics::record_channel_switch(
            "nfs",
            "grpc",
            switch_start.elapsed().as_secs_f64(),
        );

        info!(
            workspace_id = %workspace_id,
            "NFS → gRPC channel switch completed"
        );

        Ok(())
    }

    /// Rollback DB state after a failed channel switch.
    async fn rollback_switch_state(&self, workspace_id: &str) {
        if let Err(e) = self
            .workspace_repository
            .update_storage_config(
                workspace_id,
                &RemoteStorageConfig {
                    v: 1,
                    transport: RemoteTransport::Grpc,
                    nfs_url: None,
                    switching_to: None,
                    switch_phase: None,
                },
            )
            .await
        {
            error!(
                workspace_id = %workspace_id,
                error = %e,
                "Failed to rollback storage config"
            );
        }
    }
}
