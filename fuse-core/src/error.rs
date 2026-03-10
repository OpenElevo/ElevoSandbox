//! FUSE error types
//!
//! Provides a unified error type that maps to POSIX errno values for FUSE responses.

/// FUSE filesystem error
#[derive(Debug, thiserror::Error)]
pub enum FuseError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("already exists: {0}")]
    AlreadyExists(String),

    #[error("not a directory: {0}")]
    NotDirectory(String),

    #[error("is a directory: {0}")]
    IsDirectory(String),

    #[error("directory not empty: {0}")]
    NotEmpty(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("I/O error: {0}")]
    IoError(String),

    #[error("path traversal denied: {0}")]
    PathTraversalDenied(String),

    #[error("no space left: {0}")]
    NoSpace(String),

    #[error("operation not supported: {0}")]
    NotSupported(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl FuseError {
    /// Convert to a POSIX errno value for FUSE replies.
    pub fn to_errno(&self) -> i32 {
        match self {
            FuseError::NotFound(_) => libc::ENOENT,
            FuseError::PermissionDenied(_) => libc::EACCES,
            FuseError::AlreadyExists(_) => libc::EEXIST,
            FuseError::NotDirectory(_) => libc::ENOTDIR,
            FuseError::IsDirectory(_) => libc::EISDIR,
            FuseError::NotEmpty(_) => libc::ENOTEMPTY,
            FuseError::InvalidArgument(_) => libc::EINVAL,
            FuseError::IoError(_) => libc::EIO,
            FuseError::PathTraversalDenied(_) => libc::EACCES,
            FuseError::NoSpace(_) => libc::ENOSPC,
            FuseError::NotSupported(_) => libc::ENOSYS,
            FuseError::Internal(_) => libc::EIO,
        }
    }
}
