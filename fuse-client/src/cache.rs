//! Caching layer for FUSE filesystem
//!
//! Provides multi-level caching for metadata, directory listings, and file data.

use std::sync::Arc;
use std::time::Duration;

use moka::sync::Cache;
use workspace_proto::{FsFileAttr, FsFileType, FsStatFsResponse};

use crate::rpc::DirEntry;

/// Default statfs cache TTL (30 seconds)
const STATFS_CACHE_TTL: Duration = Duration::from_secs(30);

/// Cached file attributes
#[derive(Clone, Debug)]
pub struct CachedAttr {
    pub attr: FsFileAttr,
}

/// Metadata cache for file attributes
pub struct MetadataCache {
    cache: Cache<String, CachedAttr>,
}

impl MetadataCache {
    /// Create a new metadata cache with the given TTL
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(ttl)
                .build(),
        }
    }

    /// Get cached attributes for a path
    pub fn get(&self, path: &str) -> Option<CachedAttr> {
        self.cache.get(path)
    }

    /// Cache attributes for a path
    pub fn insert(&self, path: &str, attr: FsFileAttr) {
        self.cache.insert(path.to_string(), CachedAttr { attr });
    }

    /// Invalidate a specific path
    pub fn invalidate(&self, path: &str) {
        self.cache.invalidate(path);
    }

    /// Invalidate all entries under a directory
    ///
    /// Note: moka doesn't support prefix invalidation directly, so we need to
    /// iterate through all entries. For large caches this could be expensive,
    /// but it ensures correctness.
    pub fn invalidate_tree(&self, dir_path: &str) {
        // Invalidate the directory itself
        self.cache.invalidate(dir_path);

        // Invalidate all children by iterating through the cache
        let prefix = if dir_path.is_empty() {
            String::new()
        } else {
            format!("{}/", dir_path)
        };

        // Collect keys to invalidate (can't invalidate while iterating)
        let keys_to_invalidate: Vec<String> = self
            .cache
            .iter()
            .filter_map(|(k, _)| {
                let key: &String = &k;
                if key.starts_with(&prefix) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys_to_invalidate {
            self.cache.invalidate(&key);
        }
    }
}

/// Directory entry cache
pub struct DirCache {
    cache: Cache<String, Arc<Vec<DirEntry>>>,
}

impl DirCache {
    /// Create a new directory cache with the given TTL
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(1_000)
                .time_to_live(ttl)
                .build(),
        }
    }

    /// Get cached directory entries
    pub fn get(&self, path: &str) -> Option<Arc<Vec<DirEntry>>> {
        self.cache.get(path)
    }

    /// Cache directory entries
    pub fn insert(&self, path: &str, entries: Vec<DirEntry>) {
        self.cache.insert(path.to_string(), Arc::new(entries));
    }

    /// Invalidate a specific directory
    pub fn invalidate(&self, path: &str) {
        self.cache.invalidate(path);
    }

    /// Invalidate all directories under a path
    ///
    /// Note: moka doesn't support prefix invalidation directly, so we need to
    /// iterate through all entries.
    pub fn invalidate_tree(&self, path: &str) {
        // Invalidate the directory itself
        self.cache.invalidate(path);

        // Invalidate all children by iterating through the cache
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{}/", path)
        };

        // Collect keys to invalidate
        let keys_to_invalidate: Vec<String> = self
            .cache
            .iter()
            .filter_map(|(k, _)| {
                let key: &String = &k;
                if key.starts_with(&prefix) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys_to_invalidate {
            self.cache.invalidate(&key);
        }
    }
}

/// Read cache key: (path, block_index)
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ReadCacheKey {
    pub path: String,
    pub block_idx: u64,
}

/// Default read cache size in bytes (64MB)
const DEFAULT_READ_CACHE_SIZE: u64 = 64 * 1024 * 1024;

/// Read cache for file content blocks
///
/// Caches file data in fixed-size blocks for efficient random access.
pub struct ReadCache {
    cache: Cache<ReadCacheKey, Arc<Vec<u8>>>,
    block_size: u32,
}

impl ReadCache {
    /// Create a new read cache with the given block size and default max size (64MB)
    #[allow(dead_code)]
    pub fn new(block_size: u32) -> Self {
        Self::with_max_size(block_size, DEFAULT_READ_CACHE_SIZE)
    }

    /// Create a new read cache with custom max size in bytes
    pub fn with_max_size(block_size: u32, max_size_bytes: u64) -> Self {
        Self {
            cache: Cache::builder()
                // When using weigher, max_capacity is the total weight limit (in bytes)
                .max_capacity(max_size_bytes)
                .weigher(|_key: &ReadCacheKey, value: &Arc<Vec<u8>>| {
                    // Weight by actual data size, minimum 1 to avoid zero-weight entries
                    (value.len() as u32).max(1)
                })
                .build(),
            block_size,
        }
    }

    /// Get the block size
    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    /// Get a cached block
    pub fn get(&self, path: &str, block_idx: u64) -> Option<Arc<Vec<u8>>> {
        self.cache.get(&ReadCacheKey {
            path: path.to_string(),
            block_idx,
        })
    }

