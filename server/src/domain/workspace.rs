//! Workspace domain model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Workspace entity
///
/// A workspace is an independent, persistent working directory that can be
/// mounted by multiple sandboxes. It manages its own lifecycle and NFS export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// Unique identifier
    pub id: String,

    /// Optional human-readable name
    pub name: Option<String>,

    /// NFS mount URL (if available)
    pub nfs_url: Option<String>,

    /// Custom metadata
    pub metadata: HashMap<String, String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl Workspace {
    /// Create a new workspace
    pub fn new(id: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            name: None,
            nfs_url: None,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the NFS URL
    pub fn set_nfs_url(&mut self, nfs_url: String) {
        self.nfs_url = Some(nfs_url);
        self.updated_at = Utc::now();
    }
}

/// Parameters for creating a workspace
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateWorkspaceParams {
    /// Optional name
    pub name: Option<String>,

    /// Custom metadata
    pub metadata: Option<HashMap<String, String>>,
}
