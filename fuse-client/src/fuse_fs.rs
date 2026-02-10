//! FUSE filesystem implementation
//!
//! Implements the fuser::Filesystem trait for remote workspace access.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEmpty,
    ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request, TimeOrNow,
};
use libc::{EACCES, EEXIST, EINVAL, EIO, EISDIR, ENOENT, ENOSPC, ENOSYS, ENOTDIR, ENOTEMPTY};
use tokio::runtime::Runtime;
use tonic::Status;
use tonic_types::StatusExt;
use tracing::{debug, warn};
use workspace_proto::{FsFileAttr, FsFileType};

use crate::cache::{
    fs_file_type_to_fuse, i32_to_fs_file_type, DirCache, MetadataCache, ReadCache, StatfsCache,
};
use crate::inode::{join_path, InodeTable, ROOT_INODE};
use crate::rpc::FileSystemRpcClient;

/// TTL for FUSE attribute caching
const ATTR_TTL: Duration = Duration::from_secs(1);

/// Generation number for inodes
const GENERATION: u64 = 1;

/// File handle information
///
/// Tracks open file state for cache invalidation and write detection.
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

/// Workspace FUSE filesystem
pub struct WorkspaceFuse {
    /// Workspace ID (kept for debugging/logging)
    #[allow(dead_code)]
    workspace_id: String,
    /// Tokio runtime for async operations
    runtime: Runtime,
    /// RPC client
    rpc: Arc<FileSystemRpcClient>,
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
    /// Readahead state: path → last read block index (for sequential read detection)
    readahead_state: RwLock<HashMap<String, u64>>,
    /// Current uid
    uid: u32,
    /// Current gid
    gid: u32,
}

