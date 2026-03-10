//! Server-side FUSE mount management for remote workspaces
//!
//! When a Client connects and provides remote storage via gRPC,
//! the server creates a FUSE mount point that makes the remote
//! storage accessible as a local directory tree.

pub mod backend;
pub mod monitor;
pub mod mount;
