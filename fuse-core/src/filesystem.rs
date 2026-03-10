//! Generic FUSE filesystem implementation
//!
//! Implements the `fuser::Filesystem` trait parameterized by a `FuseBackend`.
//! This allows the same FUSE logic to be used with different backends:
//! - `RpcFuseBackend` for the standalone fuse-client (gRPC calls)
//! - `ServerFuseBackend` for server-side FUSE mounts (direct storage calls)

use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEmpty,
    ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request, TimeOrNow,
};
use libc::{EINVAL, ENOENT, ENOSYS};
use tokio::runtime::Handle;
use tracing::{debug, warn};
use workspace_proto::{FsFileAttr, FsFileType, FsRenameFlags};

use crate::backend::FuseBackend;
use crate::cache::{
    fs_file_type_to_fuse, i32_to_fs_file_type, DirCache, MetadataCache, ReadCache, StatfsCache,
};
use crate::inode::{join_path, InodeTable, ROOT_INODE};

/// TTL for FUSE attribute caching
const ATTR_TTL: Duration = Duration::from_secs(1);

/// Generation number for inodes
const GENERATION: u64 = 1;

/// File handle information
struct FileHandle {
    /// Corresponding inode
    #[allow(dead_code)]
    ino: u64,
    /// File path
    path: String,
    /// Whether any write operations occurred
    has_written: bool,
    /// Open flags
    #[allow(dead_code)]
    flags: u32,
}

/// Generic workspace FUSE filesystem parameterized by backend.
///
/// Holds all caching state and inode mappings. Uses a `tokio::runtime::Handle`
/// (not an owned Runtime) so both fuse-client and server can provide their own.
pub struct WorkspaceFuse<B: FuseBackend> {
    /// Workspace ID (for debugging/logging)
    #[allow(dead_code)]
    workspace_id: String,
    /// Tokio runtime handle for async operations
    handle: Handle,
    /// Backend for actual file operations
    backend: Arc<B>,
    /// Inode table
    inodes: InodeTable,
    /// Metadata cache
    meta_cache: MetadataCache,
    /// Directory listing cache
    dir_cache: DirCache,
    /// Read data cache (wrapped in Arc for async prefetch)
    read_cache: Arc<ReadCache>,
    /// Statfs cache (30-second TTL)
    statfs_cache: StatfsCache,
    /// Next file handle
    next_fh: AtomicU64,
    /// File handle table: fh → FileHandle
    fh_table: RwLock<HashMap<u64, FileHandle>>,
    /// Readahead state: path → last read block index
    readahead_state: RwLock<HashMap<String, u64>>,
    /// Current uid
    uid: u32,
    /// Current gid
    gid: u32,
}

impl<B: FuseBackend> WorkspaceFuse<B> {
    /// Create a new WorkspaceFuse instance.
    ///
    /// Takes a `tokio::runtime::Handle` rather than an owned `Runtime`, so callers
    /// can share a runtime across multiple mounts (server) or pass a dedicated one (client).
    pub fn new(
        workspace_id: String,
        handle: Handle,
        backend: B,
        cache_ttl: Duration,
        block_size: u32,
        read_cache_size_bytes: u64,
    ) -> Self {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        Self {
            workspace_id,
            handle,
            backend: Arc::new(backend),
            inodes: InodeTable::new(),
            meta_cache: MetadataCache::new(cache_ttl),
            dir_cache: DirCache::new(cache_ttl),
            read_cache: Arc::new(ReadCache::with_max_size(block_size, read_cache_size_bytes)),
            statfs_cache: StatfsCache::new(),
            next_fh: AtomicU64::new(1),
            fh_table: RwLock::new(HashMap::new()),
            readahead_state: RwLock::new(HashMap::new()),
            uid,
            gid,
        }
    }

    /// Allocate a new file handle
    fn alloc_fh(&self) -> u64 {
        self.next_fh.fetch_add(1, Ordering::SeqCst)
    }

