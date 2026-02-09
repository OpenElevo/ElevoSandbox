//! Shared protobuf definitions for Elevo Workspace
//!
//! This crate provides compiled protobuf definitions with both server
//! and client code for use by workspace-server and fuse-client.

/// Generated protobuf modules
pub mod gen {
    /// Workspace API v1
    pub mod workspace {
        pub mod v1 {
            include!("gen/workspace.v1.rs");
        }
    }
}

// Re-export commonly used types at crate root for convenience
pub use gen::workspace::v1::*;
