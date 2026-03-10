//! FUSE backend trait
//!
//! Defines the abstract interface between the FUSE filesystem and the actual storage.
//! Implementations translate backend-specific operations (gRPC RPC calls, local storage
//! operations) into this common interface using workspace-proto types directly.

use async_trait::async_trait;
use workspace_proto::{FsFileAttr, FsRenameFlags, FsStatFsResponse};

use crate::error::FuseError;

/// Result type for FUSE backend operations
pub type FuseResult<T> = std::result::Result<T, FuseError>;

/// Directory entry returned by readdir
#[derive(Debug, Clone)]
pub struct FuseDirEntry {
    /// Entry name (basename)
    pub name: String,
    /// File attributes (full metadata for readdir optimization)
    pub attr: Option<FsFileAttr>,
}

/// Abstract FUSE backend trait.
///
/// Implementations provide actual file operations. The FUSE filesystem calls these
/// methods from within `block_on` (synchronous FUSE callbacks → async backend).
///
/// All paths are workspace-relative (e.g., "src/main.rs").
#[async_trait]
pub trait FuseBackend: Send + Sync + 'static {
    /// Look up file attributes by path (stat)
    async fn lookup(&self, path: &str) -> FuseResult<FsFileAttr>;

    /// Get file attributes by path
    async fn getattr(&self, path: &str) -> FuseResult<FsFileAttr>;

    /// Set file attributes (truncate, chmod, utimes)
    async fn setattr(
        &self,
        path: &str,
        size: Option<u64>,
        mode: Option<u32>,
        atime: Option<prost_types::Timestamp>,
        mtime: Option<prost_types::Timestamp>,
    ) -> FuseResult<FsFileAttr>;

    /// Read data from a file at the given offset
    async fn read(&self, path: &str, offset: u64, size: u32) -> FuseResult<Vec<u8>>;

    /// Write data to a file at the given offset
    async fn write(&self, path: &str, offset: u64, data: &[u8]) -> FuseResult<u64>;

    /// Create a new file
    async fn create(&self, path: &str, mode: u32, exclusive: bool) -> FuseResult<FsFileAttr>;

    /// Create a directory
    async fn mkdir(&self, path: &str, mode: u32) -> FuseResult<FsFileAttr>;

    /// Remove a file
    async fn unlink(&self, path: &str) -> FuseResult<()>;

    /// Remove a directory (must be empty)
    async fn rmdir(&self, path: &str) -> FuseResult<()>;

    /// Rename a file or directory
    async fn rename(
        &self,
        old_path: &str,
        new_path: &str,
        flags: FsRenameFlags,
    ) -> FuseResult<()>;

    /// List directory entries
    async fn readdir(&self, path: &str) -> FuseResult<Vec<FuseDirEntry>>;

    /// Read the target of a symbolic link
    async fn readlink(&self, path: &str) -> FuseResult<String>;

    /// Create a symbolic link
    async fn symlink(&self, link_path: &str, target: &str) -> FuseResult<FsFileAttr>;

    /// Get filesystem statistics
    async fn statfs(&self) -> FuseResult<FsStatFsResponse>;

    /// Open a file (optional; default is no-op returning Ok(()))
    async fn open(&self, _path: &str, _flags: u32) -> FuseResult<()> {
        Ok(())
    }

    /// Release (close) a file handle (optional)
    async fn release(&self, _path: &str) -> FuseResult<()> {
        Ok(())
    }

    /// Open a directory (optional)
    async fn opendir(&self, _path: &str) -> FuseResult<()> {
        Ok(())
    }

    /// Release a directory handle (optional)
    async fn releasedir(&self, _path: &str) -> FuseResult<()> {
        Ok(())
    }

    /// Flush file data to storage (optional)
    async fn fsync(&self, _path: &str, _datasync: bool) -> FuseResult<()> {
        Ok(())
    }

    /// Copy a range of data between files (optional)
    async fn copy_file_range(
        &self,
        _src_path: &str,
        _src_offset: u64,
        _dst_path: &str,
        _dst_offset: u64,
        _length: u64,
    ) -> FuseResult<u64> {
        Err(FuseError::NotSupported("copy_file_range".to_string()))
    }
}
