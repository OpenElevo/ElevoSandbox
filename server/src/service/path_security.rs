//! Path security sanitization
//!
//! Ensures all file paths are safe and contained within expected root directories.

use std::path::{Component, Path, PathBuf};

use crate::error::Error;

/// Sanitize a user-provided path and ensure it stays within the given root directory.
///
/// Used by Share file handlers and Namespace file operations.
#[allow(dead_code)]
///
/// Rules:
/// - Eliminates `.`, `..`, and duplicate `/`
/// - Forbids NULL bytes
/// - Ensures the final resolved path is within the root
pub fn sanitize_path(root: &Path, user_path: &str) -> Result<PathBuf, Error> {
    // Reject NULL bytes
    if user_path.contains('\0') {
        return Err(Error::InvalidParameter(
            "Path contains null bytes".to_string(),
        ));
    }

    // Normalize the user path: resolve `.` and `..` without touching the filesystem
    let normalized = normalize_path(user_path);

    // Join with root
    let full_path = root.join(&normalized);

    // Canonicalize the root (it must exist)
    let canonical_root = root.canonicalize().map_err(|e| {
        Error::InvalidParameter(format!("Root directory does not exist: {}", e))
    })?;

    // Try to canonicalize the full path; if it doesn't exist yet, canonicalize the parent
    let canonical_full = if full_path.exists() {
        full_path.canonicalize().map_err(|e| {
            Error::InvalidParameter(format!("Path resolution failed: {}", e))
        })?
    } else {
        // For new files, canonicalize the parent and append the filename
        let parent = full_path.parent().ok_or_else(|| {
            Error::InvalidParameter("Invalid path".to_string())
        })?;
        let file_name = full_path.file_name().ok_or_else(|| {
            Error::InvalidParameter("Invalid path".to_string())
        })?;
        let canonical_parent = parent.canonicalize().map_err(|e| {
            Error::InvalidParameter(format!(
                "Parent directory does not exist: {}",
                e
            ))
        })?;
        canonical_parent.join(file_name)
    };

    // Ensure the resolved path is within the root
    if !canonical_full.starts_with(&canonical_root) {
        return Err(Error::InvalidParameter(
            "Path traversal detected: path escapes root directory".to_string(),
        ));
    }

    Ok(canonical_full)
}

/// Sanitize a path for share file operations.
/// Ensures the path stays within the share's source_path inside the namespace root.
#[allow(dead_code)]
pub fn sanitize_share_path(
    namespace_root: &Path,
    source_path: &str,
    user_path: &str,
) -> Result<PathBuf, Error> {
    // The share root is namespace_root/source_path
    let share_root = namespace_root.join(normalize_path(source_path));
    sanitize_path(&share_root, user_path)
}

/// Normalize a path string by resolving `.` and `..` components
/// without touching the filesystem.
fn normalize_path(path: &str) -> PathBuf {
    let path = Path::new(path);
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(p) => components.push(Component::Prefix(p)),
            Component::RootDir => {
                // Skip leading `/` — we treat all paths as relative
            }
            Component::CurDir => {
                // Skip `.`
            }
            Component::ParentDir => {
                // Pop last component if possible
                if !components.is_empty() {
                    components.pop();
                }
                // If empty, just ignore (don't go above root)
            }
            Component::Normal(c) => components.push(Component::Normal(c)),
        }
    }

    if components.is_empty() {
        PathBuf::from(".")
    } else {
        components.iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_normalize_path_basic() {
        assert_eq!(normalize_path("foo/bar"), PathBuf::from("foo/bar"));
        assert_eq!(normalize_path("./foo/bar"), PathBuf::from("foo/bar"));
        assert_eq!(normalize_path("foo/../bar"), PathBuf::from("bar"));
        assert_eq!(normalize_path("../foo"), PathBuf::from("foo"));
        assert_eq!(normalize_path("/foo/bar"), PathBuf::from("foo/bar"));
        assert_eq!(normalize_path("foo//bar"), PathBuf::from("foo/bar"));
    }

    #[test]
    fn test_normalize_path_dots() {
        assert_eq!(normalize_path("a/b/../c/./d"), PathBuf::from("a/c/d"));
        assert_eq!(normalize_path("../../.."), PathBuf::from("."));
    }

    #[test]
    fn test_sanitize_path_valid() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("subdir")).unwrap();

        let result = sanitize_path(root, "subdir").unwrap();
        assert!(result.starts_with(root.canonicalize().unwrap()));
    }

    #[test]
    fn test_sanitize_path_traversal_blocked() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("subdir")).unwrap();

        let result = sanitize_path(root, "../../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_path_null_byte_rejected() {
        let tmp = TempDir::new().unwrap();
        let result = sanitize_path(tmp.path(), "foo\0bar");
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_share_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("shared/data")).unwrap();

        let result =
            sanitize_share_path(root, "shared/data", "file.txt").unwrap();
        assert!(result.starts_with(root.canonicalize().unwrap()));
    }
}
