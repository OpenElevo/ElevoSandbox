//! Path security sanitization
//!
//! Provides path normalization and sanitization to prevent directory traversal attacks.
//! Used by share file handlers and namespace file operations.

use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

/// Normalize a path string by resolving `.` components and stripping leading `/`.
///
/// Unlike the previous silent-strip behaviour, `..` components are **rejected**
/// outright with an error.  Callers that previously relied on `..` being
/// neutralised should either pre-validate their input or use
/// `sanitize_path` / `sanitize_share_path` which already reject traversal.
pub fn normalize_path(path: &str) -> Result<PathBuf> {
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
                return Err(Error::PathNotAllowed(
                    "Path must not contain '..' components".to_string(),
                ));
            }
            Component::Normal(c) => components.push(Component::Normal(c)),
        }
    }

    if components.is_empty() {
        Ok(PathBuf::from("."))
    } else {
        Ok(components.iter().collect())
    }
}

/// Sanitize a user-provided path against a root directory.
///
/// Ensures the resulting path stays within `root` after normalization.
/// Returns the full joined path (root + normalized user path).
///
/// Rejects:
/// - Paths containing null bytes
/// - Paths that contain `..` components (returns `Error::PathNotAllowed`)
///
/// When the normalized path is `.` (empty/root input), returns `root` itself
/// rather than `root/.` to avoid surprising trailing-dot paths.
pub fn sanitize_path(root: &Path, user_path: &str) -> Result<PathBuf> {
    // Reject null bytes
    if user_path.contains('\0') {
        return Err(Error::PathNotAllowed(
            "path contains null bytes".to_string(),
        ));
    }

    // Normalize the user path (resolves `.`, rejects `..`, strips leading `/`)
    let normalized = normalize_path(user_path)?;

    // When the input resolves to the current directory, return root itself
    // so callers receive `/root` rather than the misleading `/root/.`
    if normalized == Path::new(".") {
        return Ok(root.to_path_buf());
    }

    // Join root + normalized path
    let full = root.join(&normalized);

    // Verify the result is within root (defence-in-depth: normalize_path
    // already rejects `..`, but this guard catches any future edge cases)
    if !full.starts_with(root) {
        return Err(Error::PathNotAllowed(format!(
            "path escapes root directory: {}",
            user_path
        )));
    }

    Ok(full)
}

/// Sanitize a user-provided path against a share's source directory within a namespace.
///
/// Ensures the resulting path stays within `namespace_root/source_path` after normalization.
/// Returns the full joined path.
pub fn sanitize_share_path(
    namespace_root: &Path,
    source_path: &str,
    user_path: &str,
) -> Result<PathBuf> {
    // First, normalize source_path and build the share root (rejects `..` in source too)
    let normalized_source = normalize_path(source_path)?;
    let share_root = namespace_root.join(&normalized_source);

    // Verify share_root is within namespace_root (defence-in-depth)
    if !share_root.starts_with(namespace_root) {
        return Err(Error::PathNotAllowed(format!(
            "share source_path escapes namespace: {}",
            source_path
        )));
    }

    // Now sanitize the user path against the share root
    sanitize_path(&share_root, user_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_basic() {
        assert_eq!(normalize_path("foo/bar").unwrap(), PathBuf::from("foo/bar"));
        assert_eq!(
            normalize_path("./foo/bar").unwrap(),
            PathBuf::from("foo/bar")
        );
        assert_eq!(
            normalize_path("/foo/bar").unwrap(),
            PathBuf::from("foo/bar")
        );
        assert_eq!(
            normalize_path("foo//bar").unwrap(),
            PathBuf::from("foo/bar")
        );
    }

    #[test]
    fn test_normalize_path_parent_dir_rejected() {
        // Any path component containing `..` must be an error
        assert!(normalize_path("foo/../bar").is_err());
        assert!(normalize_path("../foo").is_err());
        assert!(normalize_path("../../..").is_err());
        assert!(normalize_path("a/b/../c/./d").is_err());
    }

    #[test]
    fn test_normalize_path_empty_and_root() {
        assert_eq!(normalize_path("").unwrap(), PathBuf::from("."));
        assert_eq!(normalize_path("/").unwrap(), PathBuf::from("."));
        assert_eq!(normalize_path(".").unwrap(), PathBuf::from("."));
    }

    // ── sanitize_path tests ──

    #[test]
    fn test_sanitize_path_normal() {
        let root = Path::new("/data/namespaces/tenant1");
        let result = sanitize_path(root, "foo/bar.txt").unwrap();
        assert_eq!(
            result,
            PathBuf::from("/data/namespaces/tenant1/foo/bar.txt")
        );
    }

    #[test]
    fn test_sanitize_path_with_leading_slash() {
        let root = Path::new("/data/namespaces/tenant1");
        let result = sanitize_path(root, "/foo/bar.txt").unwrap();
        assert_eq!(
            result,
            PathBuf::from("/data/namespaces/tenant1/foo/bar.txt")
        );
    }

    #[test]
    fn test_sanitize_path_traversal_rejected() {
        let root = Path::new("/data/namespaces/tenant1");
        // `..` components must be rejected with an error
        let result = sanitize_path(root, "../../etc/passwd");
        assert!(result.is_err());
        let result2 = sanitize_path(root, "../secret");
        assert!(result2.is_err());
    }

    #[test]
    fn test_sanitize_path_null_byte_rejected() {
        let root = Path::new("/data/namespaces/tenant1");
        let result = sanitize_path(root, "foo\0bar");
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_path_empty_gives_root() {
        let root = Path::new("/data/namespaces/tenant1");
        let result = sanitize_path(root, "").unwrap();
        assert_eq!(result, PathBuf::from("/data/namespaces/tenant1"));
    }

    #[test]
    fn test_sanitize_path_root_slash_gives_root() {
        let root = Path::new("/data/namespaces/tenant1");
        let result = sanitize_path(root, "/").unwrap();
        assert_eq!(result, PathBuf::from("/data/namespaces/tenant1"));
    }

    // ── sanitize_share_path tests ──

    #[test]
    fn test_sanitize_share_path_normal() {
        let ns_root = Path::new("/data/namespaces/tenant1");
        let result = sanitize_share_path(ns_root, "shared/project", "src/main.rs").unwrap();
        assert_eq!(
            result,
            PathBuf::from("/data/namespaces/tenant1/shared/project/src/main.rs")
        );
    }

    #[test]
    fn test_sanitize_share_path_user_traversal_rejected() {
        let ns_root = Path::new("/data/namespaces/tenant1");
        // `..` in user_path must be an error, not silently neutralised
        let result = sanitize_share_path(ns_root, "shared/project", "../../secret");
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_share_path_source_traversal_rejected() {
        let ns_root = Path::new("/data/namespaces/tenant1");
        // `..` in source_path must also be an error
        let result = sanitize_share_path(ns_root, "../../etc", "passwd");
        assert!(result.is_err());
    }
}
