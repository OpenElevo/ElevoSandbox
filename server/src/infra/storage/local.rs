//! Local filesystem storage backend
//!
//! Implements `StorageBackend` using `tokio::fs` async operations. Serves both
//! local disk mode and S3 mode (where the base directory is an s3fs-fuse mount point).

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use super::{FileStat, FileType, StorageBackend, StorageError, StorageResult};

/// Local filesystem storage backend
///
/// All operations go through `tokio::fs` (non-blocking). In local mode the
/// `base_dir` is a regular directory on disk; in S3 mode it is the s3fs-fuse
/// mount point — the code path is identical.
pub struct LocalStorageBackend {
    /// Workspace root directory (e.g., `/var/lib/workspace`)
    base_dir: PathBuf,
}

impl LocalStorageBackend {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Resolve a workspace-relative path to an absolute filesystem path.
    ///
    /// Two-layer security:
    /// 1. **Component inspection**: reject `..`, absolute prefixes, root dirs
    /// 2. **Canonicalize verification**: resolved path must remain under workspace dir
    fn resolve_path(&self, workspace_id: &str, path: &str) -> StorageResult<PathBuf> {
        // Layer 1: reject dangerous path components immediately
        for component in Path::new(path).components() {
            match component {
                Component::ParentDir => {
                    return Err(StorageError::PathTraversalDenied(format!(
                        "path contains '..': {}",
                        path
                    )));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(StorageError::PathTraversalDenied(format!(
                        "path contains absolute component: {}",
                        path
                    )));
                }
                _ => {}
            }
        }

        let workspace_dir = self.base_dir.join(workspace_id);
        let full_path = workspace_dir.join(path);

        // Layer 2: canonicalize both paths and verify containment.
        // If canonicalize fails (e.g., path doesn't exist yet), fall back to the
        // raw path — the component check above already guards against traversal.
        let canonical_workspace = workspace_dir
            .canonicalize()
            .unwrap_or(workspace_dir.clone());
        let canonical_full = full_path.canonicalize().unwrap_or(full_path.clone());
        if !canonical_full.starts_with(&canonical_workspace) {
            return Err(StorageError::PathTraversalDenied(format!(
                "resolved path escapes workspace: {}",
                path
            )));
        }

        Ok(full_path)
    }

    /// Resolve the workspace root directory path (no sub-path)
    fn workspace_dir(&self, workspace_id: &str) -> PathBuf {
        self.base_dir.join(workspace_id)
    }

    /// Build a `FileStat` from tokio metadata
    fn build_file_stat(path: &str, name: &str, metadata: &std::fs::Metadata) -> FileStat {
        use std::os::unix::fs::MetadataExt;

        let file_type = if metadata.is_dir() {
            FileType::Directory
        } else if metadata.file_type().is_symlink() {
            FileType::Symlink
        } else {
            FileType::File
        };

        FileStat {
            name: name.to_string(),
            path: path.to_string(),
            file_type,
            size: metadata.len(),
            mode: metadata.mode(),
            uid: metadata.uid(),
            gid: metadata.gid(),
            modified_at: metadata.modified().ok().map(DateTime::<Utc>::from),
            accessed_at: metadata.accessed().ok().map(DateTime::<Utc>::from),
            created_at: metadata.created().ok().map(DateTime::<Utc>::from),
        }
    }
}

#[async_trait]
impl StorageBackend for LocalStorageBackend {
    // ── File Read/Write ──

    async fn read_file(&self, workspace_id: &str, path: &str) -> StorageResult<Vec<u8>> {
        let full_path = self.resolve_path(workspace_id, path)?;
        fs::read(&full_path)
            .await
            .map_err(|e| StorageError::from_io(e, path))
    }

    async fn read_file_range(
        &self,
        workspace_id: &str,
        path: &str,
        offset: u64,
        length: u32,
    ) -> StorageResult<Vec<u8>> {
        let full_path = self.resolve_path(workspace_id, path)?;
        let mut file = fs::File::open(&full_path)
            .await
            .map_err(|e| StorageError::from_io(e, path))?;

        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| StorageError::from_io(e, path))?;