    /// Convert FsFileAttr to fuser FileAttr
    fn proto_attr_to_fuse(&self, inode: u64, attr: &FsFileAttr) -> FileAttr {
        let kind = match i32_to_fs_file_type(attr.file_type) {
            FsFileType::File => FileType::RegularFile,
            FsFileType::Directory => FileType::Directory,
            FsFileType::Symlink => FileType::Symlink,
            FsFileType::Unspecified => FileType::RegularFile,
        };

        let atime = attr
            .atime
            .as_ref()
            .map(|t| UNIX_EPOCH + Duration::new(t.seconds as u64, t.nanos as u32))
            .unwrap_or(UNIX_EPOCH);

        let mtime = attr
            .mtime
            .as_ref()
            .map(|t| UNIX_EPOCH + Duration::new(t.seconds as u64, t.nanos as u32))
            .unwrap_or(UNIX_EPOCH);

        let ctime = attr
            .ctime
            .as_ref()
            .map(|t| UNIX_EPOCH + Duration::new(t.seconds as u64, t.nanos as u32))
            .unwrap_or(UNIX_EPOCH);

        FileAttr {
            ino: inode,
            size: attr.size,
            blocks: if attr.blocks > 0 {
                attr.blocks
            } else {
                attr.size.div_ceil(512)
            },
            atime,
            mtime,
            ctime,
            crtime: ctime,
            kind,
            perm: (attr.mode & 0o7777) as u16,
            nlink: attr.nlink,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: attr.blksize,
            flags: 0,
        }
    }

    /// Get attributes with caching
    fn get_attr_cached(&self, path: &str) -> Result<FsFileAttr, i32> {
        // Check cache first
        if let Some(cached) = self.meta_cache.get(path) {
            return Ok(cached.attr);
        }

        // Fetch from backend
        let backend = self.backend.clone();
        let path_owned = path.to_string();
        let attr = self
            .handle
            .block_on(async move { backend.getattr(&path_owned).await })
            .map_err(|e| e.to_errno())?;

        // Cache the result
        self.meta_cache.insert(path, attr);

        Ok(attr)
    }

    /// Invalidate cache for a specific path.
    ///
    /// Called by `FuseMountManager` when receiving `FileChanged` events from the Client.
    pub fn invalidate_path(&self, path: &str) {
        self.meta_cache.invalidate(path);
        self.dir_cache.invalidate(path);
        self.read_cache.invalidate_file(path);

        // Also invalidate parent directory listing (new/deleted entries)
        if let Some(parent) = path.rsplit_once('/').map(|(p, _)| p) {
            self.dir_cache.invalidate(parent);
        } else if !path.is_empty() {
            // Parent is root
            self.dir_cache.invalidate("");
        }
    }

    /// Purge all caches (used on Client reconnection)
    pub fn purge_all_caches(&self) {
        self.meta_cache.invalidate_all();
        self.dir_cache.invalidate_all();
        self.read_cache.invalidate_all();
        self.statfs_cache.invalidate();
    }
}

/// Newtype wrapper that holds `Arc<WorkspaceFuse<B>>` and implements `fuser::Filesystem`.
///
/// This is necessary because `fuser::mount2` takes ownership of the filesystem,
/// but we need to retain a reference for cache invalidation from `FuseMountManager`.
pub struct FuseFilesystemWrapper<B: FuseBackend> {
    inner: Arc<WorkspaceFuse<B>>,
}

impl<B: FuseBackend> FuseFilesystemWrapper<B> {
    /// Create a new wrapper from an `Arc<WorkspaceFuse<B>>`.
    pub fn new(inner: Arc<WorkspaceFuse<B>>) -> Self {
        Self { inner }
    }
}

impl<B: FuseBackend> Filesystem for FuseFilesystemWrapper<B> {
    fn init(
        &mut self,
        _req: &Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        debug!("FUSE init");
        Ok(())
    }

