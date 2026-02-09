//! Inode management for FUSE filesystem
//!
//! Provides bidirectional mapping between file paths and inode numbers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

/// Root inode number (FUSE convention)
pub const ROOT_INODE: u64 = 1;

/// Internal state protected by a single lock to avoid deadlocks
struct InodeTableInner {
    /// Path to inode mapping
    path_to_inode: HashMap<String, u64>,
    /// Inode to path mapping
    inode_to_path: HashMap<u64, String>,
}

/// Inode table for path-to-inode mapping
///
/// This struct manages bidirectional mapping between file paths
/// and inode numbers. All paths are relative to workspace root.
///
/// Design note: Uses a single RwLock to protect both HashMaps,
/// avoiding deadlock risks from multiple locks and ensuring
/// atomic updates to both maps.
pub struct InodeTable {
    /// Next available inode number
    next_inode: AtomicU64,
    /// Internal state protected by a single lock
    inner: RwLock<InodeTableInner>,
}

impl InodeTable {
    /// Create a new inode table
    pub fn new() -> Self {
        let mut inner = InodeTableInner {
            path_to_inode: HashMap::new(),
            inode_to_path: HashMap::new(),
        };

        // Pre-register root
        inner.path_to_inode.insert(String::new(), ROOT_INODE);
        inner.inode_to_path.insert(ROOT_INODE, String::new());

        Self {
            // Start from 2 (root is 1)
            next_inode: AtomicU64::new(2),
            inner: RwLock::new(inner),
        }
    }

    /// Get or create an inode for a path
    ///
    /// If the path already has an inode, returns it.
    /// Otherwise, allocates a new inode and returns it.
    ///
    /// The entire check + insert operation is performed under a single lock
    /// to avoid race conditions where two threads allocate different inodes
    /// for the same path.
    pub fn get_or_create(&self, path: &str) -> u64 {
        // Normalize path (remove trailing slash, handle "." and "..")
        let path = normalize_path(path);

        // Fast path: check if already exists with read lock
        {
            let inner = self.inner.read().unwrap();
            if let Some(&inode) = inner.path_to_inode.get(&path) {
                return inode;
            }
        }

        // Slow path: acquire write lock and double-check
        let mut inner = self.inner.write().unwrap();

        // Double-check (another thread might have created it)
        if let Some(&existing) = inner.path_to_inode.get(&path) {
            return existing;
        }

        // Allocate new inode
        let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);

        // Register in both maps atomically
        inner.path_to_inode.insert(path.clone(), inode);
        inner.inode_to_path.insert(inode, path);