        let mut buffer = vec![0u8; length as usize];
        let bytes_read = file
            .read(&mut buffer)
            .await
            .map_err(|e| StorageError::from_io(e, path))?;
        buffer.truncate(bytes_read);

        Ok(buffer)
    }

    async fn write_file(
        &self,
        workspace_id: &str,
        path: &str,
        content: &[u8],
    ) -> StorageResult<()> {
        let full_path = self.resolve_path(workspace_id, path)?;

        // Auto-create parent directories
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::from_io(e, path))?;

            // Allow sandbox containers (which may run as a different UID) to read/write
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o777);
                let _ = fs::set_permissions(parent, perms).await;
            }
        }

        fs::write(&full_path, content)
            .await
            .map_err(|e| StorageError::from_io(e, path))
    }

    async fn write_file_at(
        &self,
        workspace_id: &str,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> StorageResult<()> {
        let full_path = self.resolve_path(workspace_id, path)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(&full_path)
            .await
            .map_err(|e| StorageError::from_io(e, path))?;

        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| StorageError::from_io(e, path))?;
        file.write_all(data)
            .await
            .map_err(|e| StorageError::from_io(e, path))?;

        Ok(())
    }

    // ── File Creation ──

    async fn create_file(
        &self,
        workspace_id: &str,
        path: &str,
        exclusive: bool,
    ) -> StorageResult<()> {
        let full_path = self.resolve_path(workspace_id, path)?;

        if exclusive {
            // GUARDED/EXCLUSIVE: must not exist
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&full_path)
                .await
                .map_err(|e| StorageError::from_io(e, path))?;
        } else {
            // UNCHECKED: truncate if exists, create if missing
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&full_path)
                .await
                .map_err(|e| StorageError::from_io(e, path))?;
        }

        Ok(())
    }

    // ── Metadata ──

    async fn stat(&self, workspace_id: &str, path: &str) -> StorageResult<FileStat> {
        let full_path = self.resolve_path(workspace_id, path)?;

        // Use symlink_metadata to avoid following symlinks (needed for NFS)
        let metadata = fs::symlink_metadata(&full_path)
            .await
            .map_err(|e| StorageError::from_io(e, path))?;

        let name = full_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Convert std::fs::Metadata (tokio returns the same type)
        let metadata_std: std::fs::Metadata = metadata;
        Ok(Self::build_file_stat(path, &name, &metadata_std))
    }

    async fn list_dir(&self, workspace_id: &str, path: &str) -> StorageResult<Vec<FileStat>> {
        let full_path = self.resolve_path(workspace_id, path)?;

        let mut entries = fs::read_dir(&full_path)
            .await
            .map_err(|e| StorageError::from_io(e, path))?;

        let mut results = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| StorageError::from_io(e, path))?
        {
            let entry_name = entry.file_name().to_string_lossy().to_string();
            let entry_rel_path = if path.is_empty() || path == "." {
                entry_name.clone()
            } else {
                format!("{}/{}", path, entry_name)
            };

            // Use symlink_metadata for correct symlink handling
            let metadata = fs::symlink_metadata(entry.path())
                .await
                .map_err(|e| StorageError::from_io(e, &entry_rel_path))?;

            results.push(Self::build_file_stat(
                &entry_rel_path,
                &entry_name,
                &metadata,
            ));
        }

        // Sort by name for deterministic output
        results.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(results)
    }

    async fn exists(&self, workspace_id: &str, path: &str) -> StorageResult<bool> {
        let full_path = self.resolve_path(workspace_id, path)?;
        Ok(fs::symlink_metadata(&full_path).await.is_ok())
    }

    // ── Directory Operations ──

    async fn mkdir(&self, workspace_id: &str, path: &str, recursive: bool) -> StorageResult<()> {
        let full_path = self.resolve_path(workspace_id, path)?;

        if recursive {
            fs::create_dir_all(&full_path)
                .await
                .map_err(|e| StorageError::from_io(e, path))?;
        } else {
            fs::create_dir(&full_path)
                .await
                .map_err(|e| StorageError::from_io(e, path))?;
        }

        // Allow sandbox containers (which may run as a different UID) to read/write
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o777);
            fs::set_permissions(&full_path, perms)
                .await
                .map_err(|e| StorageError::from_io(e, path))?;
        }

        Ok(())
    }

    // ── Remove Operations ──

    async fn remove_file(&self, workspace_id: &str, path: &str) -> StorageResult<()> {
        let full_path = self.resolve_path(workspace_id, path)?;

        // Check if target is a directory first — return IsADirectory
        if let Ok(metadata) = fs::symlink_metadata(&full_path).await {
            if metadata.is_dir() {
                return Err(StorageError::IsADirectory(path.to_string()));
            }
        }

        fs::remove_file(&full_path)
            .await
            .map_err(|e| StorageError::from_io(e, path))
    }

    async fn remove_dir(
        &self,
        workspace_id: &str,
        path: &str,
        recursive: bool,
    ) -> StorageResult<()> {
        let full_path = self.resolve_path(workspace_id, path)?;

        // Check if target is a file — return NotADirectory
        if let Ok(metadata) = fs::symlink_metadata(&full_path).await {
            if !metadata.is_dir() {
                return Err(StorageError::NotADirectory(path.to_string()));
            }
        }

        if recursive {
            fs::remove_dir_all(&full_path)
                .await
                .map_err(|e| StorageError::from_io(e, path))
        } else {
            fs::remove_dir(&full_path)
                .await
                .map_err(|e| StorageError::from_io(e, path))
        }
    }

    // ── Move/Copy ──

    async fn rename(&self, workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        let src_path = self.resolve_path(workspace_id, src)?;
        let dst_path = self.resolve_path(workspace_id, dst)?;

        fs::rename(&src_path, &dst_path)
            .await
            .map_err(|e| StorageError::from_io(e, src))
    }

    async fn copy(&self, workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        let src_path = self.resolve_path(workspace_id, src)?;
        let dst_path = self.resolve_path(workspace_id, dst)?;

        // Auto-create parent directories for destination
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::from_io(e, dst))?;
        }

        let metadata = fs::symlink_metadata(&src_path)
            .await
            .map_err(|e| StorageError::from_io(e, src))?;

        if metadata.is_dir() {
            copy_dir_recursive(&src_path, &dst_path, src).await
        } else {
            fs::copy(&src_path, &dst_path)
                .await
                .map_err(|e| StorageError::from_io(e, src))?;
            Ok(())
        }
    }

    // ── Workspace Lifecycle ──

    async fn create_workspace_root(&self, workspace_id: &str) -> StorageResult<()> {
        let workspace_dir = self.workspace_dir(workspace_id);
        fs::create_dir_all(&workspace_dir)
            .await
            .map_err(|e| StorageError::from_io(e, workspace_id))?;

        // Allow sandbox containers (which may run as a different UID) to read/write
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o777);
            fs::set_permissions(&workspace_dir, perms)
                .await
                .map_err(|e| StorageError::from_io(e, workspace_id))?;
        }

        Ok(())
    }

    async fn delete_workspace_root(&self, workspace_id: &str) -> StorageResult<()> {
        let workspace_dir = self.workspace_dir(workspace_id);
        if fs::symlink_metadata(&workspace_dir).await.is_ok() {
            fs::remove_dir_all(&workspace_dir)
                .await
                .map_err(|e| StorageError::from_io(e, workspace_id))?;
        }
        Ok(())
    }

    // ── NFS Extended Operations ──

    async fn set_file_size(&self, workspace_id: &str, path: &str, size: u64) -> StorageResult<()> {
        let full_path = self.resolve_path(workspace_id, path)?;
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&full_path)
            .await
            .map_err(|e| StorageError::from_io(e, path))?;

        file.set_len(size)
            .await
            .map_err(|e| StorageError::from_io(e, path))
    }

    async fn set_permissions(
        &self,
        workspace_id: &str,
        path: &str,
        mode: u32,
    ) -> StorageResult<()> {
        let full_path = self.resolve_path(workspace_id, path)?;

        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(mode);
        fs::set_permissions(&full_path, perms)
            .await
            .map_err(|e| StorageError::from_io(e, path))
    }

    async fn set_times(
        &self,
        workspace_id: &str,
        path: &str,
        atime: Option<DateTime<Utc>>,
        mtime: Option<DateTime<Utc>>,
    ) -> StorageResult<()> {
        use std::time::{Duration, UNIX_EPOCH};

        let full_path = self.resolve_path(workspace_id, path)?;

        // Convert DateTime<Utc> to filetime values.
        // If a time is None, use the current file's existing time.
        let metadata = fs::symlink_metadata(&full_path)
            .await
            .map_err(|e| StorageError::from_io(e, path))?;

        let new_atime = match atime {
            Some(dt) => {
                let secs = dt.timestamp().max(0) as u64;
                let nanos = dt.timestamp_subsec_nanos();
                UNIX_EPOCH + Duration::new(secs, nanos)
            }
            None => metadata.accessed().unwrap_or(UNIX_EPOCH),
        };

        let new_mtime = match mtime {
            Some(dt) => {
                let secs = dt.timestamp().max(0) as u64;
                let nanos = dt.timestamp_subsec_nanos();
                UNIX_EPOCH + Duration::new(secs, nanos)
            }
            None => metadata.modified().unwrap_or(UNIX_EPOCH),
        };

        // Use libc::utimensat for precise timestamp setting
        let path_cstr = std::ffi::CString::new(full_path.to_str().ok_or_else(|| {
            StorageError::Internal(format!("invalid path: {}", full_path.display()))
        })?)
        .map_err(|e| StorageError::Internal(format!("invalid path bytes: {}", e)))?;

        let atime_dur = new_atime.duration_since(UNIX_EPOCH).unwrap_or_default();
        let mtime_dur = new_mtime.duration_since(UNIX_EPOCH).unwrap_or_default();

        let times = [
            libc::timespec {
                tv_sec: atime_dur.as_secs() as libc::time_t,
                tv_nsec: atime_dur.subsec_nanos() as libc::c_long,
            },
            libc::timespec {
                tv_sec: mtime_dur.as_secs() as libc::time_t,
                tv_nsec: mtime_dur.subsec_nanos() as libc::c_long,
            },
        ];

        let path_cstr_clone = path_cstr.clone();
        let result = tokio::task::spawn_blocking(move || unsafe {
            libc::utimensat(
                libc::AT_FDCWD,
                path_cstr_clone.as_ptr(),
                times.as_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        })
        .await
        .map_err(|e| StorageError::Internal(format!("spawn_blocking failed: {}", e)))?;

        if result != 0 {
            let err = std::io::Error::last_os_error();
            return Err(StorageError::from_io(err, path));
        }

        Ok(())
    }

    async fn symlink(
        &self,
        workspace_id: &str,
        link_path: &str,
        target: &str,
    ) -> StorageResult<()> {
        let full_link_path = self.resolve_path(workspace_id, link_path)?;
        tokio::fs::symlink(target, &full_link_path)
            .await
            .map_err(|e| StorageError::from_io(e, link_path))
    }

    async fn readlink(&self, workspace_id: &str, path: &str) -> StorageResult<String> {
        let full_path = self.resolve_path(workspace_id, path)?;
        let target = fs::read_link(&full_path)
            .await
            .map_err(|e| StorageError::from_io(e, path))?;

        Ok(target.to_string_lossy().to_string())
    }

    async fn stat_fs(&self, workspace_id: &str) -> StorageResult<super::FsStats> {
        let workspace_dir = self.workspace_dir(workspace_id);

        // Use statvfs to get real filesystem statistics
        let path_cstr = std::ffi::CString::new(workspace_dir.to_str().ok_or_else(|| {
            StorageError::Internal(format!("invalid path: {}", workspace_dir.display()))
        })?)
        .map_err(|e| StorageError::Internal(format!("invalid path bytes: {}", e)))?;

        let result = tokio::task::spawn_blocking(move || {
            let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
            let ret = unsafe { libc::statvfs(path_cstr.as_ptr(), &mut stat) };
            if ret == 0 {
                Ok(stat)
            } else {
                Err(std::io::Error::last_os_error())
            }
        })
        .await
        .map_err(|e| StorageError::Internal(format!("spawn_blocking failed: {}", e)))?
        .map_err(|e| StorageError::from_io(e, workspace_id))?;

        Ok(super::FsStats {
            blocks: result.f_blocks,
            bfree: result.f_bfree,
            bavail: result.f_bavail,
            files: result.f_files,
            ffree: result.f_ffree,
            bsize: result.f_bsize as u32,
            namelen: result.f_namemax as u32,
            frsize: result.f_frsize as u32,
        })
    }
}