impl WorkspaceFuse {
    /// Create a new WorkspaceFuse instance
    pub fn new(
        workspace_id: String,
        runtime: Runtime,
        rpc: FileSystemRpcClient,
        cache_ttl: Duration,
        block_size: u32,
        read_cache_size_bytes: u64,
    ) -> Self {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        Self {
            workspace_id,
            runtime,
            rpc: Arc::new(rpc),
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

    /// Convert gRPC Status to errno
    ///
    /// First tries to extract errno from structured error details (tonic-types),
    /// then falls back to status code mapping for compatibility with older servers.
    fn status_to_errno(&self, status: &Status) -> i32 {
        // Try to extract errno from structured error details first
        let details = status.get_error_details();
        if let Some(error_info) = details.error_info() {
            if let Some(errno_str) = error_info.metadata.get("errno") {
                if let Ok(errno) = errno_str.parse::<i32>() {
                    return errno;
                }
            }
        }

        // Fallback: map status code to errno (for compatibility with older servers)
        match status.code() {
            tonic::Code::NotFound => ENOENT,
            tonic::Code::AlreadyExists => EEXIST,
            tonic::Code::PermissionDenied | tonic::Code::Unauthenticated => EACCES,
            tonic::Code::ResourceExhausted => ENOSPC,
            tonic::Code::InvalidArgument => {
                // Legacy: parse message for specific errors
                let msg = status.message();
                if msg.contains("EISDIR") {
                    EISDIR
                } else if msg.contains("ENOTDIR") {
                    ENOTDIR
                } else {
                    EINVAL
                }
            }
            tonic::Code::FailedPrecondition => {
                // Legacy: parse message for specific errors
                let msg = status.message();
                if msg.contains("ENOTEMPTY") {
                    ENOTEMPTY
                } else if msg.contains("EISDIR") {
                    EISDIR
                } else if msg.contains("ENOTDIR") {
                    ENOTDIR
                } else {
                    EIO
                }
            }
            tonic::Code::Unimplemented => ENOSYS,
            tonic::Code::Unavailable | tonic::Code::DeadlineExceeded => EIO,
            _ => EIO,
        }
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
            // Use blocks from server if available, otherwise estimate from size
            // This provides backward compatibility with older servers that don't return blocks
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
    #[allow(clippy::result_large_err)]
    fn get_attr_cached(&self, path: &str) -> Result<FsFileAttr, Status> {
        // Check cache first
        if let Some(cached) = self.meta_cache.get(path) {
            return Ok(cached.attr);
        }

        // Fetch from server
        let rpc = self.rpc.clone();
        let path_owned = path.to_string();
        let attr = self
            .runtime
            .block_on(async move { rpc.stat(&path_owned).await })?;

        // Cache the result
        self.meta_cache.insert(path, attr);

        Ok(attr)
    }
}

impl Filesystem for WorkspaceFuse {
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

        // Build full path
        let path = match self.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // Get attributes
        match self.get_attr_cached(&path) {
            Ok(attr) => {
                let inode = self.inodes.get_or_create(&path);
                let fuse_attr = self.proto_attr_to_fuse(inode, &attr);
                reply.entry(&ATTR_TTL, &fuse_attr, GENERATION);
            }
            Err(status) => {
                let errno = self.status_to_errno(&status);
                debug!(path = %path, errno = errno, "lookup failed: {}", status.message());
                reply.error(errno);
            }
        }
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        debug!(ino = ino, "getattr");

        let path = match self.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        match self.get_attr_cached(&path) {
            Ok(attr) => {
                let fuse_attr = self.proto_attr_to_fuse(ino, &attr);
                reply.attr(&ATTR_TTL, &fuse_attr);
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
            }
        }
    }

    fn setattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        debug!(ino = ino, size = ?size, "setattr");

        let path = match self.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // Convert TimeOrNow to timestamps
        let atime = _atime.map(|t| match t {
            TimeOrNow::SpecificTime(st) => {
                let d = st.duration_since(UNIX_EPOCH).unwrap_or_default();
                prost_types::Timestamp {
                    seconds: d.as_secs() as i64,
                    nanos: d.subsec_nanos() as i32,
                }
            }
            TimeOrNow::Now => {
                let d = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                prost_types::Timestamp {
                    seconds: d.as_secs() as i64,
                    nanos: d.subsec_nanos() as i32,
                }
            }
        });

        let mtime = _mtime.map(|t| match t {
            TimeOrNow::SpecificTime(st) => {
                let d = st.duration_since(UNIX_EPOCH).unwrap_or_default();
                prost_types::Timestamp {
                    seconds: d.as_secs() as i64,
                    nanos: d.subsec_nanos() as i32,
                }
            }
            TimeOrNow::Now => {
                let d = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                prost_types::Timestamp {
                    seconds: d.as_secs() as i64,
                    nanos: d.subsec_nanos() as i32,
                }
            }
        });

        let mode = _mode;

        let rpc = self.rpc.clone();
        let path_owned = path.clone();
        let result = self
            .runtime
            .block_on(async move { rpc.set_attr(&path_owned, size, mode, atime, mtime).await });

        match result {
            Ok(attr) => {
                // Invalidate cache
                self.meta_cache.invalidate(&path);
                if size.is_some() {
                    self.read_cache.invalidate_file(&path);
                }

                let fuse_attr = self.proto_attr_to_fuse(ino, &attr);
                reply.attr(&ATTR_TTL, &fuse_attr);
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
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

        let path = match self.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let offset = offset as u64;
        let size = size as usize;
        let block_size = self.read_cache.block_size() as u64;

        // Calculate block range
        let start_block = self.read_cache.offset_to_block_idx(offset);
        let end_block = if size == 0 {
            start_block
        } else {
            self.read_cache
                .offset_to_block_idx(offset + size as u64 - 1)
        };

        // Check for sequential read pattern and determine if we should prefetch
        let should_prefetch = {
            let readahead = self.readahead_state.read().unwrap();
            if let Some(&last_block) = readahead.get(&path) {
                // Sequential if current start is right after last read block
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
            let block_start = self.read_cache.block_idx_to_offset(block_idx);

            // Get block data (from cache or server)
            let block_data = if let Some(cached) = self.read_cache.get(&path, block_idx) {
                cached
            } else {
                // Fetch from server
                let rpc = self.rpc.clone();
                let path_owned = path.clone();
                let fetch_size = self.read_cache.block_size();
                match self.runtime.block_on(async move {
                    rpc.read_at(&path_owned, block_start, fetch_size).await
                }) {
                    Ok(data) => {
                        // Cache the block
                        self.read_cache.insert(&path, block_idx, data.clone());
                        Arc::new(data)
                    }
                    Err(status) => {
                        reply.error(self.status_to_errno(&status));
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

            // Update current offset for next iteration
            current_offset = block_end;

            // If we've reached EOF (block smaller than block_size), stop
            if block_data.len() < block_size as usize {
                last_block_was_eof = true;
                break;
            }
        }

        // Update readahead state with the last block we read
        {
            let mut readahead = self.readahead_state.write().unwrap();
            readahead.insert(path.clone(), end_block);
        }

        // Prefetch next block asynchronously if sequential read pattern detected and not at EOF
        // This does not block the current read operation
        if should_prefetch && !last_block_was_eof {
            let prefetch_block = end_block + 1;
            // Only prefetch if not already cached
            if self.read_cache.get(&path, prefetch_block).is_none() {
                let rpc = self.rpc.clone();
                let path_owned = path.clone();
                let block_start = self.read_cache.block_idx_to_offset(prefetch_block);
                let fetch_size = self.read_cache.block_size();
                let read_cache = Arc::clone(&self.read_cache);

                // Spawn async prefetch task - does not block current read
                self.runtime.spawn(async move {
                    if let Ok(data) = rpc.read_at(&path_owned, block_start, fetch_size).await {
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

        let path = match self.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let rpc = self.rpc.clone();
        let path_owned = path.clone();
        let data_owned = data.to_vec();
        let offset = offset as u64;

        let result = self
            .runtime
            .block_on(async move { rpc.write_at(&path_owned, offset, &data_owned).await });

        match result {
            Ok(bytes_written) => {
                // Mark file handle as having written data
                {
                    let mut fh_table = self.fh_table.write().unwrap();
                    if let Some(handle) = fh_table.get_mut(&fh) {
                        handle.has_written = true;
                    }
                }

                // Invalidate caches
                self.meta_cache.invalidate(&path);
                self.read_cache.invalidate_file(&path);

                reply.written(bytes_written as u32);
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
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

        let path = match self.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // Check cache
        let entries = if let Some(cached) = self.dir_cache.get(&path) {
            cached
        } else {
            // Fetch from server
            let rpc = self.rpc.clone();
            let path_owned = path.clone();
            match self
                .runtime
                .block_on(async move { rpc.list_dir(&path_owned).await })
            {
                Ok(entries) => {
                    self.dir_cache.insert(&path, entries.clone());
                    Arc::new(entries)
                }
                Err(status) => {
                    reply.error(self.status_to_errno(&status));
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
                // Find parent
                if let Some(parent_path) = path.rsplit_once('/').map(|(p, _)| p.to_string()) {
                    self.inodes.get_or_create(&parent_path)
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
            let child_ino = self.inodes.get_or_create(&child_path);

            // Get file type from attr, default to RegularFile if attr is missing
            let file_type = entry
                .attr
                .as_ref()
                .map(|a| fs_file_type_to_fuse(i32_to_fs_file_type(a.file_type)))
                .unwrap_or(FileType::RegularFile);

            // Cache the metadata if available
            if let Some(ref attr) = entry.attr {
                self.meta_cache.insert(&child_path, *attr);
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

        let path = match self.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let rpc = self.rpc.clone();
        let path_owned = path.clone();
        let result = self
            .runtime
            .block_on(async move { rpc.mkdir(&path_owned, mode).await });

        match result {
            Ok(attr) => {
                // Invalidate parent directory cache
                if let Some(parent_path) = self.inodes.get_path(parent) {
                    self.dir_cache.invalidate(&parent_path);
                }

                let inode = self.inodes.get_or_create(&path);
                let fuse_attr = self.proto_attr_to_fuse(inode, &attr);
                reply.entry(&ATTR_TTL, &fuse_attr, GENERATION);
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
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

        let path = match self.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // O_EXCL check
        let exclusive = (flags & libc::O_EXCL) != 0;

        let rpc = self.rpc.clone();
        let path_owned = path.clone();
        let result = self
            .runtime
            .block_on(async move { rpc.create(&path_owned, mode, exclusive).await });

        match result {
            Ok(attr) => {
                // Invalidate parent directory cache
                if let Some(parent_path) = self.inodes.get_path(parent) {
                    self.dir_cache.invalidate(&parent_path);
                }

                let inode = self.inodes.get_or_create(&path);
                let fh = self.alloc_fh();

                // Register file handle
                {
                    let mut fh_table = self.fh_table.write().unwrap();
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

                let fuse_attr = self.proto_attr_to_fuse(inode, &attr);
                reply.created(&ATTR_TTL, &fuse_attr, GENERATION, fh, 0);
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
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

        let path = match self.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let rpc = self.rpc.clone();
        let path_owned = path.clone();
        let result = self
            .runtime
            .block_on(async move { rpc.remove_file(&path_owned).await });

        match result {
            Ok(()) => {
                // Cleanup
                self.inodes.remove_by_path(&path);
                self.meta_cache.invalidate(&path);
                self.read_cache.invalidate_file(&path);

                if let Some(parent_path) = self.inodes.get_path(parent) {
                    self.dir_cache.invalidate(&parent_path);
                }

                reply.ok();
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
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

        let path = match self.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let rpc = self.rpc.clone();
        let path_owned = path.clone();
        let result = self
            .runtime
            .block_on(async move { rpc.remove_dir(&path_owned).await });

        match result {
            Ok(()) => {
                // Cleanup
                self.inodes.remove_by_path(&path);
                self.meta_cache.invalidate_tree(&path);
                self.dir_cache.invalidate_tree(&path);
                self.read_cache.invalidate_file(&path);

                if let Some(parent_path) = self.inodes.get_path(parent) {
                    self.dir_cache.invalidate(&parent_path);
                }

                reply.ok();
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
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

        let old_path = match self.inodes.build_child_path(parent, name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let new_path = match self.inodes.build_child_path(newparent, newname) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let rpc = self.rpc.clone();
        let old_path_owned = old_path.clone();
        let new_path_owned = new_path.clone();

        // Convert FUSE flags to proto flags
        // RENAME_NOREPLACE = 1 (libc::RENAME_NOREPLACE)
        // RENAME_EXCHANGE = 2 (libc::RENAME_EXCHANGE)
        let proto_flags = if flags & 1 != 0 {
            workspace_proto::FsRenameFlags::Noreplace
        } else if flags & 2 != 0 {
            workspace_proto::FsRenameFlags::Exchange
        } else {
            workspace_proto::FsRenameFlags::None
        };

        let result = self.runtime.block_on(async move {
            rpc.rename_with_flags(&old_path_owned, &new_path_owned, proto_flags)
                .await
        });

        match result {
            Ok(()) => {
                // Update inode mapping
                self.inodes.rename_tree(&old_path, &new_path);

                // Invalidate caches
                self.meta_cache.invalidate_tree(&old_path);
                self.meta_cache.invalidate_tree(&new_path);
                self.dir_cache.invalidate_tree(&old_path);
                self.dir_cache.invalidate_tree(&new_path);
                self.read_cache.invalidate_file(&old_path);

                if let Some(parent_path) = self.inodes.get_path(parent) {
                    self.dir_cache.invalidate(&parent_path);
                }
                if parent != newparent {
                    if let Some(newparent_path) = self.inodes.get_path(newparent) {
                        self.dir_cache.invalidate(&newparent_path);
                    }
                }

                reply.ok();
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
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

        let link_path = match self.inodes.build_child_path(parent, link_name) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let rpc = self.rpc.clone();
        let link_path_owned = link_path.clone();
        let target_owned = target.to_string();
        let result = self
            .runtime
            .block_on(async move { rpc.symlink(&link_path_owned, &target_owned).await });

        match result {
            Ok(attr) => {
                // Invalidate parent directory cache
                if let Some(parent_path) = self.inodes.get_path(parent) {
                    self.dir_cache.invalidate(&parent_path);
                }

                let inode = self.inodes.get_or_create(&link_path);
                let fuse_attr = self.proto_attr_to_fuse(inode, &attr);
                reply.entry(&ATTR_TTL, &fuse_attr, GENERATION);
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
            }
        }
    }

    fn readlink(&mut self, _req: &Request<'_>, ino: u64, reply: fuser::ReplyData) {
        debug!(ino = ino, "readlink");

        let path = match self.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let rpc = self.rpc.clone();
        let path_owned = path.clone();
        let result = self
            .runtime
            .block_on(async move { rpc.read_link(&path_owned).await });

        match result {
            Ok(target) => {
                reply.data(target.as_bytes());
            }
            Err(status) => {
                reply.error(self.status_to_errno(&status));
            }
        }
    }

    fn open(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        debug!(ino = ino, flags = flags, "open");

        // Get path for this inode
        let path = match self.inodes.get_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // Allocate a file handle and register it
        let fh = self.alloc_fh();
        {
            let mut fh_table = self.fh_table.write().unwrap();
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

        // Remove file handle and check if we need to invalidate caches
        let file_handle = {
            let mut fh_table = self.fh_table.write().unwrap();
            fh_table.remove(&fh)
        };

        if let Some(handle) = file_handle {
            // If file was written, invalidate metadata cache (size may have changed)
            if handle.has_written {
                self.meta_cache.invalidate(&handle.path);
            }
            // Clean up readahead state for this file
            {
                let mut readahead = self.readahead_state.write().unwrap();
                readahead.remove(&handle.path);
            }
        }

        reply.ok();
    }

    fn opendir(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        debug!(ino = ino, flags = flags, "opendir");
        let fh = self.alloc_fh();
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
        if let Some(stat) = self.statfs_cache.get() {
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

        // Fetch from server
        let rpc = self.rpc.clone();
        let result = self.runtime.block_on(async move { rpc.stat_fs().await });

        match result {
            Ok(stat) => {
                // Cache the result
                self.statfs_cache.insert(stat);
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
            Err(status) => {
                warn!("statfs failed: {}", status.message());
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