        inode
    }

    /// Get the path for an inode
    pub fn get_path(&self, inode: u64) -> Option<String> {
        let inner = self.inner.read().unwrap();
        inner.inode_to_path.get(&inode).cloned()
    }

    /// Get the inode for a path (without creating)
    #[cfg(test)]
    pub fn get_inode(&self, path: &str) -> Option<u64> {
        let path = normalize_path(path);
        let inner = self.inner.read().unwrap();
        inner.path_to_inode.get(&path).copied()
    }

    /// Remove a path from the inode table
    ///
    /// If the path is a directory, also removes all child paths to prevent
    /// memory leaks from orphaned inode mappings.
    pub fn remove_by_path(&self, path: &str) {
        let path = normalize_path(path);
        let mut inner = self.inner.write().unwrap();

        // Collect paths to remove: the path itself and all children
        // For non-empty paths, use "path/" prefix to match children
        let prefix = if path.is_empty() {
            // Empty path is root - don't remove root itself, but this case
            // shouldn't happen in practice
            return;
        } else {
            format!("{}/", path)
        };

        let paths_to_remove: Vec<String> = inner
            .path_to_inode
            .keys()
            .filter(|p| *p == &path || p.starts_with(&prefix))
            .cloned()
            .collect();

        // Remove all collected paths atomically
        for p in paths_to_remove {
            if let Some(inode) = inner.path_to_inode.remove(&p) {
                inner.inode_to_path.remove(&inode);
            }
        }
    }

    /// Rename a path (update both mappings)
    #[cfg(test)]
    pub fn rename(&self, old_path: &str, new_path: &str) {
        let old_path = normalize_path(old_path);
        let new_path = normalize_path(new_path);

        let mut inner = self.inner.write().unwrap();

        if let Some(inode) = inner.path_to_inode.remove(&old_path) {
            inner.path_to_inode.insert(new_path.clone(), inode);
            inner.inode_to_path.insert(inode, new_path);
        }
    }

    /// Rename a directory and all its children
    ///
    /// POSIX rename semantics: if new_dir already exists, it is atomically replaced.
    /// This method removes any existing mappings for new_dir and its children before
    /// renaming old_dir to new_dir.
    pub fn rename_tree(&self, old_dir: &str, new_dir: &str) {
        let old_dir = normalize_path(old_dir);
        let new_dir = normalize_path(new_dir);

        // Renaming root is not allowed
        if old_dir.is_empty() {
            return;
        }

        let old_prefix = format!("{}/", old_dir);
        let new_prefix = if new_dir.is_empty() {
            String::new()
        } else {
            format!("{}/", new_dir)
        };

        let mut inner = self.inner.write().unwrap();

        // POSIX rename semantics: remove existing destination and its children first
        // This prevents inode leaks when overwriting existing files/directories
        if !new_dir.is_empty() {
            let paths_to_remove: Vec<String> = inner
                .path_to_inode
                .keys()
                .filter(|path| *path == &new_dir || path.starts_with(&new_prefix))
                .cloned()
                .collect();

            for path in paths_to_remove {
                if let Some(inode) = inner.path_to_inode.remove(&path) {
                    inner.inode_to_path.remove(&inode);
                }
            }
        }

        // Collect paths to rename
        let paths_to_rename: Vec<(String, u64)> = inner
            .path_to_inode
            .iter()
            .filter(|(path, _)| *path == &old_dir || path.starts_with(&old_prefix))
            .map(|(path, &inode)| (path.clone(), inode))
            .collect();

        // Rename each path atomically
        for (old_path, inode) in paths_to_rename {
            inner.path_to_inode.remove(&old_path);

            let new_path = if old_path == old_dir {
                new_dir.clone()
            } else {
                format!(
                    "{}{}",
                    new_prefix,
                    old_path.strip_prefix(&old_prefix).unwrap_or(&old_path)
                )
            };

            inner.path_to_inode.insert(new_path.clone(), inode);
            inner.inode_to_path.insert(inode, new_path);
        }
    }

    /// Build a child path from parent path and name
    pub fn build_child_path(&self, parent_inode: u64, name: &str) -> Option<String> {
        let parent_path = self.get_path(parent_inode)?;
        Some(join_path(&parent_path, name))
    }
}

impl Default for InodeTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a path for consistent mapping
///
/// Handles:
/// - Leading/trailing slashes
/// - "." (current directory)
/// - ".." (parent directory)
/// - Multiple consecutive slashes
fn normalize_path(path: &str) -> String {
    let mut components: Vec<&str> = Vec::new();

    for part in path.split('/') {
        match part {
            "" | "." => {
                // Skip empty parts (from leading/trailing/consecutive slashes) and "."
                continue;
            }
            ".." => {
                // Go up one level if possible
                components.pop();
            }
            _ => {
                components.push(part);
            }
        }
    }

    components.join("/")
}

