//! Server-side FUSE backend
//!
//! Adapts `RemoteStorageBackend` (which proxies operations to a connected Client
//! over gRPC) to the `fuse_core::FuseBackend` trait for FUSE mount operations.
//!
//! Key design decision: holds `Arc<RemoteStorageBackend>` directly, NOT going through
//! `StorageRouter`, to avoid circular dependencies (StorageRouter → FUSE → StorageRouter).

use std::sync::Arc;

use async_trait::async_trait;
use workspace_proto::{FsFileAttr, FsFileType, FsRenameFlags, FsStatFsResponse};

use fuse_core::backend::{FuseDirEntry, FuseResult};
use fuse_core::error::FuseError;

use crate::infra::storage::remote::RemoteStorageBackend;
use crate::infra::storage::{FileType, StorageBackend, StorageError};

/// Server-side FUSE backend that wraps a `RemoteStorageBackend`.
pub struct ServerFuseBackend {
    workspace_id: String,
    backend: Arc<RemoteStorageBackend>,
}

impl ServerFuseBackend {
    pub fn new(workspace_id: String, backend: Arc<RemoteStorageBackend>) -> Self {
        Self {
            workspace_id,
            backend,
        }
    }
}

/// Convert `StorageError` to `FuseError`
fn storage_to_fuse_error(err: StorageError) -> FuseError {
    match err {
        StorageError::NotFound(msg) => FuseError::NotFound(msg),
        StorageError::AlreadyExists(msg) => FuseError::AlreadyExists(msg),
        StorageError::IsADirectory(msg) => FuseError::IsDirectory(msg),
        StorageError::NotADirectory(msg) => FuseError::NotDirectory(msg),
        StorageError::NotAFile(msg) => FuseError::NotDirectory(msg),
        StorageError::DirectoryNotEmpty(msg) => FuseError::NotEmpty(msg),
        StorageError::PermissionDenied(msg) => FuseError::PermissionDenied(msg),
        StorageError::PathTraversalDenied(msg) => FuseError::PathTraversalDenied(msg),
        StorageError::NotSupported(msg) => FuseError::NotSupported(msg),
        StorageError::Io { path, source } => {
            FuseError::IoError(format!("{}: {}", path, source))
        }
        StorageError::Internal(msg) => FuseError::Internal(msg),
    }
}

/// Convert `crate::infra::storage::FileStat` to `FsFileAttr`
fn file_stat_to_proto(stat: &crate::infra::storage::FileStat) -> FsFileAttr {
    let file_type = match stat.file_type {
        FileType::File => FsFileType::File,
        FileType::Directory => FsFileType::Directory,
        FileType::Symlink => FsFileType::Symlink,
    };

    let atime = stat.accessed_at.map(|dt| prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    });
    let mtime = stat.modified_at.map(|dt| prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    });
    let ctime = stat.created_at.map(|dt| prost_types::Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    });

    let size = stat.size;
    let blocks = size.div_ceil(512);

    FsFileAttr {
        file_type: file_type.into(),
        size,
        mode: stat.mode,
        uid: stat.uid,
        gid: stat.gid,
        atime,
        mtime,
        ctime,
        nlink: if stat.file_type == FileType::Directory {
            2
        } else {
            1
        },
        blksize: 4096,
        blocks,
    }
}

#[async_trait]
impl fuse_core::backend::FuseBackend for ServerFuseBackend {
    async fn lookup(&self, path: &str) -> FuseResult<FsFileAttr> {
        let stat = self
            .backend
            .stat(&self.workspace_id, path)
            .await
            .map_err(storage_to_fuse_error)?;
        Ok(file_stat_to_proto(&stat))
    }

    async fn getattr(&self, path: &str) -> FuseResult<FsFileAttr> {
        let stat = self
            .backend
            .stat(&self.workspace_id, path)
            .await
            .map_err(storage_to_fuse_error)?;
        Ok(file_stat_to_proto(&stat))
    }

    async fn setattr(
        &self,
        path: &str,
        size: Option<u64>,
        mode: Option<u32>,
        atime: Option<prost_types::Timestamp>,
        mtime: Option<prost_types::Timestamp>,
    ) -> FuseResult<FsFileAttr> {
        use chrono::TimeZone;

        // Apply individual attribute changes.
        // Track which operations succeeded so partial failures are diagnosable.
        let mut applied = Vec::new();

        if let Some(new_size) = size {
            if let Err(e) = self
                .backend
                .set_file_size(&self.workspace_id, path, new_size)
                .await
            {
                return Err(storage_to_fuse_error(e));
            }
            applied.push("size");
        }

        if let Some(new_mode) = mode {
            if let Err(e) = self
                .backend
                .set_permissions(&self.workspace_id, path, new_mode)
                .await
            {
                if !applied.is_empty() {
                    tracing::warn!(
                        path = %path,
                        applied = ?applied,
                        error = %e,
                        "setattr partial failure: mode change failed after other changes applied"
                    );
                }
                return Err(storage_to_fuse_error(e));
            }
            applied.push("mode");
        }

        if atime.is_some() || mtime.is_some() {
            let atime_chrono = atime.map(|ts| {
                chrono::Utc
                    .timestamp_opt(ts.seconds, ts.nanos as u32)
                    .single()
                    .unwrap_or_else(chrono::Utc::now)
            });
            let mtime_chrono = mtime.map(|ts| {
                chrono::Utc
                    .timestamp_opt(ts.seconds, ts.nanos as u32)
                    .single()
                    .unwrap_or_else(chrono::Utc::now)
            });
            if let Err(e) = self
                .backend
                .set_times(&self.workspace_id, path, atime_chrono, mtime_chrono)
                .await
            {
                if !applied.is_empty() {
                    tracing::warn!(
                        path = %path,
                        applied = ?applied,
                        error = %e,
                        "setattr partial failure: times change failed after other changes applied"
                    );
                }
                return Err(storage_to_fuse_error(e));
            }
        }

        // Re-fetch attributes after modifications
        let stat = self
            .backend
            .stat(&self.workspace_id, path)
            .await
            .map_err(storage_to_fuse_error)?;
        Ok(file_stat_to_proto(&stat))
    }

