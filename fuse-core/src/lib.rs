//! Shared FUSE filesystem core for Elevo Workspace
//!
//! Provides a generic FUSE filesystem implementation parameterized by a `FuseBackend` trait.
//! Used by both the standalone fuse-client (via RPC) and the server-side FUSE mount (via
//! `RemoteStorageBackend`).

pub mod backend;
pub mod cache;
pub mod error;
pub mod filesystem;
pub mod inode;