/// Recursively copy a directory tree
async fn copy_dir_recursive(src: &Path, dst: &Path, rel_path: &str) -> StorageResult<()> {
    fs::create_dir_all(dst)
        .await
        .map_err(|e| StorageError::from_io(e, rel_path))?;

    let mut entries = fs::read_dir(src)
        .await
        .map_err(|e| StorageError::from_io(e, rel_path))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| StorageError::from_io(e, rel_path))?
    {
        let src_child = entry.path();
        let dst_child = dst.join(entry.file_name());

        if src_child.is_dir() {
            Box::pin(copy_dir_recursive(&src_child, &dst_child, rel_path)).await?;
        } else {
            fs::copy(&src_child, &dst_child)
                .await
                .map_err(|e| StorageError::from_io(e, rel_path))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup() -> (TempDir, LocalStorageBackend) {
        let tmp = TempDir::new().unwrap();
        let backend = LocalStorageBackend::new(tmp.path().to_path_buf());
        // Create workspace root
        backend.create_workspace_root("ws1").await.unwrap();
        (tmp, backend)
    }

    #[tokio::test]
    async fn test_write_and_read_file() {
        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "hello.txt", b"hello world")
            .await
            .unwrap();
        let content = backend.read_file("ws1", "hello.txt").await.unwrap();
        assert_eq!(content, b"hello world");
    }

    #[tokio::test]
    async fn test_read_file_range() {
        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "data.txt", b"0123456789")
            .await
            .unwrap();
        let range = backend
            .read_file_range("ws1", "data.txt", 3, 4)
            .await
            .unwrap();
        assert_eq!(range, b"3456");
    }

    #[tokio::test]
    async fn test_write_file_at() {
        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "data.txt", b"hello world")
            .await
            .unwrap();
        backend
            .write_file_at("ws1", "data.txt", 6, b"rust!")
            .await
            .unwrap();
        let content = backend.read_file("ws1", "data.txt").await.unwrap();
        assert_eq!(&content, b"hello rust!");
    }

    #[tokio::test]
    async fn test_create_file_exclusive() {
        let (_tmp, backend) = setup().await;
        backend.create_file("ws1", "new.txt", true).await.unwrap();
        // Second exclusive create should fail
        let err = backend
            .create_file("ws1", "new.txt", true)
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn test_create_file_non_exclusive() {
        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "existing.txt", b"old content")
            .await
            .unwrap();
        // Non-exclusive should truncate
        backend
            .create_file("ws1", "existing.txt", false)
            .await
            .unwrap();
        let content = backend.read_file("ws1", "existing.txt").await.unwrap();
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn test_stat() {
        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "file.txt", b"content")
            .await
            .unwrap();
        let stat = backend.stat("ws1", "file.txt").await.unwrap();
        assert_eq!(stat.name, "file.txt");
        assert_eq!(stat.file_type, FileType::File);
        assert_eq!(stat.size, 7);
    }

    #[tokio::test]
    async fn test_list_dir() {
        let (_tmp, backend) = setup().await;
        backend.write_file("ws1", "a.txt", b"a").await.unwrap();
        backend.write_file("ws1", "b.txt", b"b").await.unwrap();
        backend.mkdir("ws1", "subdir", false).await.unwrap();

        let entries = backend.list_dir("ws1", "").await.unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "a.txt");
        assert_eq!(entries[1].name, "b.txt");
        assert_eq!(entries[2].name, "subdir");
    }

    #[tokio::test]
    async fn test_exists() {
        let (_tmp, backend) = setup().await;
        assert!(!backend.exists("ws1", "nope.txt").await.unwrap());
        backend.write_file("ws1", "yes.txt", b"y").await.unwrap();
        assert!(backend.exists("ws1", "yes.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_mkdir_recursive_and_non_recursive() {
        let (_tmp, backend) = setup().await;
        // Recursive should create parents
        backend.mkdir("ws1", "a/b/c", true).await.unwrap();
        assert!(backend.exists("ws1", "a/b/c").await.unwrap());

        // Non-recursive should fail if parent missing
        let err = backend.mkdir("ws1", "x/y/z", false).await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_remove_file() {
        let (_tmp, backend) = setup().await;
        backend.write_file("ws1", "del.txt", b"bye").await.unwrap();
        backend.remove_file("ws1", "del.txt").await.unwrap();
        assert!(!backend.exists("ws1", "del.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_remove_file_on_directory() {
        let (_tmp, backend) = setup().await;
        backend.mkdir("ws1", "mydir", false).await.unwrap();
        let err = backend.remove_file("ws1", "mydir").await.unwrap_err();
        assert!(matches!(err, StorageError::IsADirectory(_)));
    }

    #[tokio::test]
    async fn test_remove_dir_non_empty() {
        let (_tmp, backend) = setup().await;
        backend.mkdir("ws1", "dir", false).await.unwrap();
        backend
            .write_file("ws1", "dir/file.txt", b"f")
            .await
            .unwrap();
        let err = backend.remove_dir("ws1", "dir", false).await.unwrap_err();
        assert!(matches!(
            err,
            StorageError::DirectoryNotEmpty(_) | StorageError::Io { .. }
        ));
    }

    #[tokio::test]
    async fn test_remove_dir_on_file() {
        let (_tmp, backend) = setup().await;
        backend.write_file("ws1", "file.txt", b"f").await.unwrap();
        let err = backend
            .remove_dir("ws1", "file.txt", false)
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::NotADirectory(_)));
    }

    #[tokio::test]
    async fn test_remove_dir_recursive() {
        let (_tmp, backend) = setup().await;
        backend.mkdir("ws1", "tree/sub", true).await.unwrap();
        backend
            .write_file("ws1", "tree/sub/f.txt", b"f")
            .await
            .unwrap();
        backend.remove_dir("ws1", "tree", true).await.unwrap();
        assert!(!backend.exists("ws1", "tree").await.unwrap());
    }

    #[tokio::test]
    async fn test_rename() {
        let (_tmp, backend) = setup().await;
        backend.write_file("ws1", "old.txt", b"data").await.unwrap();
        backend.rename("ws1", "old.txt", "new.txt").await.unwrap();
        assert!(!backend.exists("ws1", "old.txt").await.unwrap());
        let content = backend.read_file("ws1", "new.txt").await.unwrap();
        assert_eq!(content, b"data");
    }

    #[tokio::test]
    async fn test_copy_file() {
        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "src.txt", b"copy me")
            .await
            .unwrap();
        backend.copy("ws1", "src.txt", "dst.txt").await.unwrap();
        assert_eq!(
            backend.read_file("ws1", "dst.txt").await.unwrap(),
            b"copy me"
        );
        // Source still exists
        assert!(backend.exists("ws1", "src.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_copy_directory() {
        let (_tmp, backend) = setup().await;
        backend.write_file("ws1", "dir/a.txt", b"a").await.unwrap();
        backend.write_file("ws1", "dir/b.txt", b"b").await.unwrap();
        backend.copy("ws1", "dir", "dir_copy").await.unwrap();
        assert_eq!(
            backend.read_file("ws1", "dir_copy/a.txt").await.unwrap(),
            b"a"
        );
        assert_eq!(
            backend.read_file("ws1", "dir_copy/b.txt").await.unwrap(),
            b"b"
        );
    }

    #[tokio::test]
    async fn test_set_file_size() {
        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "trunc.txt", b"hello world")
            .await
            .unwrap();
        backend.set_file_size("ws1", "trunc.txt", 5).await.unwrap();
        let content = backend.read_file("ws1", "trunc.txt").await.unwrap();
        assert_eq!(content, b"hello");
    }

    #[tokio::test]
    async fn test_set_permissions() {
        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "perm.txt", b"data")
            .await
            .unwrap();

        backend
            .set_permissions("ws1", "perm.txt", 0o600)
            .await
            .unwrap();

        let stat = backend.stat("ws1", "perm.txt").await.unwrap();
        // Check only the permission bits (lower 12 bits)
        assert_eq!(stat.mode & 0o7777, 0o600);
    }

    #[tokio::test]
    async fn test_set_times() {
        use chrono::TimeZone;

        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "times.txt", b"data")
            .await
            .unwrap();

        let target_atime = Utc.with_ymd_and_hms(2020, 6, 15, 12, 0, 0).unwrap();
        let target_mtime = Utc.with_ymd_and_hms(2021, 3, 10, 8, 30, 0).unwrap();

        backend
            .set_times("ws1", "times.txt", Some(target_atime), Some(target_mtime))
            .await
            .unwrap();

        let stat = backend.stat("ws1", "times.txt").await.unwrap();
        // Verify mtime was set (second-level precision)
        assert_eq!(
            stat.modified_at.unwrap().timestamp(),
            target_mtime.timestamp()
        );
        // Verify atime was set
        assert_eq!(
            stat.accessed_at.unwrap().timestamp(),
            target_atime.timestamp()
        );
    }

    #[tokio::test]
    async fn test_symlink_and_readlink() {
        let (_tmp, backend) = setup().await;
        backend
            .write_file("ws1", "target.txt", b"target content")
            .await
            .unwrap();
        backend
            .symlink("ws1", "link.txt", "target.txt")
            .await
            .unwrap();
        let target = backend.readlink("ws1", "link.txt").await.unwrap();
        assert_eq!(target, "target.txt");
    }

    #[tokio::test]
    async fn test_workspace_lifecycle() {
        let tmp = TempDir::new().unwrap();
        let backend = LocalStorageBackend::new(tmp.path().to_path_buf());

        backend.create_workspace_root("ws_test").await.unwrap();
        assert!(tmp.path().join("ws_test").exists());

        backend.delete_workspace_root("ws_test").await.unwrap();
        assert!(!tmp.path().join("ws_test").exists());
    }

    #[tokio::test]
    async fn test_path_traversal_parent_dir() {
        let (_tmp, backend) = setup().await;
        let err = backend.read_file("ws1", "../etc/passwd").await.unwrap_err();
        assert!(matches!(err, StorageError::PathTraversalDenied(_)));
    }

    #[tokio::test]
    async fn test_path_traversal_absolute() {
        let (_tmp, backend) = setup().await;
        let err = backend.read_file("ws1", "/etc/passwd").await.unwrap_err();
        assert!(matches!(err, StorageError::PathTraversalDenied(_)));
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let (_tmp, backend) = setup().await;
        let err = backend.read_file("ws1", "nope.txt").await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_storage_error_from_io() {
        let not_found = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = StorageError::from_io(not_found, "/foo/bar");
        assert!(matches!(err, StorageError::NotFound(_)));

        let already = std::io::Error::new(std::io::ErrorKind::AlreadyExists, "exists");
        let err = StorageError::from_io(already, "/foo/bar");
        assert!(matches!(err, StorageError::AlreadyExists(_)));

        let perm = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let err = StorageError::from_io(perm, "/foo/bar");
        assert!(matches!(err, StorageError::PermissionDenied(_)));

        // Test EISDIR via raw OS error
        let eisdir = std::io::Error::from_raw_os_error(libc::EISDIR);
        let err = StorageError::from_io(eisdir, "/foo/bar");
        assert!(matches!(err, StorageError::IsADirectory(_)));

        let enotdir = std::io::Error::from_raw_os_error(libc::ENOTDIR);
        let err = StorageError::from_io(enotdir, "/foo/bar");
        assert!(matches!(err, StorageError::NotADirectory(_)));

        let enotempty = std::io::Error::from_raw_os_error(libc::ENOTEMPTY);
        let err = StorageError::from_io(enotempty, "/foo/bar");
        assert!(matches!(err, StorageError::DirectoryNotEmpty(_)));
    }
}