    /// Cache a block
    pub fn insert(&self, path: &str, block_idx: u64, data: Vec<u8>) {
        self.cache.insert(
            ReadCacheKey {
                path: path.to_string(),
                block_idx,
            },
            Arc::new(data),
        );
    }

    /// Invalidate all blocks for a file
    pub fn invalidate_file(&self, path: &str) {
        // Collect keys to invalidate
        let keys_to_invalidate: Vec<ReadCacheKey> = self
            .cache
            .iter()
            .filter_map(|(k, _)| {
                let key: &ReadCacheKey = &k;
                if key.path == path {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in keys_to_invalidate {
            self.cache.invalidate(&key);
        }
    }

    /// Calculate block index for an offset
    pub fn offset_to_block_idx(&self, offset: u64) -> u64 {
        offset / self.block_size as u64
    }

    /// Calculate the starting offset for a block
    pub fn block_idx_to_offset(&self, block_idx: u64) -> u64 {
        block_idx * self.block_size as u64
    }
}

/// Statfs cache for filesystem statistics
///
/// Caches statfs results with a 30-second TTL to reduce server load.
/// Uses a single-entry cache since there's only one statfs result per workspace.
pub struct StatfsCache {
    cache: Cache<(), FsStatFsResponse>,
}

impl StatfsCache {
    /// Create a new statfs cache with default TTL (30 seconds)
    pub fn new() -> Self {
        Self::with_ttl(STATFS_CACHE_TTL)
    }

    /// Create a new statfs cache with custom TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(1)
                .time_to_live(ttl)
                .build(),
        }
    }

    /// Get cached statfs result
    pub fn get(&self) -> Option<FsStatFsResponse> {
        self.cache.get(&())
    }

    /// Cache statfs result
    pub fn insert(&self, stat: FsStatFsResponse) {
        self.cache.insert((), stat);
    }

    /// Invalidate cached statfs result
    #[allow(dead_code)]
    pub fn invalidate(&self) {
        self.cache.invalidate(&());
    }
}

impl Default for StatfsCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert FsFileType to libc file type
pub fn fs_file_type_to_fuse(ft: FsFileType) -> fuser::FileType {
    match ft {
        FsFileType::File => fuser::FileType::RegularFile,
        FsFileType::Directory => fuser::FileType::Directory,
        FsFileType::Symlink => fuser::FileType::Symlink,
        FsFileType::Unspecified => fuser::FileType::RegularFile, // Default to regular file
    }
}

/// Convert i32 file type to FsFileType
pub fn i32_to_fs_file_type(ft: i32) -> FsFileType {
    FsFileType::try_from(ft).unwrap_or(FsFileType::Unspecified)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_cache() {
        let cache = MetadataCache::new(Duration::from_secs(60));

        let attr = FsFileAttr {
            file_type: FsFileType::File.into(),
            size: 100,
            mode: 0o644,
            uid: 1000,
            gid: 1000,
            atime: None,
            mtime: None,
            ctime: None,
            nlink: 1,
            blksize: 4096,
            blocks: 1,
        };

        cache.insert("test.txt", attr.clone());

        let cached = cache.get("test.txt");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().attr.size, 100);

        cache.invalidate("test.txt");
        assert!(cache.get("test.txt").is_none());
    }

    #[test]
    fn test_dir_cache() {
        let cache = DirCache::new(Duration::from_secs(60));

        let entries = vec![
            DirEntry {
                name: "file1.txt".to_string(),
                attr: Some(FsFileAttr {
                    file_type: FsFileType::File.into(),
                    size: 100,
                    mode: 0o644,
                    uid: 1000,
                    gid: 1000,
                    atime: None,
                    mtime: None,
                    ctime: None,
                    nlink: 1,
                    blksize: 4096,
                    blocks: 1,
                }),
            },
            DirEntry {
                name: "dir1".to_string(),
                attr: Some(FsFileAttr {
                    file_type: FsFileType::Directory.into(),
                    size: 4096,
                    mode: 0o755,
                    uid: 1000,
                    gid: 1000,
                    atime: None,
                    mtime: None,
                    ctime: None,
                    nlink: 2,
                    blksize: 4096,
                    blocks: 8,
                }),
            },
        ];

        cache.insert("src", entries);

        let cached = cache.get("src");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 2);

        cache.invalidate("src");
        assert!(cache.get("src").is_none());
    }

    #[test]
    fn test_read_cache() {
        let cache = ReadCache::new(64 * 1024);

        let data = vec![0u8; 1024];
        cache.insert("test.txt", 0, data.clone());

        let cached = cache.get("test.txt", 0);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1024);

        assert!(cache.get("test.txt", 1).is_none());

        cache.invalidate_file("test.txt");
        assert!(cache.get("test.txt", 0).is_none());
    }

    #[test]
    fn test_block_calculations() {
        let cache = ReadCache::new(64 * 1024); // 64KB blocks

        assert_eq!(cache.offset_to_block_idx(0), 0);
        assert_eq!(cache.offset_to_block_idx(65535), 0);
        assert_eq!(cache.offset_to_block_idx(65536), 1);
        assert_eq!(cache.offset_to_block_idx(131072), 2);

        assert_eq!(cache.block_idx_to_offset(0), 0);
        assert_eq!(cache.block_idx_to_offset(1), 65536);
        assert_eq!(cache.block_idx_to_offset(2), 131072);
    }
}
