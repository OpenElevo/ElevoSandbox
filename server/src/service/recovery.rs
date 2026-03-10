//! Recovery service
//!
//! Restores remote workspace state on server restart. Scans all remote workspaces
//! and ensures their storage backends, FUSE mounts, and NFS mounts are
//! correctly configured based on persisted DB state.

use std::path::Path;
use std::sync::Arc;

use tracing::{error, info, warn};

use crate::domain::workspace::{RemoteStorageConfig, RemoteTransport, SwitchPhase};
use crate::infra::fuse::mount::FuseMountManager;
use crate::infra::storage::local::LocalStorageBackend;
use crate::infra::storage::nfs_remote::RemoteNfsMountManager;
use crate::infra::storage::remote::RemoteStoragePool;
use crate::infra::storage::router::StorageRouter;
use crate::infra::storage::StorageBackend;
use crate::infra::workspace_repository::WorkspaceRepository;

/// Service for recovering remote workspace state on server startup.
pub struct RecoveryService {
    workspace_repository: Arc<WorkspaceRepository>,
    storage_router: Arc<StorageRouter>,
    fuse_manager: Arc<FuseMountManager>,
    nfs_remote: Arc<RemoteNfsMountManager>,
    remote_pool: Arc<RemoteStoragePool>,
    op_timeout_secs: u64,
    max_concurrent: usize,
    data_stream_threshold: usize,
    transfer_timeout_secs: u64,
}