/// Join parent path and child name
pub fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_inode() {
        let table = InodeTable::new();
        assert_eq!(table.get_inode(""), Some(ROOT_INODE));
        assert_eq!(table.get_path(ROOT_INODE), Some(String::new()));
    }

    #[test]
    fn test_get_or_create() {
        let table = InodeTable::new();

        let inode1 = table.get_or_create("src/main.rs");
        let inode2 = table.get_or_create("src/main.rs");
        let inode3 = table.get_or_create("src/lib.rs");

        assert_eq!(inode1, inode2);
        assert_ne!(inode1, inode3);
        assert_ne!(inode1, ROOT_INODE);
    }

    #[test]
    fn test_path_normalization() {
        let table = InodeTable::new();

        let inode1 = table.get_or_create("/src/main.rs");
        let inode2 = table.get_or_create("src/main.rs/");
        let inode3 = table.get_or_create("/src/main.rs/");

        assert_eq!(inode1, inode2);
        assert_eq!(inode2, inode3);
    }

    #[test]
    fn test_remove_by_path() {
        let table = InodeTable::new();

        let inode = table.get_or_create("test.txt");
        assert!(table.get_path(inode).is_some());

        table.remove_by_path("test.txt");
        assert!(table.get_path(inode).is_none());
        assert!(table.get_inode("test.txt").is_none());
    }

    #[test]
    fn test_rename() {
        let table = InodeTable::new();

        let inode = table.get_or_create("old.txt");
        table.rename("old.txt", "new.txt");

        assert!(table.get_inode("old.txt").is_none());
        assert_eq!(table.get_inode("new.txt"), Some(inode));
        assert_eq!(table.get_path(inode), Some("new.txt".to_string()));
    }

    #[test]
    fn test_rename_tree() {
        let table = InodeTable::new();

        let dir_inode = table.get_or_create("olddir");
        let file_inode = table.get_or_create("olddir/file.txt");
        let subdir_inode = table.get_or_create("olddir/subdir");
        let subfile_inode = table.get_or_create("olddir/subdir/nested.txt");

        table.rename_tree("olddir", "newdir");

        assert!(table.get_inode("olddir").is_none());
        assert!(table.get_inode("olddir/file.txt").is_none());

        assert_eq!(table.get_inode("newdir"), Some(dir_inode));
        assert_eq!(table.get_inode("newdir/file.txt"), Some(file_inode));
        assert_eq!(table.get_inode("newdir/subdir"), Some(subdir_inode));
        assert_eq!(table.get_inode("newdir/subdir/nested.txt"), Some(subfile_inode));
    }

    #[test]
    fn test_build_child_path() {
        let table = InodeTable::new();

        // Root's child
        let child_path = table.build_child_path(ROOT_INODE, "test.txt");
        assert_eq!(child_path, Some("test.txt".to_string()));

        // Directory's child
        let dir_inode = table.get_or_create("src");
        let child_path = table.build_child_path(dir_inode, "main.rs");
        assert_eq!(child_path, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_normalize_path_with_dotdot() {
        // Test ".." handling
        assert_eq!(normalize_path("foo/../bar"), "bar");
        assert_eq!(normalize_path("./foo/../bar"), "bar");
        assert_eq!(normalize_path("a/b/c/../../d"), "a/d");
        assert_eq!(normalize_path("a/b/../c/../d"), "a/d");

        // ".." at root level should be ignored (can't go above root)
        assert_eq!(normalize_path("../foo"), "foo");
        assert_eq!(normalize_path("../../foo/bar"), "foo/bar");

        // Multiple consecutive slashes
        assert_eq!(normalize_path("foo//bar///baz"), "foo/bar/baz");

        // "." handling
        assert_eq!(normalize_path("./foo/./bar/."), "foo/bar");
        assert_eq!(normalize_path("."), "");

        // Combined edge cases
        assert_eq!(normalize_path("/./foo/../bar/./baz/../qux/"), "bar/qux");
    }

    #[test]
    fn test_inode_with_dotdot_paths() {
        let table = InodeTable::new();

        // These should all resolve to the same inode
        let inode1 = table.get_or_create("src/main.rs");
        let inode2 = table.get_or_create("src/lib/../main.rs");
        let inode3 = table.get_or_create("./src/main.rs");
        let inode4 = table.get_or_create("foo/../src/main.rs");

        assert_eq!(inode1, inode2);
        assert_eq!(inode2, inode3);
        assert_eq!(inode3, inode4);
    }
}
