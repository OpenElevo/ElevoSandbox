//! RPC-based FUSE backend
//!
//! Implements `fuse_core::FuseBackend` by delegating to `FileSystemRpcClient`.
//! This allows the standalone fuse-client to use the shared FUSE filesystem logic.

use async_trait::async_trait;
use tonic::Status;
use workspace_proto::{FsFileAttr, FsRenameFlags, FsStatFsResponse};

use fuse_core::backend::{FuseDirEntry, FuseResult};
use fuse_core::error::FuseError;

use crate::rpc::FileSystemRpcClient;

/// FUSE backend that delegates to gRPC `FileSystemService`.
pub struct RpcFuseBackend {
    rpc: FileSystemRpcClient,
}

impl RpcFuseBackend {
    pub fn new(rpc: FileSystemRpcClient) -> Self {
        Self { rpc }
    }
}

/// Convert a gRPC `Status` to a `FuseError`.
fn status_to_fuse_error(status: Status) -> FuseError {
    let message = status.message().to_string();

    match status.code() {
        tonic::Code::NotFound => FuseError::NotFound(message),
        tonic::Code::PermissionDenied => FuseError::PermissionDenied(message),
        tonic::Code::AlreadyExists => FuseError::AlreadyExists(message),
        tonic::Code::InvalidArgument => FuseError::InvalidArgument(message),
        tonic::Code::Unimplemented => FuseError::NotSupported(message),
        tonic::Code::ResourceExhausted => FuseError::NoSpace(message),
        _ => {
            // Check for specific error messages to map to more precise errors
            let msg_lower = status.message().to_lowercase();
            if msg_lower.contains("not a directory") || msg_lower.contains("enotdir") {
                FuseError::NotDirectory(message)
            } else if msg_lower.contains("is a directory") || msg_lower.contains("eisdir") {
                FuseError::IsDirectory(message)
            } else if msg_lower.contains("not empty") || msg_lower.contains("enotempty") {
                FuseError::NotEmpty(message)
            } else {
                FuseError::IoError(message)
            }
        }
    }
}

#[async_trait]
impl fuse_core::backend::FuseBackend for RpcFuseBackend {
    async fn lookup(&self, path: &str) -> FuseResult<FsFileAttr> {
        self.rpc.stat(path).await.map_err(status_to_fuse_error)
    }

    async fn getattr(&self, path: &str) -> FuseResult<FsFileAttr> {
        self.rpc.stat(path).await.map_err(status_to_fuse_error)
    }

    async fn setattr(
        &self,
        path: &str,
        size: Option<u64>,
        mode: Option<u32>,
        atime: Option<prost_types::Timestamp>,
        mtime: Option<prost_types::Timestamp>,
    ) -> FuseResult<FsFileAttr> {
        self.rpc
            .set_attr(path, size, mode, atime, mtime)
            .await
            .map_err(status_to_fuse_error)
    }

    async fn read(&self, path: &str, offset: u64, size: u32) -> FuseResult<Vec<u8>> {
        self.rpc
            .read_at(path, offset, size)
            .await
            .map_err(status_to_fuse_error)
    }

    async fn write(&self, path: &str, offset: u64, data: &[u8]) -> FuseResult<u64> {
        self.rpc
            .write_at(path, offset, data)
            .await
            .map_err(status_to_fuse_error)
    }

    async fn create(&self, path: &str, mode: u32, exclusive: bool) -> FuseResult<FsFileAttr> {
        self.rpc
            .create(path, mode, exclusive)
            .await
            .map_err(status_to_fuse_error)
    }

    async fn mkdir(&self, path: &str, mode: u32) -> FuseResult<FsFileAttr> {
        self.rpc
            .mkdir(path, mode)
            .await
            .map_err(status_to_fuse_error)
    }

    async fn unlink(&self, path: &str) -> FuseResult<()> {
        self.rpc
            .remove_file(path)
            .await
            .map_err(status_to_fuse_error)
    }

    async fn rmdir(&self, path: &str) -> FuseResult<()> {
        self.rpc
            .remove_dir(path)
            .await
            .map_err(status_to_fuse_error)
    }

    async fn rename(&self, old_path: &str, new_path: &str, flags: FsRenameFlags) -> FuseResult<()> {
        self.rpc
            .rename_with_flags(old_path, new_path, flags)
            .await
            .map_err(status_to_fuse_error)
    }

    async fn readdir(&self, path: &str) -> FuseResult<Vec<FuseDirEntry>> {
        self.rpc.list_dir(path).await.map_err(status_to_fuse_error)
    }

    async fn readlink(&self, path: &str) -> FuseResult<String> {
        self.rpc.read_link(path).await.map_err(status_to_fuse_error)
    }

    async fn symlink(&self, link_path: &str, target: &str) -> FuseResult<FsFileAttr> {
        self.rpc
            .symlink(link_path, target)
            .await
            .map_err(status_to_fuse_error)
    }

    async fn statfs(&self) -> FuseResult<FsStatFsResponse> {
        self.rpc.stat_fs().await.map_err(status_to_fuse_error)
    }
}
