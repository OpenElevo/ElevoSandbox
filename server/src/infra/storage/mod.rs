//! Storage backend abstraction
//!
//! Defines the `StorageBackend` trait for decoupling file operations from
//! concrete storage implementations. All path parameters are workspace-relative
//! paths (e.g., "src/main.rs") — the backend maps them to physical locations.

pub mod lease;
pub mod local;
pub mod nfs_remote;
pub mod nfs_remote_monitor;
pub mod remote;
pub mod router;
pub mod s3fs_mount;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::service::workspace::FileInfo;

/// File type enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    File,
    Directory,
    Symlink,
}

/// File metadata (storage layer DTO)
///
/// Contains full POSIX metadata needed by the NFS layer. For HTTP API responses,
/// convert to `FileInfo` via the `From<FileStat> for FileInfo` implementation.
#[derive(Debug, Clone)]
pub struct FileStat {
    /// File name (basename)
    pub name: String,
    /// Relative path within workspace
    pub path: String,
    /// File type
    pub file_type: FileType,
    /// File size in bytes
    pub size: u64,
    /// Unix permission mode (e.g., 0o644)
    pub mode: u32,
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
    /// Last modification time
    pub modified_at: Option<DateTime<Utc>>,
    /// Last access time
    pub accessed_at: Option<DateTime<Utc>>,
    /// Creation time (birth time on supported filesystems)
    pub created_at: Option<DateTime<Utc>>,
}

/// Storage backend error types
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("file not found: {0}")]
    NotFound(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("is a directory: {0}")]
    IsADirectory(String),

    #[error("not a directory: {0}")]
    NotADirectory(String),

    #[error("not a file: {0}")]
    NotAFile(String),

    #[error("directory not empty: {0}")]
    DirectoryNotEmpty(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("path traversal denied: {0}")]
    PathTraversalDenied(String),

    #[error("operation not supported: {0}")]
    NotSupported(String),

    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },

    #[error("storage backend error: {0}")]
    Internal(String),
}

impl StorageError {
    /// Convert from `std::io::Error` with path context.
    ///
    /// Maps well-known errno values (via `libc` constants) to semantic
    /// `StorageError` variants for precise error handling upstream.
    pub fn from_io(err: std::io::Error, path: impl Into<String>) -> Self {
        let path = path.into();
        match err.kind() {
            std::io::ErrorKind::NotFound => StorageError::NotFound(path),
            std::io::ErrorKind::AlreadyExists => StorageError::AlreadyExists(path),
            std::io::ErrorKind::PermissionDenied => StorageError::PermissionDenied(path),
            _ if err.raw_os_error() == Some(libc::EISDIR) => StorageError::IsADirectory(path),
            _ if err.raw_os_error() == Some(libc::ENOTDIR) => StorageError::NotADirectory(path),
            _ if err.raw_os_error() == Some(libc::ENOTEMPTY) => {
                StorageError::DirectoryNotEmpty(path)
            }
            _ => StorageError::Io { path, source: err },
        }
    }
}

/// Result type alias for storage operations
pub type StorageResult<T> = std::result::Result<T, StorageError>;