    async fn read(&self, path: &str, offset: u64, size: u32) -> FuseResult<Vec<u8>> {
        self.backend
            .read_file_range(&self.workspace_id, path, offset, size)
            .await
            .map_err(storage_to_fuse_error)
    }

    async fn write(&self, path: &str, offset: u64, data: &[u8]) -> FuseResult<u64> {
        self.backend
            .write_file_at(&self.workspace_id, path, offset, data)
            .await
            .map_err(storage_to_fuse_error)?;
        Ok(data.len() as u64)
    }

    async fn create(&self, path: &str, mode: u32, exclusive: bool) -> FuseResult<FsFileAttr> {
        self.backend
            .create_file(&self.workspace_id, path, exclusive)
            .await
            .map_err(storage_to_fuse_error)?;

        // Set permissions if non-default
        if mode != 0 && mode != 0o644 {
            let _ = self
                .backend
                .set_permissions(&self.workspace_id, path, mode)
                .await;
        }

        // Fetch attributes for the newly created file
        let stat = self
            .backend
            .stat(&self.workspace_id, path)
            .await
            .map_err(storage_to_fuse_error)?;
        Ok(file_stat_to_proto(&stat))
    }

    async fn mkdir(&self, path: &str, mode: u32) -> FuseResult<FsFileAttr> {
        self.backend
            .mkdir(&self.workspace_id, path, false)
            .await
            .map_err(storage_to_fuse_error)?;

        // Set permissions if non-default
        if mode != 0 && mode != 0o755 {
            let _ = self
                .backend
                .set_permissions(&self.workspace_id, path, mode)
                .await;
        }

        // Fetch attributes for the newly created directory
        let stat = self
            .backend
            .stat(&self.workspace_id, path)
            .await
            .map_err(storage_to_fuse_error)?;
        Ok(file_stat_to_proto(&stat))
    }

    async fn unlink(&self, path: &str) -> FuseResult<()> {
        self.backend
            .remove_file(&self.workspace_id, path)
            .await
            .map_err(storage_to_fuse_error)
    }

    async fn rmdir(&self, path: &str) -> FuseResult<()> {
        self.backend
            .remove_dir(&self.workspace_id, path, false)
            .await
            .map_err(storage_to_fuse_error)
    }

    async fn rename(
        &self,
        old_path: &str,
        new_path: &str,
        flags: FsRenameFlags,
    ) -> FuseResult<()> {
        match flags {
            FsRenameFlags::None => {
                self.backend
                    .rename(&self.workspace_id, old_path, new_path)
                    .await
            }
            FsRenameFlags::Noreplace => {
                self.backend
                    .rename_noreplace(&self.workspace_id, old_path, new_path)
                    .await
            }
            FsRenameFlags::Exchange => {
                self.backend
                    .rename_exchange(&self.workspace_id, old_path, new_path)
                    .await
            }
        }
        .map_err(storage_to_fuse_error)
    }

    async fn readdir(&self, path: &str) -> FuseResult<Vec<FuseDirEntry>> {
        let entries = self
            .backend
            .list_dir(&self.workspace_id, path)
            .await
            .map_err(storage_to_fuse_error)?;

        Ok(entries
            .into_iter()
            .map(|stat| FuseDirEntry {
                name: stat.name.clone(),
                attr: Some(file_stat_to_proto(&stat)),
            })
            .collect())
    }

    async fn readlink(&self, path: &str) -> FuseResult<String> {
        self.backend
            .readlink(&self.workspace_id, path)
            .await
            .map_err(storage_to_fuse_error)
    }

    async fn symlink(&self, link_path: &str, target: &str) -> FuseResult<FsFileAttr> {
        self.backend
            .symlink(&self.workspace_id, link_path, target)
            .await
            .map_err(storage_to_fuse_error)?;

        // Fetch attributes for the newly created symlink
        let stat = self
            .backend
            .stat(&self.workspace_id, link_path)
            .await
            .map_err(storage_to_fuse_error)?;
        Ok(file_stat_to_proto(&stat))
    }

    async fn statfs(&self) -> FuseResult<FsStatFsResponse> {
        let stats = self
            .backend
            .stat_fs(&self.workspace_id)
            .await
            .map_err(storage_to_fuse_error)?;

        Ok(FsStatFsResponse {
            blocks: stats.blocks,
            bfree: stats.bfree,
            bavail: stats.bavail,
            files: stats.files,
            ffree: stats.ffree,
            bsize: stats.bsize,
            namelen: stats.namelen,
            frsize: stats.frsize,
        })
    }
}