impl RecoveryService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workspace_repository: Arc<WorkspaceRepository>,
        storage_router: Arc<StorageRouter>,
        fuse_manager: Arc<FuseMountManager>,
        nfs_remote: Arc<RemoteNfsMountManager>,
        remote_pool: Arc<RemoteStoragePool>,
        op_timeout_secs: u64,
        max_concurrent: usize,
        data_stream_threshold: usize,
        transfer_timeout_secs: u64,
    ) -> Self {
        Self {
            workspace_repository,
            storage_router,
            fuse_manager,
            nfs_remote,
            remote_pool,
            op_timeout_secs,
            max_concurrent,
            data_stream_threshold,
            transfer_timeout_secs,
        }
    }

    /// Run recovery for all remote workspaces.
    ///
    /// Should be called once during server startup, before accepting connections.
    pub async fn run(&self) {
        let workspaces = match self.workspace_repository.list_remote().await {
            Ok(ws) => ws,
            Err(e) => {
                error!("Failed to list remote workspaces for recovery: {}", e);
                return;
            }
        };

        if workspaces.is_empty() {
            info!("No remote workspaces to recover");
            return;
        }

        info!("Recovering {} remote workspace(s)", workspaces.len());

        for workspace in &workspaces {
            if let Err(e) = self
                .recover_workspace(&workspace.id, &workspace.storage_config)
                .await
            {
                error!(
                    workspace_id = %workspace.id,
                    error = %e,
                    "Failed to recover remote workspace"
                );
            }
        }

        info!("Remote workspace recovery complete");
    }

    /// Recover a single remote workspace based on its storage config.
    async fn recover_workspace(
        &self,
        workspace_id: &str,
        config: &RemoteStorageConfig,
    ) -> Result<(), String> {
        // Handle interrupted channel switches first
        if config.is_switching() {
            return self.recover_interrupted_switch(workspace_id, config).await;
        }

        match config.transport {
            RemoteTransport::Grpc => self.recover_grpc_workspace(workspace_id).await,
            RemoteTransport::Nfs => self.recover_nfs_workspace(workspace_id, config).await,
        }
    }

    /// Recover a gRPC-transport workspace.
    ///
    /// Creates a RemoteStorageBackend in the pool and registers it with the
    /// StorageRouter. The Client will reconnect and bind the stream later.
    /// FUSE mount is NOT created here — it will be created when the Client connects.
    async fn recover_grpc_workspace(&self, workspace_id: &str) -> Result<(), String> {
        let backend = self.remote_pool.get_or_create(
            workspace_id,
            self.op_timeout_secs,
            self.max_concurrent,
            self.data_stream_threshold,
            self.transfer_timeout_secs,
        );

        self.storage_router
            .register(workspace_id, backend as Arc<dyn StorageBackend>);

        info!(
            workspace_id = %workspace_id,
            "Recovered gRPC workspace (awaiting Client reconnection)"
        );
        Ok(())
    }

    /// Recover an NFS-transport workspace.
    ///
    /// Checks if the NFS mount is still active; if not, remounts it.
    /// Also creates a RemoteStorageBackend in the pool (for potential fallback)
    /// and registers a LocalStorageBackend with the StorageRouter.
    async fn recover_nfs_workspace(
        &self,
        workspace_id: &str,
        config: &RemoteStorageConfig,
    ) -> Result<(), String> {
        let nfs_url = config.nfs_url.as_deref().ok_or_else(|| {
            format!(
                "workspace '{}' has NFS transport but no nfs_url",
                workspace_id
            )
        })?;

        // Also register a RemoteStorageBackend in the pool for potential NFS → gRPC fallback
        let _backend = self.remote_pool.get_or_create(
            workspace_id,
            self.op_timeout_secs,
            self.max_concurrent,
            self.data_stream_threshold,
            self.transfer_timeout_secs,
        );

        // Check if NFS is still mounted
        if !self.nfs_remote.is_mounted(workspace_id).await {
            info!(
                workspace_id = %workspace_id,
                "NFS mount lost, remounting"
            );
            self.nfs_remote
                .mount(workspace_id, nfs_url)
                .await
                .map_err(|e| format!("NFS remount failed: {}", e))?;
        }

        // Register LocalStorageBackend on the NFS mount point
        let local_backend = Arc::new(LocalStorageBackend::new(
            self.nfs_remote.workspace_dir().to_path_buf(),
        )) as Arc<dyn StorageBackend>;
        self.storage_router.register(workspace_id, local_backend);

        info!(
            workspace_id = %workspace_id,
            "Recovered NFS workspace"
        );
        Ok(())
    }

    /// Recover from an interrupted channel switch.
    ///
    /// Strategy:
    /// - Pending phase: The NFS mount was not yet established → rollback to gRPC
    /// - Mounted phase: If temp NFS mount is still present, attempt to complete the
    ///   switch (mount --move → register LocalStorageBackend → update DB). If the
    ///   temp mount is gone or completion fails, rollback to gRPC.
    async fn recover_interrupted_switch(
        &self,
        workspace_id: &str,
        config: &RemoteStorageConfig,
    ) -> Result<(), String> {
        let phase = config.switch_phase.as_ref();

        warn!(
            workspace_id = %workspace_id,
            phase = ?phase,
            "Recovering interrupted channel switch"
        );

        match phase {
            Some(SwitchPhase::Pending) => {
                // NFS mount never happened, just rollback DB
                self.rollback_to_grpc(workspace_id).await?;
                self.recover_grpc_workspace(workspace_id).await
            }
            Some(SwitchPhase::Mounted) => {
                let temp_path = self
                    .nfs_remote
                    .workspace_dir()
                    .join(format!(".nfs-temp-{}", workspace_id));

                if self.nfs_remote.is_path_mounted(&temp_path).await {
                    // Temp NFS mount still exists — attempt to complete the switch
                    info!(
                        workspace_id = %workspace_id,
                        "Temp NFS mount still present, attempting to complete switch"
                    );
                    match self
                        .complete_nfs_switch(workspace_id, config, &temp_path)
                        .await
                    {
                        Ok(()) => return Ok(()),
                        Err(e) => {
                            warn!(
                                workspace_id = %workspace_id,
                                error = %e,
                                "Failed to complete NFS switch, rolling back to gRPC"
                            );
                            // Clean up temp mount before rollback
                            let _ = self.nfs_remote.umount_path(&temp_path).await;
                        }
                    }
                }

                // Temp mount gone or completion failed — rollback to gRPC
                self.rollback_to_grpc(workspace_id).await?;
                self.recover_grpc_workspace(workspace_id).await
            }
            None => {
                // switching_to is set but no phase — treat as pending
                self.rollback_to_grpc(workspace_id).await?;
                self.recover_grpc_workspace(workspace_id).await
            }
        }
    }

    /// Attempt to complete a partially-done gRPC → NFS switch.
    ///
    /// Resumes from after Step 4 (mounted phase):
    /// - mount --move temp → final path
    /// - Register LocalStorageBackend in StorageRouter
    /// - Update DB: transport=nfs, clear switching fields
    async fn complete_nfs_switch(
        &self,
        workspace_id: &str,
        config: &RemoteStorageConfig,
        temp_path: &Path,
    ) -> Result<(), String> {
        let final_path = self.nfs_remote.workspace_dir().join(workspace_id);

        // Ensure final path directory exists
        tokio::fs::create_dir_all(&final_path)
            .await
            .map_err(|e| format!("failed to create final mount dir: {}", e))?;

        // Unmount FUSE if present (may have been mounted before crash)
        self.fuse_manager.umount(workspace_id).await;

        // mount --move temp → final
        self.nfs_remote
            .mount_move(temp_path, &final_path)
            .await
            .map_err(|e| format!("mount --move failed: {}", e))?;

        // Register LocalStorageBackend on the NFS mount point
        let local_backend = Arc::new(LocalStorageBackend::new(
            self.nfs_remote.workspace_dir().to_path_buf(),
        )) as Arc<dyn StorageBackend>;
        self.storage_router.register(workspace_id, local_backend);

        // Also register a RemoteStorageBackend for potential NFS → gRPC fallback
        let _backend = self.remote_pool.get_or_create(
            workspace_id,
            self.op_timeout_secs,
            self.max_concurrent,
            self.data_stream_threshold,
            self.transfer_timeout_secs,
        );

        // Update DB: transport=nfs, clear switching fields
        let nfs_url = config.nfs_url.clone();
        self.workspace_repository
            .update_storage_config(
                workspace_id,
                &RemoteStorageConfig {
                    v: 1,
                    transport: RemoteTransport::Nfs,
                    nfs_url,
                    switching_to: None,
                    switch_phase: None,
                },
            )
            .await
            .map_err(|e| format!("failed to update storage config: {}", e))?;

        info!(
            workspace_id = %workspace_id,
            "Completed interrupted gRPC → NFS switch during recovery"
        );
        Ok(())
    }

    /// Rollback the DB state to gRPC transport, clearing all switching fields.
    async fn rollback_to_grpc(&self, workspace_id: &str) -> Result<(), String> {
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
            .await
            .map_err(|e| format!("failed to rollback storage config: {}", e))?;

        info!(
            workspace_id = %workspace_id,
            "Rolled back storage config to gRPC"
        );
        Ok(())
    }
}