/// Storage backend abstraction trait
///
/// All path parameters are workspace-internal relative paths, excluding the
/// workspace_id prefix. Implementations are responsible for mapping relative
/// paths to actual storage locations.
///
/// Example: `path = "src/main.rs"`
///   - `LocalStorageBackend` → `/var/lib/workspace/{workspace_id}/src/main.rs`
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    // ── File Read/Write ──

    /// Read the entire contents of a file
    async fn read_file(&self, workspace_id: &str, path: &str) -> StorageResult<Vec<u8>>;

    /// Read a range of bytes from a file (for NFS offset+count reads)
    async fn read_file_range(
        &self,
        workspace_id: &str,
        path: &str,
        offset: u64,
        length: u32,
    ) -> StorageResult<Vec<u8>>;

    /// Write file content (full overwrite; creates if missing, overwrites if exists)
    async fn write_file(&self, workspace_id: &str, path: &str, content: &[u8])
        -> StorageResult<()>;

    /// Write data at a specific offset (for NFS offset writes)
    async fn write_file_at(
        &self,
        workspace_id: &str,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> StorageResult<()>;

    // ── File Creation ──

    /// Create a file
    ///
    /// - `exclusive = true`: file must not exist, returns `AlreadyExists` otherwise
    ///   (NFS GUARDED/EXCLUSIVE mode)
    /// - `exclusive = false`: truncates if exists, creates if missing
    ///   (NFS UNCHECKED mode)
    async fn create_file(
        &self,
        workspace_id: &str,
        path: &str,
        exclusive: bool,
    ) -> StorageResult<()>;

    // ── Metadata ──

    /// Get file/directory metadata
    async fn stat(&self, workspace_id: &str, path: &str) -> StorageResult<FileStat>;

    /// List direct children of a directory
    async fn list_dir(&self, workspace_id: &str, path: &str) -> StorageResult<Vec<FileStat>>;

    /// Check whether a file or directory exists
    async fn exists(&self, workspace_id: &str, path: &str) -> StorageResult<bool>;

    // ── Directory Operations ──

    /// Create a directory
    ///
    /// - `recursive = true`: create parent directories as needed (`mkdir -p`)
    /// - `recursive = false`: create only the leaf directory; returns `NotFound`
    ///   if the parent does not exist (NFS mkdir semantics)
    async fn mkdir(&self, workspace_id: &str, path: &str, recursive: bool) -> StorageResult<()>;

    // ── Remove Operations ──

    /// Remove a file
    ///
    /// Returns `IsADirectory` if the path points to a directory.
    async fn remove_file(&self, workspace_id: &str, path: &str) -> StorageResult<()>;

    /// Remove a directory
    ///
    /// - `recursive = true`: remove directory and all contents (`rm -rf`)
    /// - `recursive = false`: directory must be empty, otherwise `DirectoryNotEmpty`
    ///
    /// Returns `NotADirectory` if the path points to a file.
    async fn remove_dir(
        &self,
        workspace_id: &str,
        path: &str,
        recursive: bool,
    ) -> StorageResult<()>;

    // ── Move/Copy ──

    /// Rename or move a file/directory
    async fn rename(&self, workspace_id: &str, src: &str, dst: &str) -> StorageResult<()>;

    /// Rename with NOREPLACE semantics (atomic, fails if destination exists)
    ///
    /// Uses `renameat2(RENAME_NOREPLACE)` on Linux for atomic operation.
    /// Returns `AlreadyExists` if the destination already exists.
    ///
    /// Default implementation falls back to stat + rename (non-atomic, has TOCTOU race).
    async fn rename_noreplace(
        &self,
        workspace_id: &str,
        src: &str,
        dst: &str,
    ) -> StorageResult<()> {
        // Default: non-atomic fallback (TOCTOU race possible)
        if self.exists(workspace_id, dst).await? {
            return Err(StorageError::AlreadyExists(dst.to_string()));
        }
        self.rename(workspace_id, src, dst).await
    }

    /// Rename with EXCHANGE semantics (atomic swap of two paths)
    ///
    /// Uses `renameat2(RENAME_EXCHANGE)` on Linux for atomic operation.
    /// Returns `NotSupported` if the filesystem doesn't support this operation.
    ///
    /// Default implementation returns `NotSupported`.
    async fn rename_exchange(&self, workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        let _ = (workspace_id, src, dst);
        Err(StorageError::NotSupported("RENAME_EXCHANGE".to_string()))
    }

    /// Copy a file or directory
    async fn copy(&self, workspace_id: &str, src: &str, dst: &str) -> StorageResult<()>;

    // ── Workspace Lifecycle ──

    /// Create the workspace root directory
    async fn create_workspace_root(&self, workspace_id: &str) -> StorageResult<()>;

    /// Delete the workspace root directory and all its contents
    async fn delete_workspace_root(&self, workspace_id: &str) -> StorageResult<()>;

    // ── NFS Extended Operations ──

    /// Set file size (truncate, used by NFS setattr)
    async fn set_file_size(&self, workspace_id: &str, path: &str, size: u64) -> StorageResult<()>;

    /// Set file permission mode (e.g., 0o644, used by NFS setattr)
    async fn set_permissions(&self, workspace_id: &str, path: &str, mode: u32)
        -> StorageResult<()>;

    /// Set file access and modification times (used by NFS setattr)
    ///
    /// Either time can be `None` to leave it unchanged.
    async fn set_times(
        &self,
        workspace_id: &str,
        path: &str,
        atime: Option<DateTime<Utc>>,
        mtime: Option<DateTime<Utc>>,
    ) -> StorageResult<()>;

    /// Create a symbolic link
    ///
    /// `target` is the symlink target path. NFS allows arbitrary target strings
    /// (relative, absolute, or non-existent paths). Only `link_path` is subject
    /// to workspace path validation; `target` is passed through as-is.
    async fn symlink(&self, workspace_id: &str, link_path: &str, target: &str)
        -> StorageResult<()>;

    /// Read the target of a symbolic link
    async fn readlink(&self, workspace_id: &str, path: &str) -> StorageResult<String>;

    /// Get filesystem statistics for a workspace
    ///
    /// Returns filesystem statistics (total/free/available space, inodes, etc.)
    /// for the underlying storage of the given workspace.
    ///
    /// Default implementation returns reasonable defaults for systems where
    /// statvfs is not available or not meaningful (e.g., object storage).
    async fn stat_fs(&self, workspace_id: &str) -> StorageResult<FsStats> {
        let _ = workspace_id;
        Ok(FsStats::default())
    }
}

/// Filesystem statistics (corresponds to POSIX statvfs)
#[derive(Debug, Clone)]
pub struct FsStats {
    /// Total number of blocks
    pub blocks: u64,
    /// Free blocks
    pub bfree: u64,
    /// Free blocks available to non-privileged users
    pub bavail: u64,
    /// Total number of file nodes (inodes)
    pub files: u64,
    /// Free file nodes
    pub ffree: u64,
    /// Filesystem block size
    pub bsize: u32,
    /// Maximum filename length
    pub namelen: u32,
    /// Fragment size (usually same as bsize)
    pub frsize: u32,
}

impl Default for FsStats {
    fn default() -> Self {
        Self {
            blocks: 1024 * 1024 * 100, // 100GB in 4K blocks
            bfree: 1024 * 1024 * 50,   // 50GB free
            bavail: 1024 * 1024 * 50,  // 50GB available
            files: 1_000_000,          // 1M total inodes
            ffree: 900_000,            // 900K free inodes
            bsize: 4096,               // 4KB block size
            namelen: 255,              // Max filename length
            frsize: 4096,              // Fragment size
        }
    }
}

/// Convert storage-layer `FileStat` to HTTP API `FileInfo`
impl From<FileStat> for FileInfo {
    fn from(stat: FileStat) -> Self {
        FileInfo {
            name: stat.name,
            path: stat.path,
            file_type: match stat.file_type {
                FileType::Directory => "directory".to_string(),
                FileType::File => "file".to_string(),
                FileType::Symlink => "symlink".to_string(),
            },
            size: stat.size,
            modified_at: stat.modified_at,
        }
    }
}
