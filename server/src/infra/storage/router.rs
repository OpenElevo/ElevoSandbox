//! Per-workspace storage router
//!
//! Routes file operations to the appropriate backend per workspace.
//! Managed workspaces use the default backend; remote workspaces use
//! per-workspace override backends (RemoteStorageBackend or LocalStorageBackend
//! on an NFS mount point).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tokio::sync::RwLock;

use super::{FileStat, FsStats, StorageBackend, StorageError, StorageResult};

/// Per-workspace storage router.
///
/// Implements `StorageBackend` by delegating to either a per-workspace override
/// backend or the global default backend. For remote workspaces, also manages
/// a read-write lock for channel switching (draining in-flight operations).
pub struct StorageRouter {
    /// Global default backend (used by managed workspaces)
    default_backend: Arc<dyn StorageBackend>,
    /// Per-workspace override backends
    overrides: DashMap<String, Arc<dyn StorageBackend>>,
    /// Per-workspace read-write locks for channel switching
    locks: DashMap<String, Arc<RwLock<()>>>,
}

impl StorageRouter {
    pub fn new(default_backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            default_backend,
            overrides: DashMap::new(),
            locks: DashMap::new(),
        }
    }

    /// Register a per-workspace backend override
    pub fn register(&self, workspace_id: &str, backend: Arc<dyn StorageBackend>) {
        self.overrides.insert(workspace_id.to_string(), backend);
        self.locks
            .entry(workspace_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(())));
    }

    /// Unregister a per-workspace backend override (reverts to default)
    pub fn unregister(&self, workspace_id: &str) {
        self.overrides.remove(workspace_id);
        self.locks.remove(workspace_id);
    }

    /// Replace the backend for a workspace (call while holding write lock)
    pub fn replace_backend(&self, workspace_id: &str, backend: Arc<dyn StorageBackend>) {
        self.overrides.insert(workspace_id.to_string(), backend);
    }

    /// Acquire the write lock for a workspace (used during channel switching).
    /// This blocks until all in-flight read operations complete, with a timeout.
    pub async fn write_lock(
        &self,
        workspace_id: &str,
        timeout: std::time::Duration,
    ) -> StorageResult<tokio::sync::OwnedRwLockWriteGuard<()>> {
        let lock = self
            .locks
            .get(workspace_id)
            .ok_or_else(|| {
                StorageError::Internal(format!(
                    "no lock registered for workspace '{}'",
                    workspace_id
                ))
            })?
            .value()
            .clone();

        tokio::time::timeout(timeout, lock.write_owned())
            .await
            .map_err(|_| {
                StorageError::Internal(format!(
                    "channel switch timeout for workspace '{}'",
                    workspace_id
                ))
            })
    }

    /// Check if a workspace has an override backend registered.
    /// Returns true if the workspace has a per-workspace backend override,
    /// which indicates it's a remote workspace with an active or pending connection.
    pub fn has_override(&self, workspace_id: &str) -> bool {
        self.overrides.contains_key(workspace_id)
    }

    /// Resolve the backend for a workspace
    fn resolve(&self, workspace_id: &str) -> Arc<dyn StorageBackend> {
        self.overrides
            .get(workspace_id)
            .map(|r| r.value().clone())
            .unwrap_or_else(|| self.default_backend.clone())
    }

    /// Get the read lock Arc for a remote workspace (if one exists).
    fn get_lock(&self, workspace_id: &str) -> Option<Arc<RwLock<()>>> {
        self.locks.get(workspace_id).map(|r| r.value().clone())
    }
}

#[async_trait]
impl StorageBackend for StorageRouter {
    async fn read_file(&self, workspace_id: &str, path: &str) -> StorageResult<Vec<u8>> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.read_file(workspace_id, path).await
    }

    async fn read_file_range(
        &self,
        workspace_id: &str,
        path: &str,
        offset: u64,
        length: u32,
    ) -> StorageResult<Vec<u8>> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend
            .read_file_range(workspace_id, path, offset, length)
            .await
    }

    async fn write_file(
        &self,
        workspace_id: &str,
        path: &str,
        content: &[u8],
    ) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.write_file(workspace_id, path, content).await
    }

    async fn write_file_at(
        &self,
        workspace_id: &str,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend
            .write_file_at(workspace_id, path, offset, data)
            .await
    }

    async fn create_file(
        &self,
        workspace_id: &str,
        path: &str,
        exclusive: bool,
    ) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.create_file(workspace_id, path, exclusive).await
    }

    async fn stat(&self, workspace_id: &str, path: &str) -> StorageResult<FileStat> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.stat(workspace_id, path).await
    }

    async fn list_dir(&self, workspace_id: &str, path: &str) -> StorageResult<Vec<FileStat>> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.list_dir(workspace_id, path).await
    }

    async fn exists(&self, workspace_id: &str, path: &str) -> StorageResult<bool> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.exists(workspace_id, path).await
    }

    async fn mkdir(&self, workspace_id: &str, path: &str, recursive: bool) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.mkdir(workspace_id, path, recursive).await
    }

    async fn remove_file(&self, workspace_id: &str, path: &str) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.remove_file(workspace_id, path).await
    }

    async fn remove_dir(
        &self,
        workspace_id: &str,
        path: &str,
        recursive: bool,
    ) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.remove_dir(workspace_id, path, recursive).await
    }

    async fn rename(&self, workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.rename(workspace_id, src, dst).await
    }

    async fn rename_noreplace(
        &self,
        workspace_id: &str,
        src: &str,
        dst: &str,
    ) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.rename_noreplace(workspace_id, src, dst).await
    }

    async fn rename_exchange(&self, workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.rename_exchange(workspace_id, src, dst).await
    }

    async fn copy(&self, workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.copy(workspace_id, src, dst).await
    }

    async fn create_workspace_root(&self, workspace_id: &str) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        backend.create_workspace_root(workspace_id).await
    }

    async fn delete_workspace_root(&self, workspace_id: &str) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        backend.delete_workspace_root(workspace_id).await
    }

    async fn set_file_size(&self, workspace_id: &str, path: &str, size: u64) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.set_file_size(workspace_id, path, size).await
    }

    async fn set_permissions(
        &self,
        workspace_id: &str,
        path: &str,
        mode: u32,
    ) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.set_permissions(workspace_id, path, mode).await
    }

    async fn set_times(
        &self,
        workspace_id: &str,
        path: &str,
        atime: Option<DateTime<Utc>>,
        mtime: Option<DateTime<Utc>>,
    ) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.set_times(workspace_id, path, atime, mtime).await
    }

    async fn symlink(
        &self,
        workspace_id: &str,
        link_path: &str,
        target: &str,
    ) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.symlink(workspace_id, link_path, target).await
    }

    async fn readlink(&self, workspace_id: &str, path: &str) -> StorageResult<String> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.readlink(workspace_id, path).await
    }

    async fn stat_fs(&self, workspace_id: &str) -> StorageResult<FsStats> {
        let backend = self.resolve(workspace_id);
        let lock = self.get_lock(workspace_id);
        let _guard = match &lock {
            Some(l) => Some(l.read().await),
            None => None,
        };
        backend.stat_fs(workspace_id).await
    }
}
