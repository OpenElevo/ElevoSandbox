//! NFS file system management
//!
//! Provides embedded NFS server for exposing sandbox workspaces.
//! All file operations are delegated to `StorageBackend` for async, non-blocking I/O.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nfsserve::nfs::{
    fattr3, fileid3, filename3, ftype3, nfspath3, nfsstat3, nfstime3, sattr3, set_atime,
    set_mode3, set_mtime, set_size3, specdata3,
};
use nfsserve::tcp::{NFSTcp, NFSTcpListener};
use nfsserve::vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use super::storage::{FileStat, FileType, StorageBackend, StorageError};

/// NFS manager for handling file system exports
pub struct NfsManager {
    mode: NfsMode,
    port: u16,
    host: String,
    storage: Arc<dyn StorageBackend>,
    /// Map of workspace_id (used as NFS export name) -> workspace_id
    /// The storage backend knows how to resolve workspace_id to a physical path.
    exports: Arc<RwLock<HashMap<String, String>>>,
    /// Server handle (if running)
    server_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// NFS operation mode
#[derive(Debug, Clone, PartialEq)]
pub enum NfsMode {
    /// Use embedded nfsserve crate
    Embedded,
    /// Use system nfs-kernel-server
    System,
}

impl NfsManager {
    /// Create a new NFS manager
    pub fn new(mode: NfsMode, port: u16, host: String, storage: Arc<dyn StorageBackend>) -> Self {
        Self {
            mode,
            port,
            host,
            storage,
            exports: Arc::new(RwLock::new(HashMap::new())),
            server_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Start the NFS server
    pub async fn start(&self) -> anyhow::Result<()> {
        if self.mode != NfsMode::Embedded {
            info!("NFS mode is not embedded, skipping embedded server start");
            return Ok(());
        }

        let mut handle = self.server_handle.write().await;
        if handle.is_some() {
            warn!("NFS server already running");
            return Ok(());
        }

        let port = self.port;
        let exports = self.exports.clone();
        let storage = self.storage.clone();

        info!("Starting embedded NFS server on port {}", port);

        let server_task = tokio::spawn(async move {
            let fs = WorkspaceNfs::new(storage, exports);
            let addr = format!("0.0.0.0:{}", port);

            match NFSTcpListener::bind(&addr, fs).await {
                Ok(listener) => {
                    info!("NFS server listening on port {}", port);
                    if let Err(e) = listener.handle_forever().await {
                        error!("NFS server error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to start NFS server: {}", e);
                }
            }
        });

        *handle = Some(server_task);
        Ok(())
    }

    /// Stop the NFS server
    pub async fn stop(&self) {
        let mut handle = self.server_handle.write().await;
        if let Some(h) = handle.take() {
            h.abort();
            info!("NFS server stopped");
        }
    }

    /// Export a workspace
    pub async fn export(&self, workspace_id: &str) -> anyhow::Result<String> {
        let mut exports = self.exports.write().await;
        exports.insert(workspace_id.to_string(), workspace_id.to_string());

        let nfs_url = format!("nfs://{}:{}/{}", self.host, self.port, workspace_id);
        info!("Exported workspace {} at {}", workspace_id, nfs_url);

        Ok(nfs_url)
    }

    /// Unexport a workspace
    pub async fn unexport(&self, workspace_id: &str) {
        let mut exports = self.exports.write().await;
        if exports.remove(workspace_id).is_some() {
            info!("Unexported workspace {}", workspace_id);
        }
    }

    /// Get NFS URL for a workspace
    pub async fn get_nfs_url(&self, workspace_id: &str) -> Option<String> {
        let exports = self.exports.read().await;
        if exports.contains_key(workspace_id) {
            Some(format!(
                "nfs://{}:{}/{}",
                self.host, self.port, workspace_id
            ))
        } else {
            None
        }
    }

    /// Get the NFS mode
    pub fn mode(&self) -> &NfsMode {
        &self.mode
    }

    /// Get the NFS port
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Inode key: (workspace_id, relative_path)
/// For workspace root dirs, relative_path is empty string.
type InodeKey = (String, String);

/// Convert `StorageError` to NFS `nfsstat3`
fn storage_error_to_nfsstat(err: &StorageError) -> nfsstat3 {
    match err {
        StorageError::NotFound(_) => nfsstat3::NFS3ERR_NOENT,
        StorageError::AlreadyExists(_) => nfsstat3::NFS3ERR_EXIST,
        StorageError::IsADirectory(_) => nfsstat3::NFS3ERR_ISDIR,
        StorageError::NotADirectory(_) => nfsstat3::NFS3ERR_NOTDIR,
        StorageError::DirectoryNotEmpty(_) => nfsstat3::NFS3ERR_NOTEMPTY,
        StorageError::PermissionDenied(_) | StorageError::PathTraversalDenied(_) => {
            nfsstat3::NFS3ERR_ACCES
        }
        StorageError::NotSupported(_) => nfsstat3::NFS3ERR_NOTSUPP,
        StorageError::NotAFile(_) => nfsstat3::NFS3ERR_INVAL,
        StorageError::Io { .. } | StorageError::Internal(_) => nfsstat3::NFS3ERR_IO,
    }
}

/// Convert an optional `DateTime<Utc>` to NFS `nfstime3`
fn to_nfstime3(dt: Option<DateTime<Utc>>) -> nfstime3 {
    match dt {
        Some(dt) => {
            let ts = dt.timestamp();
            let nsec = dt.timestamp_subsec_nanos();
            nfstime3 {
                seconds: ts.clamp(0, u32::MAX as i64) as u32,
                nseconds: nsec,
            }
        }
        None => nfstime3 {
            seconds: 0,
            nseconds: 0,
        },
    }
}

/// Convert NFS `nfstime3` to `DateTime<Utc>`
fn nfstime3_to_datetime(t: nfstime3) -> DateTime<Utc> {
    use chrono::TimeZone;
    Utc.timestamp_opt(t.seconds as i64, t.nseconds)
        .single()
        .unwrap_or_else(Utc::now)
}

/// Convert `FileStat` to NFS `fattr3`
fn file_stat_to_fattr(stat: &FileStat, fileid: fileid3) -> fattr3 {
    let ftype = match stat.file_type {
        FileType::File => ftype3::NF3REG,
        FileType::Directory => ftype3::NF3DIR,
        FileType::Symlink => ftype3::NF3LNK,
    };

    fattr3 {
        ftype,
        mode: stat.mode,
        nlink: if stat.file_type == FileType::Directory {
            2
        } else {
            1
        },
        uid: stat.uid,
        gid: stat.gid,
        size: stat.size,
        used: stat.size,
        rdev: specdata3 {
            specdata1: 0,
            specdata2: 0,
        },
        fsid: 0,
        fileid,
        atime: to_nfstime3(stat.accessed_at),
        mtime: to_nfstime3(stat.modified_at),
        ctime: to_nfstime3(stat.modified_at),
    }
}

/// Default fattr3 for a directory (fallback when stat fails)
fn default_dir_fattr(fileid: fileid3) -> fattr3 {
    fattr3 {
        ftype: ftype3::NF3DIR,
        mode: 0o755,
        nlink: 2,
        uid: 0,
        gid: 0,
        size: 4096,
        used: 4096,
        rdev: specdata3 {
            specdata1: 0,
            specdata2: 0,
        },
        fsid: 0,
        fileid,
        atime: nfstime3 {
            seconds: 0,
            nseconds: 0,
        },
        mtime: nfstime3 {
            seconds: 0,
            nseconds: 0,
        },
        ctime: nfstime3 {
            seconds: 0,
            nseconds: 0,
        },
    }
}

/// NFS filesystem implementation backed by `StorageBackend`
///
/// The inode mapping uses `(workspace_id, relative_path)` tuples, decoupling
/// logical paths from physical storage locations.
struct WorkspaceNfs {
    storage: Arc<dyn StorageBackend>,
    exports: Arc<RwLock<HashMap<String, String>>>,
    /// File ID counter
    next_fileid: std::sync::atomic::AtomicU64,
    /// Logical path to file ID mapping
    path_to_id: RwLock<HashMap<InodeKey, fileid3>>,
    /// File ID to logical path mapping
    id_to_path: RwLock<HashMap<fileid3, InodeKey>>,
}

impl WorkspaceNfs {
    fn new(
        storage: Arc<dyn StorageBackend>,
        exports: Arc<RwLock<HashMap<String, String>>>,
    ) -> Self {
        Self {
            storage,
            exports,
            next_fileid: std::sync::atomic::AtomicU64::new(2), // 1 is reserved for root
            path_to_id: RwLock::new(HashMap::new()),
            id_to_path: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a file ID for a logical path
    async fn get_or_create_fileid(&self, key: &InodeKey) -> fileid3 {
        // Fast path: read lock
        {
            let path_to_id = self.path_to_id.read().await;
            if let Some(&id) = path_to_id.get(key) {
                return id;
            }
        }

        // Slow path: write lock
        let mut path_to_id = self.path_to_id.write().await;
        // Double-check after acquiring write lock
        if let Some(&id) = path_to_id.get(key) {
            return id;
        }

        let id = self
            .next_fileid
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        path_to_id.insert(key.clone(), id);

        let mut id_to_path = self.id_to_path.write().await;
        id_to_path.insert(id, key.clone());

        id
    }

    /// Get the logical path for a file ID
    async fn get_key_by_id(&self, id: fileid3) -> Option<InodeKey> {
        let id_to_path = self.id_to_path.read().await;
        id_to_path.get(&id).cloned()
    }

    /// Remove inode mapping for a given key
    async fn remove_inode(&self, key: &InodeKey) {
        let mut path_to_id = self.path_to_id.write().await;
        if let Some(id) = path_to_id.remove(key) {
            let mut id_to_path = self.id_to_path.write().await;
            id_to_path.remove(&id);
        }
    }

    /// Remap inode from old key to new key (for rename operations)
    async fn remap_inode(&self, old_key: &InodeKey, new_key: InodeKey) {
        let mut path_to_id = self.path_to_id.write().await;
        if let Some(id) = path_to_id.remove(old_key) {
            path_to_id.insert(new_key.clone(), id);
            let mut id_to_path = self.id_to_path.write().await;
            id_to_path.insert(id, new_key);
        }
    }

    /// Build a relative path for a child entry inside a directory.
    /// If parent_path is empty (workspace root), return just the child name.
    fn child_path(parent_path: &str, child_name: &str) -> String {
        if parent_path.is_empty() {
            child_name.to_string()
        } else {
            format!("{}/{}", parent_path, child_name)
        }
    }
}

#[async_trait]
impl NFSFileSystem for WorkspaceNfs {
    fn root_dir(&self) -> fileid3 {
        1
    }

    fn capabilities(&self) -> VFSCapabilities {
        VFSCapabilities::ReadWrite
    }

    async fn lookup(&self, dirid: fileid3, filename: &filename3) -> Result<fileid3, nfsstat3> {
        let filename_str =
            std::str::from_utf8(&filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        if dirid == 1 {
            // Root directory — look up workspace export by name
            let exports = self.exports.read().await;
            let workspace_id = exports
                .get(filename_str)
                .ok_or(nfsstat3::NFS3ERR_NOENT)?
                .clone();

            // Verify workspace root exists via storage
            self.storage
                .exists(&workspace_id, "")
                .await
                .map_err(|e| storage_error_to_nfsstat(&e))?;

            let key = (workspace_id, String::new());
            return Ok(self.get_or_create_fileid(&key).await);
        }

        // Non-root directory: resolve parent and build child path
        let (workspace_id, parent_path) =
            self.get_key_by_id(dirid).await.ok_or(nfsstat3::NFS3ERR_STALE)?;

        let child_rel = Self::child_path(&parent_path, filename_str);

        // Verify the child exists
        let exists = self
            .storage
            .exists(&workspace_id, &child_rel)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        if !exists {
            return Err(nfsstat3::NFS3ERR_NOENT);
        }

        let key = (workspace_id, child_rel);
        Ok(self.get_or_create_fileid(&key).await)
    }

    async fn getattr(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        if id == 1 {
            // Root directory: synthetic attributes
            return Ok(default_dir_fattr(1));
        }

        let (workspace_id, rel_path) = self
            .get_key_by_id(id)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        // stat the workspace root if rel_path is empty, otherwise stat the file
        let stat = if rel_path.is_empty() {
            // Workspace root — stat "." by checking if it exists and getting default dir attrs
            match self.storage.stat(&workspace_id, ".").await {
                Ok(s) => s,
                Err(_) => {
                    // Fallback: workspace root is always a directory
                    return Ok(default_dir_fattr(id));
                }
            }
        } else {
            self.storage
                .stat(&workspace_id, &rel_path)
                .await
                .map_err(|e| storage_error_to_nfsstat(&e))?
        };

        Ok(file_stat_to_fattr(&stat, id))
    }

    async fn setattr(&self, id: fileid3, setattr: sattr3) -> Result<fattr3, nfsstat3> {
        let (workspace_id, rel_path) = self
            .get_key_by_id(id)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        // Handle size truncation
        if let set_size3::size(size) = setattr.size {
            self.storage
                .set_file_size(&workspace_id, &rel_path, size)
                .await
                .map_err(|e| storage_error_to_nfsstat(&e))?;
        }

        // Handle permission mode change.
        //
        // NOTE: uid/gid changes (setattr.uid, setattr.gid) are intentionally ignored.
        // Reasons:
        // 1. In S3 mode via s3fs-fuse, chown operations have very limited effect —
        //    uid/gid are stored as S3 object metadata headers, which is slow and
        //    may not be respected by all S3-compatible backends.
        // 2. Most workspace use cases don't require changing file ownership.
        // 3. The StorageBackend trait doesn't expose set_uid/set_gid methods.
        //
        // If uid/gid support is needed in the future, consider:
        // - Adding set_owner(uid, gid) to StorageBackend trait
        // - Using libc::chown via spawn_blocking in LocalStorageBackend
        if let set_mode3::mode(mode) = setattr.mode {
            self.storage
                .set_permissions(&workspace_id, &rel_path, mode)
                .await
                .map_err(|e| storage_error_to_nfsstat(&e))?;
        }

        // Handle timestamp changes
        let atime = match setattr.atime {
            set_atime::SET_TO_CLIENT_TIME(t) => Some(nfstime3_to_datetime(t)),
            set_atime::SET_TO_SERVER_TIME => Some(Utc::now()),
            set_atime::DONT_CHANGE => None,
        };
        let mtime = match setattr.mtime {
            set_mtime::SET_TO_CLIENT_TIME(t) => Some(nfstime3_to_datetime(t)),
            set_mtime::SET_TO_SERVER_TIME => Some(Utc::now()),
            set_mtime::DONT_CHANGE => None,
        };
        if atime.is_some() || mtime.is_some() {
            self.storage
                .set_times(&workspace_id, &rel_path, atime, mtime)
                .await
                .map_err(|e| storage_error_to_nfsstat(&e))?;
        }

        // Return updated attributes
        let stat = self
            .storage
            .stat(&workspace_id, &rel_path)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        Ok(file_stat_to_fattr(&stat, id))
    }

    async fn read(
        &self,
        id: fileid3,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), nfsstat3> {
        let (workspace_id, rel_path) = self
            .get_key_by_id(id)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        let data = self
            .storage
            .read_file_range(&workspace_id, &rel_path, offset, count)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        // Determine EOF: if we read less than requested, we're at/past the end
        let eof = (data.len() as u32) < count;

        Ok((data, eof))
    }

    async fn write(&self, id: fileid3, offset: u64, data: &[u8]) -> Result<fattr3, nfsstat3> {
        let (workspace_id, rel_path) = self
            .get_key_by_id(id)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        self.storage
            .write_file_at(&workspace_id, &rel_path, offset, data)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        // Return updated attributes
        let stat = self
            .storage
            .stat(&workspace_id, &rel_path)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        Ok(file_stat_to_fattr(&stat, id))
    }

    async fn create(
        &self,
        dirid: fileid3,
        filename: &filename3,
        _setattr: sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let (workspace_id, parent_path) = self
            .get_key_by_id(dirid)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        let filename_str =
            std::str::from_utf8(&filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        let child_rel = Self::child_path(&parent_path, filename_str);

        // UNCHECKED create: truncate if exists, create if missing
        self.storage
            .create_file(&workspace_id, &child_rel, false)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        let key = (workspace_id.clone(), child_rel.clone());
        let id = self.get_or_create_fileid(&key).await;

        let stat = self
            .storage
            .stat(&workspace_id, &child_rel)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        Ok((id, file_stat_to_fattr(&stat, id)))
    }

    async fn create_exclusive(
        &self,
        dirid: fileid3,
        filename: &filename3,
    ) -> Result<fileid3, nfsstat3> {
        let (workspace_id, parent_path) = self
            .get_key_by_id(dirid)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        let filename_str =
            std::str::from_utf8(&filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        let child_rel = Self::child_path(&parent_path, filename_str);

        // EXCLUSIVE create: must not exist
        self.storage
            .create_file(&workspace_id, &child_rel, true)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        let key = (workspace_id, child_rel);
        Ok(self.get_or_create_fileid(&key).await)
    }

    async fn remove(&self, dirid: fileid3, filename: &filename3) -> Result<(), nfsstat3> {
        let (workspace_id, parent_path) = self
            .get_key_by_id(dirid)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        let filename_str =
            std::str::from_utf8(&filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        let child_rel = Self::child_path(&parent_path, filename_str);

        // Determine if it's a file or directory and call appropriate method
        let stat = self
            .storage
            .stat(&workspace_id, &child_rel)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        if stat.file_type == FileType::Directory {
            self.storage
                .remove_dir(&workspace_id, &child_rel, false)
                .await
                .map_err(|e| storage_error_to_nfsstat(&e))?;
        } else {
            self.storage
                .remove_file(&workspace_id, &child_rel)
                .await
                .map_err(|e| storage_error_to_nfsstat(&e))?;
        }

        // Clean up inode mapping for the removed entry
        let key = (workspace_id, child_rel);
        self.remove_inode(&key).await;

        Ok(())
    }

    async fn rename(
        &self,
        from_dirid: fileid3,
        from_filename: &filename3,
        to_dirid: fileid3,
        to_filename: &filename3,
    ) -> Result<(), nfsstat3> {
        let (from_ws, from_parent) = self
            .get_key_by_id(from_dirid)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;
        let (to_ws, to_parent) = self
            .get_key_by_id(to_dirid)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        // Rename across workspaces is not supported
        if from_ws != to_ws {
            return Err(nfsstat3::NFS3ERR_NOTSUPP);
        }

        let from_name =
            std::str::from_utf8(&from_filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;
        let to_name =
            std::str::from_utf8(&to_filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        let from_rel = Self::child_path(&from_parent, from_name);
        let to_rel = Self::child_path(&to_parent, to_name);

        self.storage
            .rename(&from_ws, &from_rel, &to_rel)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        // Remap inode from old path to new path
        let old_key = (from_ws, from_rel);
        let new_key = (to_ws, to_rel);
        self.remap_inode(&old_key, new_key).await;

        Ok(())
    }

    async fn mkdir(
        &self,
        dirid: fileid3,
        dirname: &filename3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let (workspace_id, parent_path) = self
            .get_key_by_id(dirid)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        let dirname_str =
            std::str::from_utf8(&dirname.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        let child_rel = Self::child_path(&parent_path, dirname_str);

        // NFS mkdir: non-recursive (single level)
        self.storage
            .mkdir(&workspace_id, &child_rel, false)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        let key = (workspace_id.clone(), child_rel.clone());
        let id = self.get_or_create_fileid(&key).await;

        let stat = self
            .storage
            .stat(&workspace_id, &child_rel)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        Ok((id, file_stat_to_fattr(&stat, id)))
    }

    async fn readdir(
        &self,
        dirid: fileid3,
        start_after: fileid3,
        max_entries: usize,
    ) -> Result<ReadDirResult, nfsstat3> {
        if dirid == 1 {
            // Root directory: list exported workspaces
            let exports = self.exports.read().await;
            // Sort exports by name for deterministic ordering
            let mut sorted_exports: Vec<_> = exports.iter().collect();
            sorted_exports.sort_by(|(a, _), (b, _)| a.cmp(b));

            let mut entries = Vec::new();
            // start_after is used as a 1-based index cookie (0 means start from beginning)
            let skip = start_after as usize;

            for (export_name, workspace_id) in sorted_exports.iter().skip(skip) {
                let key = (workspace_id.to_string(), String::new());
                let id = self.get_or_create_fileid(&key).await;

                let attr = match self.getattr(id).await {
                    Ok(a) => a,
                    Err(_) => default_dir_fattr(id),
                };
                entries.push(DirEntry {
                    fileid: id,
                    name: export_name.as_bytes().to_vec().into(),
                    attr,
                });

                if entries.len() >= max_entries {
                    let end = skip + entries.len() >= sorted_exports.len();
                    return Ok(ReadDirResult { entries, end });
                }
            }

            return Ok(ReadDirResult { entries, end: true });
        }

        // Non-root: list directory contents via storage backend
        let (workspace_id, rel_path) = self
            .get_key_by_id(dirid)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        let dir_path = if rel_path.is_empty() { "." } else { &rel_path };

        let file_stats = self
            .storage
            .list_dir(&workspace_id, dir_path)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        let mut entries = Vec::new();
        // start_after is used as a 1-based index cookie (0 means start from beginning)
        let skip = start_after as usize;
        let total = file_stats.len();

        for stat in file_stats.into_iter().skip(skip) {
            let child_rel = Self::child_path(&rel_path, &stat.name);
            let key = (workspace_id.clone(), child_rel);
            let id = self.get_or_create_fileid(&key).await;

            let attr = file_stat_to_fattr(&stat, id);
            entries.push(DirEntry {
                fileid: id,
                name: stat.name.as_bytes().to_vec().into(),
                attr,
            });

            if entries.len() >= max_entries {
                let end = skip + entries.len() >= total;
                return Ok(ReadDirResult { entries, end });
            }
        }

        Ok(ReadDirResult { entries, end: true })
    }

    async fn symlink(
        &self,
        dirid: fileid3,
        linkname: &filename3,
        symlink: &nfspath3,
        _attr: &sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let (workspace_id, parent_path) = self
            .get_key_by_id(dirid)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        let linkname_str =
            std::str::from_utf8(&linkname.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;
        let target_str =
            std::str::from_utf8(&symlink.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        let link_rel = Self::child_path(&parent_path, linkname_str);

        self.storage
            .symlink(&workspace_id, &link_rel, target_str)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        let key = (workspace_id.clone(), link_rel.clone());
        let id = self.get_or_create_fileid(&key).await;

        let stat = self
            .storage
            .stat(&workspace_id, &link_rel)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        Ok((id, file_stat_to_fattr(&stat, id)))
    }

    async fn readlink(&self, id: fileid3) -> Result<nfspath3, nfsstat3> {
        let (workspace_id, rel_path) = self
            .get_key_by_id(id)
            .await
            .ok_or(nfsstat3::NFS3ERR_STALE)?;

        let target = self
            .storage
            .readlink(&workspace_id, &rel_path)
            .await
            .map_err(|e| storage_error_to_nfsstat(&e))?;

        let target_bytes = target.as_bytes().to_vec();
        Ok(target_bytes.into())
    }
}