    fn destroy(&mut self) {
        debug!("FUSE destroy");
    }

    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        debug!(parent = parent, name = %name, "lookup");

        let path = match self.inner.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        match self.inner.get_attr_cached(&path) {
            Ok(attr) => {
                let inode = self.inner.inodes.get_or_create(&path);
                let fuse_attr = self.inner.proto_attr_to_fuse(inode, &attr);
                reply.entry(&ATTR_TTL, &fuse_attr, GENERATION);
            }
            Err(errno) => {
                debug!(path = %path, errno = errno, "lookup failed");
                reply.error(errno);
            }
        }
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        debug!(ino = ino, "getattr");

        let path = match self.inner.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        match self.inner.get_attr_cached(&path) {
            Ok(attr) => {
                let fuse_attr = self.inner.proto_attr_to_fuse(ino, &attr);
                reply.attr(&ATTR_TTL, &fuse_attr);
            }
            Err(errno) => {
                reply.error(errno);
            }
        }
    }

    fn setattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        debug!(ino = ino, size = ?size, "setattr");

        let path = match self.inner.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let atime_ts = atime.map(|t| time_or_now_to_timestamp(t));
        let mtime_ts = mtime.map(|t| time_or_now_to_timestamp(t));

        let backend = self.inner.backend.clone();
        let path_owned = path.clone();
        let result = self.inner.handle.block_on(async move {
            backend
                .setattr(&path_owned, size, mode, atime_ts, mtime_ts)
                .await
        });

        match result {
            Ok(attr) => {
                // Invalidate cache
                self.inner.meta_cache.invalidate(&path);
                if size.is_some() {
                    self.inner.read_cache.invalidate_file(&path);
                }

                let fuse_attr = self.inner.proto_attr_to_fuse(ino, &attr);
                reply.attr(&ATTR_TTL, &fuse_attr);
            }
            Err(e) => {
                reply.error(e.to_errno());
            }
        }
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        debug!(ino = ino, offset = offset, size = size, "read");

        let path = match self.inner.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let offset = offset as u64;
        let size = size as usize;
        let block_size = self.inner.read_cache.block_size() as u64;

        // Calculate block range
        let start_block = self.inner.read_cache.offset_to_block_idx(offset);
        let end_block = if size == 0 {
            start_block
        } else {
            self.inner
                .read_cache
                .offset_to_block_idx(offset + size as u64 - 1)
        };

        // Check for sequential read pattern and determine if we should prefetch
        let should_prefetch = {
            let readahead = self.inner.readahead_state.read().unwrap();
            if let Some(&last_block) = readahead.get(&path) {
                start_block == last_block + 1
            } else {
                false
            }
        };

        // Collect data from all blocks in range
        let mut result_data = Vec::with_capacity(size);
        let mut current_offset = offset;
        let end_offset = offset + size as u64;
        let mut last_block_was_eof = false;

        for block_idx in start_block..=end_block {
            let block_start = self.inner.read_cache.block_idx_to_offset(block_idx);

            // Get block data (from cache or backend)
            let block_data = if let Some(cached) = self.inner.read_cache.get(&path, block_idx) {
                cached
            } else {
                // Fetch from backend
                let backend = self.inner.backend.clone();
                let path_owned = path.clone();
                let fetch_size = self.inner.read_cache.block_size();
                match self.inner.handle.block_on(async move {
                    backend.read(&path_owned, block_start, fetch_size).await
                }) {
                    Ok(data) => {
                        self.inner.read_cache.insert(&path, block_idx, data.clone());
                        Arc::new(data)
                    }
                    Err(e) => {
                        reply.error(e.to_errno());
                        return;
                    }
                }
            };

            // Calculate the portion of this block we need
            let block_offset_in_data = if current_offset > block_start {
                (current_offset - block_start) as usize
            } else {
                0
            };

            let block_end = block_start + block_size;
            let copy_end = if end_offset < block_end {
                (end_offset - block_start) as usize
            } else {
                block_data.len()
            };

            // Copy the relevant portion
            if block_offset_in_data < block_data.len() {
                let actual_end = copy_end.min(block_data.len());
                result_data.extend_from_slice(&block_data[block_offset_in_data..actual_end]);
            }

            current_offset = block_end;

            // If we've reached EOF (block smaller than block_size), stop
            if block_data.len() < block_size as usize {
                last_block_was_eof = true;
                break;
            }
        }

        // Update readahead state
        {
            let mut readahead = self.inner.readahead_state.write().unwrap();
            readahead.insert(path.clone(), end_block);
        }

        // Prefetch next block asynchronously if sequential read pattern detected
        if should_prefetch && !last_block_was_eof {
            let prefetch_block = end_block + 1;
            if self.inner.read_cache.get(&path, prefetch_block).is_none() {
                let backend = self.inner.backend.clone();
                let path_owned = path.clone();
                let block_start = self.inner.read_cache.block_idx_to_offset(prefetch_block);
                let fetch_size = self.inner.read_cache.block_size();
                let read_cache = Arc::clone(&self.inner.read_cache);

                self.inner.handle.spawn(async move {
                    if let Ok(data) = backend.read(&path_owned, block_start, fetch_size).await {
                        read_cache.insert(&path_owned, prefetch_block, data);
                        debug!(path = %path_owned, block = prefetch_block, "prefetched block async");
                    }
                });
            }
        }

        reply.data(&result_data);
    }

    fn write(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        debug!(ino = ino, offset = offset, size = data.len(), "write");

        let path = match self.inner.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let backend = self.inner.backend.clone();
        let path_owned = path.clone();
        let data_owned = data.to_vec();
        let offset = offset as u64;

        let result = self
            .inner
            .handle
            .block_on(async move { backend.write(&path_owned, offset, &data_owned).await });

        match result {
            Ok(bytes_written) => {
                // Mark file handle as having written data
                {
                    let mut fh_table = self.inner.fh_table.write().unwrap();
                    if let Some(handle) = fh_table.get_mut(&fh) {
                        handle.has_written = true;
                    }
                }

                // Invalidate caches
                self.inner.meta_cache.invalidate(&path);
                self.inner.read_cache.invalidate_file(&path);

                reply.written(bytes_written as u32);
            }
            Err(e) => {
                reply.error(e.to_errno());
            }
        }
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        debug!(ino = ino, offset = offset, "readdir");

        let path = match self.inner.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // Check cache
        let entries = if let Some(cached) = self.inner.dir_cache.get(&path) {
            cached
        } else {
            // Fetch from backend
            let backend = self.inner.backend.clone();
            let path_owned = path.clone();
            match self
                .inner
                .handle
                .block_on(async move { backend.readdir(&path_owned).await })
            {
                Ok(entries) => {
                    self.inner.dir_cache.insert(&path, entries.clone());
                    Arc::new(entries)
                }
                Err(e) => {
                    reply.error(e.to_errno());
                    return;
                }
            }
        };

        // Add . and ..
        if offset == 0 && reply.add(ino, 1, FileType::Directory, ".") {
            reply.ok();
            return;
        }
        if offset <= 1 {
            let parent_ino = if ino == ROOT_INODE {
                ROOT_INODE
            } else {
                if let Some(parent_path) = path.rsplit_once('/').map(|(p, _)| p.to_string()) {
                    self.inner.inodes.get_or_create(&parent_path)
                } else {
                    ROOT_INODE
                }
            };
            if reply.add(parent_ino, 2, FileType::Directory, "..") {
                reply.ok();
                return;
            }
        }

        // Add entries
        for (i, entry) in entries.iter().enumerate() {
            let entry_offset = i as i64 + 2; // +2 for . and ..
            if entry_offset <= offset {
                continue;
            }

            let child_path = join_path(&path, &entry.name);
            let child_ino = self.inner.inodes.get_or_create(&child_path);

            let file_type = entry
                .attr
                .as_ref()
                .map(|a| fs_file_type_to_fuse(i32_to_fs_file_type(a.file_type)))
                .unwrap_or(FileType::RegularFile);

            // Cache the metadata if available
            if let Some(ref attr) = entry.attr {
                self.inner.meta_cache.insert(&child_path, *attr);
            }

            if reply.add(child_ino, entry_offset + 1, file_type, &entry.name) {
                break;
            }
        }

        reply.ok();
    }

    fn mkdir(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        debug!(parent = parent, name = %name, mode = mode, "mkdir");

        let path = match self.inner.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let backend = self.inner.backend.clone();
        let path_owned = path.clone();
        let result = self
            .inner
            .handle
            .block_on(async move { backend.mkdir(&path_owned, mode).await });

        match result {
            Ok(attr) => {
                if let Some(parent_path) = self.inner.inodes.get_path(parent) {
                    self.inner.dir_cache.invalidate(&parent_path);
                }

                let inode = self.inner.inodes.get_or_create(&path);
                let fuse_attr = self.inner.proto_attr_to_fuse(inode, &attr);
                reply.entry(&ATTR_TTL, &fuse_attr, GENERATION);
            }
            Err(e) => {
                reply.error(e.to_errno());
            }
        }
    }

    fn create(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        debug!(parent = parent, name = %name, mode = mode, flags = flags, "create");

        let path = match self.inner.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let exclusive = (flags & libc::O_EXCL) != 0;

        let backend = self.inner.backend.clone();
        let path_owned = path.clone();
        let result = self
            .inner
            .handle
            .block_on(async move { backend.create(&path_owned, mode, exclusive).await });

        match result {
            Ok(attr) => {
                if let Some(parent_path) = self.inner.inodes.get_path(parent) {
                    self.inner.dir_cache.invalidate(&parent_path);
                }

                let inode = self.inner.inodes.get_or_create(&path);
                let fh = self.inner.alloc_fh();

                {
                    let mut fh_table = self.inner.fh_table.write().unwrap();
                    fh_table.insert(
                        fh,
                        FileHandle {
                            ino: inode,
                            path: path.clone(),
                            has_written: false,
                            flags: flags as u32,
                        },
                    );
                }

                let fuse_attr = self.inner.proto_attr_to_fuse(inode, &attr);
                reply.created(&ATTR_TTL, &fuse_attr, GENERATION, fh, 0);
            }
            Err(e) => {
                reply.error(e.to_errno());
            }
        }
    }

    fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        debug!(parent = parent, name = %name, "unlink");

        let path = match self.inner.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let backend = self.inner.backend.clone();
        let path_owned = path.clone();
        let result = self
            .inner
            .handle
            .block_on(async move { backend.unlink(&path_owned).await });

        match result {
            Ok(()) => {
                self.inner.inodes.remove_by_path(&path);
                self.inner.meta_cache.invalidate(&path);
                self.inner.read_cache.invalidate_file(&path);

                if let Some(parent_path) = self.inner.inodes.get_path(parent) {
                    self.inner.dir_cache.invalidate(&parent_path);
                }

                reply.ok();
            }
            Err(e) => {
                reply.error(e.to_errno());
            }
        }
    }

    fn rmdir(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        debug!(parent = parent, name = %name, "rmdir");

        let path = match self.inner.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let backend = self.inner.backend.clone();
        let path_owned = path.clone();
        let result = self
            .inner
            .handle
            .block_on(async move { backend.rmdir(&path_owned).await });

        match result {
            Ok(()) => {
                self.inner.inodes.remove_by_path(&path);
                self.inner.meta_cache.invalidate_tree(&path);
                self.inner.dir_cache.invalidate_tree(&path);
                self.inner.read_cache.invalidate_file(&path);

                if let Some(parent_path) = self.inner.inodes.get_path(parent) {
                    self.inner.dir_cache.invalidate(&parent_path);
                }

                reply.ok();
            }
            Err(e) => {
                reply.error(e.to_errno());
            }
        }
    }

    fn rename(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        flags: u32,
        reply: ReplyEmpty,
    ) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };
        let newname = match newname.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        debug!(
            parent = parent,
            name = %name,
            newparent = newparent,
            newname = %newname,
            "rename"
        );

        let old_path = match self.inner.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let new_path = match self.inner.inodes.build_child_path(newparent, newname) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let proto_flags = if flags & 1 != 0 {
            FsRenameFlags::Noreplace
        } else if flags & 2 != 0 {
            FsRenameFlags::Exchange
        } else {
            FsRenameFlags::None
        };

        let backend = self.inner.backend.clone();
        let old_path_owned = old_path.clone();
        let new_path_owned = new_path.clone();
        let result = self.inner.handle.block_on(async move {
            backend
                .rename(&old_path_owned, &new_path_owned, proto_flags)
                .await
        });

        match result {
            Ok(()) => {
                self.inner.inodes.rename_tree(&old_path, &new_path);

                self.inner.meta_cache.invalidate_tree(&old_path);
                self.inner.meta_cache.invalidate_tree(&new_path);
                self.inner.dir_cache.invalidate_tree(&old_path);
                self.inner.dir_cache.invalidate_tree(&new_path);
                self.inner.read_cache.invalidate_file(&old_path);

                if let Some(parent_path) = self.inner.inodes.get_path(parent) {
                    self.inner.dir_cache.invalidate(&parent_path);
                }
                if parent != newparent {
                    if let Some(newparent_path) = self.inner.inodes.get_path(newparent) {
                        self.inner.dir_cache.invalidate(&newparent_path);
                    }
                }

                reply.ok();
            }
            Err(e) => {
                reply.error(e.to_errno());
            }
        }
    }

    fn symlink(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        link_name: &OsStr,
        target: &std::path::Path,
        reply: ReplyEntry,
    ) {
        let link_name = match link_name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };
        let target = match target.to_str() {
            Some(t) => t,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        debug!(parent = parent, link_name = %link_name, target = %target, "symlink");

        let link_path = match self.inner.inodes.build_child_path(parent, link_name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let backend = self.inner.backend.clone();
        let link_path_owned = link_path.clone();
        let target_owned = target.to_string();
        let result = self
            .inner
            .handle
            .block_on(async move { backend.symlink(&link_path_owned, &target_owned).await });

        match result {
            Ok(attr) => {
                if let Some(parent_path) = self.inner.inodes.get_path(parent) {
                    self.inner.dir_cache.invalidate(&parent_path);
                }

                let inode = self.inner.inodes.get_or_create(&link_path);
                let fuse_attr = self.inner.proto_attr_to_fuse(inode, &attr);
                reply.entry(&ATTR_TTL, &fuse_attr, GENERATION);
            }
            Err(e) => {
                reply.error(e.to_errno());
            }
        }
    }

    fn readlink(&mut self, _req: &Request<'_>, ino: u64, reply: fuser::ReplyData) {
        debug!(ino = ino, "readlink");

        let path = match self.inner.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let backend = self.inner.backend.clone();
        let path_owned = path.clone();
        let result = self
            .inner
            .handle
            .block_on(async move { backend.readlink(&path_owned).await });

        match result {
            Ok(target) => {
                reply.data(target.as_bytes());
            }
            Err(e) => {
                reply.error(e.to_errno());
            }
        }
    }

    fn open(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        debug!(ino = ino, flags = flags, "open");

        let path = match self.inner.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let fh = self.inner.alloc_fh();
        {
            let mut fh_table = self.inner.fh_table.write().unwrap();
            fh_table.insert(
                fh,
                FileHandle {
                    ino,
                    path,
                    has_written: false,
                    flags: flags as u32,
                },
            );
        }
        reply.opened(fh, 0);
    }

    fn release(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        debug!(ino = ino, fh = fh, "release");

        let file_handle = {
            let mut fh_table = self.inner.fh_table.write().unwrap();
            fh_table.remove(&fh)
        };

        if let Some(handle) = file_handle {
            if handle.has_written {
                self.inner.meta_cache.invalidate(&handle.path);
            }
            {
                let mut readahead = self.inner.readahead_state.write().unwrap();
                readahead.remove(&handle.path);
            }
        }

        reply.ok();
    }

    fn opendir(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        debug!(ino = ino, flags = flags, "opendir");
        let fh = self.inner.alloc_fh();
        reply.opened(fh, 0);
    }

    fn releasedir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        _flags: i32,
        reply: ReplyEmpty,
    ) {
        debug!(ino = ino, fh = fh, "releasedir");
        reply.ok();
    }

    fn statfs(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyStatfs) {
        debug!(ino = ino, "statfs");

        // Check cache first
        if let Some(stat) = self.inner.statfs_cache.get() {
            reply.statfs(
                stat.blocks,
                stat.bfree,
                stat.bavail,
                stat.files,
                stat.ffree,
                stat.bsize,
                stat.namelen,
                stat.frsize,
            );
            return;
        }

        // Fetch from backend
        let backend = self.inner.backend.clone();
        let result = self
            .inner
            .handle
            .block_on(async move { backend.statfs().await });

        match result {
            Ok(stat) => {
                self.inner.statfs_cache.insert(stat);
                reply.statfs(
                    stat.blocks,
                    stat.bfree,
                    stat.bavail,
                    stat.files,
                    stat.ffree,
                    stat.bsize,
                    stat.namelen,
                    stat.frsize,
                );
            }
            Err(e) => {
                warn!("statfs failed: {}", e);
                // Return default values on error
                reply.statfs(
                    1024 * 1024 * 100, // blocks
                    1024 * 1024 * 50,  // bfree
                    1024 * 1024 * 50,  // bavail
                    1_000_000,         // files
                    900_000,           // ffree
                    4096,              // bsize
                    255,               // namelen
                    4096,              // frsize
                );
            }
        }
    }

    // Unsupported operations
    fn link(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _newparent: u64,
        _newname: &OsStr,
        reply: ReplyEntry,
    ) {
        reply.error(ENOSYS);
    }

    fn mknod(
        &mut self,
        _req: &Request<'_>,
        _parent: u64,
        _name: &OsStr,
        _mode: u32,
        _umask: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
        reply.error(ENOSYS);
    }

    fn getxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &OsStr,
        _size: u32,
        reply: fuser::ReplyXattr,
    ) {
        reply.error(ENOSYS);
    }

    fn setxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &OsStr,
        _value: &[u8],
        _flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }

    fn listxattr(&mut self, _req: &Request<'_>, _ino: u64, _size: u32, reply: fuser::ReplyXattr) {
        reply.error(ENOSYS);
    }

    fn removexattr(&mut self, _req: &Request<'_>, _ino: u64, _name: &OsStr, reply: ReplyEmpty) {
        reply.error(ENOSYS);
    }
}

/// Convert `TimeOrNow` to a protobuf `Timestamp`.
fn time_or_now_to_timestamp(t: TimeOrNow) -> prost_types::Timestamp {
    let d = match t {
        TimeOrNow::SpecificTime(st) => st.duration_since(UNIX_EPOCH).unwrap_or_default(),
        TimeOrNow::Now => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default(),
    };
    prost_types::Timestamp {
        seconds: d.as_secs() as i64,
        nanos: d.subsec_nanos() as i32,
    }
}
