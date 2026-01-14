//! NFS file system management
//!
//! Provides embedded NFS server for exposing sandbox workspaces.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write as IoWrite};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use nfsserve::nfs::{
    fattr3, fileid3, filename3, ftype3, nfsstat3, nfstime3, sattr3, set_size3, nfspath3,
};
use nfsserve::tcp::{NFSTcp, NFSTcpListener};
use nfsserve::vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// NFS manager for handling file system exports
pub struct NfsManager {
    mode: NfsMode,
    base_dir: PathBuf,
    port: u16,
    /// Map of sandbox_id -> exported path
    exports: Arc<RwLock<HashMap<String, PathBuf>>>,
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
    pub fn new(mode: NfsMode, base_dir: PathBuf, port: u16) -> Self {
        Self {
            mode,
            base_dir,
            port,
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

        let base_dir = self.base_dir.clone();
        let port = self.port;
        let exports = self.exports.clone();

        info!("Starting embedded NFS server on port {}", port);

        let server_task = tokio::spawn(async move {
            let fs = WorkspaceNfs::new(base_dir, exports);
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

    /// Export a sandbox workspace
    pub async fn export(&self, sandbox_id: &str, workspace_path: &Path) -> anyhow::Result<String> {
        let mut exports = self.exports.write().await;
        exports.insert(sandbox_id.to_string(), workspace_path.to_path_buf());

        let nfs_url = format!("nfs://127.0.0.1:{}/{}", self.port, sandbox_id);
        info!("Exported sandbox {} at {}", sandbox_id, nfs_url);

        Ok(nfs_url)
    }

    /// Unexport a sandbox workspace
    pub async fn unexport(&self, sandbox_id: &str) {
        let mut exports = self.exports.write().await;
        if exports.remove(sandbox_id).is_some() {
            info!("Unexported sandbox {}", sandbox_id);
        }
    }

    /// Get NFS URL for a sandbox
    pub async fn get_nfs_url(&self, sandbox_id: &str) -> Option<String> {
        let exports = self.exports.read().await;
        if exports.contains_key(sandbox_id) {
            Some(format!("nfs://127.0.0.1:{}/{}", self.port, sandbox_id))
        } else {
            None
        }
    }

    /// Get the NFS mode
    pub fn mode(&self) -> &NfsMode {
        &self.mode
    }

    /// Get the base directory
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Get the NFS port
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// NFS filesystem implementation for workspace access
struct WorkspaceNfs {
    base_dir: PathBuf,
    exports: Arc<RwLock<HashMap<String, PathBuf>>>,
    /// File ID counter
    next_fileid: std::sync::atomic::AtomicU64,
    /// Path to file ID mapping
    path_to_id: std::sync::RwLock<HashMap<PathBuf, fileid3>>,
    /// File ID to path mapping
    id_to_path: std::sync::RwLock<HashMap<fileid3, PathBuf>>,
}

impl WorkspaceNfs {
    fn new(base_dir: PathBuf, exports: Arc<RwLock<HashMap<String, PathBuf>>>) -> Self {
        Self {
            base_dir,
            exports,
            next_fileid: std::sync::atomic::AtomicU64::new(2), // 1 is reserved for root
            path_to_id: std::sync::RwLock::new(HashMap::new()),
            id_to_path: std::sync::RwLock::new(HashMap::new()),
        }
    }

    fn get_or_create_fileid(&self, path: &Path) -> fileid3 {
        let mut path_to_id = self.path_to_id.write().unwrap();
        if let Some(&id) = path_to_id.get(path) {
            return id;
        }

        let id = self.next_fileid.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        path_to_id.insert(path.to_path_buf(), id);

        let mut id_to_path = self.id_to_path.write().unwrap();
        id_to_path.insert(id, path.to_path_buf());

        id
    }

    fn get_path_by_id(&self, id: fileid3) -> Option<PathBuf> {
        if id == 1 {
            return Some(self.base_dir.clone());
        }
        let id_to_path = self.id_to_path.read().unwrap();
        id_to_path.get(&id).cloned()
    }

    fn metadata_to_fattr(&self, metadata: &std::fs::Metadata, fileid: fileid3) -> fattr3 {
        let ftype = if metadata.is_dir() {
            ftype3::NF3DIR
        } else if metadata.is_symlink() {
            ftype3::NF3LNK
        } else {
            ftype3::NF3REG
        };

        let mode = metadata.mode();
        let nlink = metadata.nlink() as u32;
        let uid = metadata.uid();
        let gid = metadata.gid();
        let size = metadata.len();
        let used = metadata.blocks() * 512;

        let atime = metadata.accessed().ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| nfstime3 { seconds: d.as_secs() as u32, nseconds: d.subsec_nanos() })
            .unwrap_or(nfstime3 { seconds: 0, nseconds: 0 });

        let mtime = metadata.modified().ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| nfstime3 { seconds: d.as_secs() as u32, nseconds: d.subsec_nanos() })
            .unwrap_or(nfstime3 { seconds: 0, nseconds: 0 });

        let ctime = nfstime3 {
            seconds: metadata.ctime() as u32,
            nseconds: metadata.ctime_nsec() as u32,
        };

        fattr3 {
            ftype,
            mode,
            nlink,
            uid,
            gid,
            size,
            used,
            rdev: nfsserve::nfs::specdata3 { specdata1: 0, specdata2: 0 },
            fsid: 0,
            fileid,
            atime,
            mtime,
            ctime,
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
        let dir_path = self.get_path_by_id(dirid).ok_or(nfsstat3::NFS3ERR_STALE)?;

        let filename_str = std::str::from_utf8(&filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        let target_path = if dirid == 1 {
            // Root directory - sandbox_id lookup
            let exports = self.exports.read().await;
            exports.get(filename_str)
                .cloned()
                .ok_or(nfsstat3::NFS3ERR_NOENT)?
        } else {
            dir_path.join(filename_str)
        };

        if !target_path.exists() {
            return Err(nfsstat3::NFS3ERR_NOENT);
        }

        Ok(self.get_or_create_fileid(&target_path))
    }

    async fn getattr(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        if id == 1 {
            // Root directory attributes
            let metadata = std::fs::metadata(&self.base_dir).map_err(|_| nfsstat3::NFS3ERR_IO)?;
            return Ok(self.metadata_to_fattr(&metadata, id));
        }

        let path = self.get_path_by_id(id).ok_or(nfsstat3::NFS3ERR_STALE)?;
        let metadata = std::fs::metadata(&path).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        Ok(self.metadata_to_fattr(&metadata, id))
    }

    async fn setattr(&self, id: fileid3, setattr: sattr3) -> Result<fattr3, nfsstat3> {
        let path = self.get_path_by_id(id).ok_or(nfsstat3::NFS3ERR_STALE)?;

        // Handle size truncation
        if let set_size3::size(size) = setattr.size {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|_| nfsstat3::NFS3ERR_IO)?;
            file.set_len(size).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }

        // Get updated attributes
        let metadata = std::fs::metadata(&path).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        Ok(self.metadata_to_fattr(&metadata, id))
    }

    async fn read(&self, id: fileid3, offset: u64, count: u32) -> Result<(Vec<u8>, bool), nfsstat3> {
        let path = self.get_path_by_id(id).ok_or(nfsstat3::NFS3ERR_STALE)?;

        let mut file = std::fs::File::open(&path).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        file.seek(SeekFrom::Start(offset)).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let mut buffer = vec![0u8; count as usize];
        let bytes_read = file.read(&mut buffer).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        buffer.truncate(bytes_read);

        let metadata = file.metadata().map_err(|_| nfsstat3::NFS3ERR_IO)?;
        let eof = offset + bytes_read as u64 >= metadata.len();

        Ok((buffer, eof))
    }

    async fn write(&self, id: fileid3, offset: u64, data: &[u8]) -> Result<fattr3, nfsstat3> {
        let path = self.get_path_by_id(id).ok_or(nfsstat3::NFS3ERR_STALE)?;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|_| nfsstat3::NFS3ERR_IO)?;

        file.seek(SeekFrom::Start(offset)).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        file.write_all(data).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let metadata = file.metadata().map_err(|_| nfsstat3::NFS3ERR_IO)?;
        Ok(self.metadata_to_fattr(&metadata, id))
    }

    async fn create(
        &self,
        dirid: fileid3,
        filename: &filename3,
        _setattr: sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let dir_path = self.get_path_by_id(dirid).ok_or(nfsstat3::NFS3ERR_STALE)?;
        let filename_str = std::str::from_utf8(&filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;
        let file_path = dir_path.join(filename_str);

        std::fs::File::create(&file_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let id = self.get_or_create_fileid(&file_path);
        let metadata = std::fs::metadata(&file_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        Ok((id, self.metadata_to_fattr(&metadata, id)))
    }

    async fn create_exclusive(&self, dirid: fileid3, filename: &filename3) -> Result<fileid3, nfsstat3> {
        let dir_path = self.get_path_by_id(dirid).ok_or(nfsstat3::NFS3ERR_STALE)?;
        let filename_str = std::str::from_utf8(&filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;
        let file_path = dir_path.join(filename_str);

        if file_path.exists() {
            return Err(nfsstat3::NFS3ERR_EXIST);
        }

        std::fs::File::create(&file_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        Ok(self.get_or_create_fileid(&file_path))
    }

    async fn remove(&self, dirid: fileid3, filename: &filename3) -> Result<(), nfsstat3> {
        let dir_path = self.get_path_by_id(dirid).ok_or(nfsstat3::NFS3ERR_STALE)?;
        let filename_str = std::str::from_utf8(&filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;
        let file_path = dir_path.join(filename_str);

        // Check if it's a directory or file
        let metadata = std::fs::metadata(&file_path).map_err(|_| nfsstat3::NFS3ERR_NOENT)?;
        if metadata.is_dir() {
            std::fs::remove_dir(&file_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        } else {
            std::fs::remove_file(&file_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        }
        Ok(())
    }

    async fn rename(
        &self,
        from_dirid: fileid3,
        from_filename: &filename3,
        to_dirid: fileid3,
        to_filename: &filename3,
    ) -> Result<(), nfsstat3> {
        let from_dir = self.get_path_by_id(from_dirid).ok_or(nfsstat3::NFS3ERR_STALE)?;
        let to_dir = self.get_path_by_id(to_dirid).ok_or(nfsstat3::NFS3ERR_STALE)?;

        let from_name = std::str::from_utf8(&from_filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;
        let to_name = std::str::from_utf8(&to_filename.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        let from_path = from_dir.join(from_name);
        let to_path = to_dir.join(to_name);

        std::fs::rename(&from_path, &to_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        Ok(())
    }

    async fn mkdir(&self, dirid: fileid3, dirname: &filename3) -> Result<(fileid3, fattr3), nfsstat3> {
        let dir_path = self.get_path_by_id(dirid).ok_or(nfsstat3::NFS3ERR_STALE)?;
        let dirname_str = std::str::from_utf8(&dirname.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;
        let new_dir_path = dir_path.join(dirname_str);

        std::fs::create_dir(&new_dir_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let id = self.get_or_create_fileid(&new_dir_path);
        let metadata = std::fs::metadata(&new_dir_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        Ok((id, self.metadata_to_fattr(&metadata, id)))
    }

    async fn readdir(
        &self,
        dirid: fileid3,
        start_after: fileid3,
        max_entries: usize,
    ) -> Result<ReadDirResult, nfsstat3> {
        if dirid == 1 {
            // Root directory - list exported sandboxes
            let exports = self.exports.read().await;
            let mut entries = Vec::new();

            for (sandbox_id, path) in exports.iter() {
                let id = self.get_or_create_fileid(path);
                if start_after == 0 || id > start_after {
                    let attr = match self.getattr(id).await {
                        Ok(a) => a,
                        Err(_) => fattr3 {
                            ftype: ftype3::NF3DIR,
                            mode: 0o755,
                            nlink: 2,
                            uid: 0,
                            gid: 0,
                            size: 4096,
                            used: 4096,
                            rdev: nfsserve::nfs::specdata3 { specdata1: 0, specdata2: 0 },
                            fsid: 0,
                            fileid: id,
                            atime: nfstime3 { seconds: 0, nseconds: 0 },
                            mtime: nfstime3 { seconds: 0, nseconds: 0 },
                            ctime: nfstime3 { seconds: 0, nseconds: 0 },
                        },
                    };
                    entries.push(DirEntry {
                        fileid: id,
                        name: sandbox_id.as_bytes().to_vec().into(),
                        attr,
                    });
                }
                if entries.len() >= max_entries {
                    break;
                }
            }

            return Ok(ReadDirResult {
                entries,
                end: true,
            });
        }

        let dir_path = self.get_path_by_id(dirid).ok_or(nfsstat3::NFS3ERR_STALE)?;

        let mut entries = Vec::new();
        let read_dir = std::fs::read_dir(&dir_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        for entry in read_dir {
            let entry = entry.map_err(|_| nfsstat3::NFS3ERR_IO)?;
            let path = entry.path();
            let id = self.get_or_create_fileid(&path);

            if start_after == 0 || id > start_after {
                let metadata = entry.metadata().map_err(|_| nfsstat3::NFS3ERR_IO)?;
                entries.push(DirEntry {
                    fileid: id,
                    name: entry.file_name().as_encoded_bytes().to_vec().into(),
                    attr: self.metadata_to_fattr(&metadata, id),
                });
            }

            if entries.len() >= max_entries {
                break;
            }
        }

        Ok(ReadDirResult {
            entries,
            end: true,
        })
    }

    async fn symlink(
        &self,
        dirid: fileid3,
        linkname: &filename3,
        symlink: &nfspath3,
        _attr: &sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let dir_path = self.get_path_by_id(dirid).ok_or(nfsstat3::NFS3ERR_STALE)?;
        let linkname_str = std::str::from_utf8(&linkname.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;
        let target_str = std::str::from_utf8(&symlink.0).map_err(|_| nfsstat3::NFS3ERR_INVAL)?;

        let link_path = dir_path.join(linkname_str);

        #[cfg(unix)]
        std::os::unix::fs::symlink(target_str, &link_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        let id = self.get_or_create_fileid(&link_path);
        let metadata = std::fs::symlink_metadata(&link_path).map_err(|_| nfsstat3::NFS3ERR_IO)?;

        Ok((id, self.metadata_to_fattr(&metadata, id)))
    }

    async fn readlink(&self, id: fileid3) -> Result<nfspath3, nfsstat3> {
        let path = self.get_path_by_id(id).ok_or(nfsstat3::NFS3ERR_STALE)?;
        let target = std::fs::read_link(&path).map_err(|_| nfsstat3::NFS3ERR_IO)?;
        let target_bytes = target.as_os_str().as_encoded_bytes().to_vec();

        Ok(target_bytes.into())
    }
}
