//! Workspace domain model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Storage type: how a workspace's files are stored
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
    /// Server-managed storage (local disk or S3)
    #[default]
    Managed,
    /// Client-provided remote storage (gRPC reverse stream or NFS)
    Remote,
}

impl StorageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageType::Managed => "managed",
            StorageType::Remote => "remote",
        }
    }

    pub fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "managed" => Ok(StorageType::Managed),
            "remote" => Ok(StorageType::Remote),
            _ => Err(format!("unknown storage type: {}", s)),
        }
    }
}


/// Transport channel for remote storage
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteTransport {
    /// gRPC bidirectional stream (default for remote)
    Grpc,
    /// NFS mount from Client
    Nfs,
}

/// Phase of an in-progress channel switch
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchPhase {
    /// Switch initiated, new channel not yet mounted
    Pending,
    /// New channel mounted at temporary path, ready for cutover
    Mounted,
}

/// Configuration for remote storage (persisted as JSON in DB `storage_config`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStorageConfig {
    /// Schema version (currently 1)
    pub v: u32,
    /// Current transport channel
    pub transport: RemoteTransport,
    /// NFS URL (only set when transport=Nfs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_url: Option<String>,
    /// Target transport during an in-progress channel switch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switching_to: Option<RemoteTransport>,
    /// Current phase of the channel switch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_phase: Option<SwitchPhase>,
}

impl Default for RemoteStorageConfig {
    fn default() -> Self {
        Self {
            v: 1,
            transport: RemoteTransport::Grpc,
            nfs_url: None,
            switching_to: None,
            switch_phase: None,
        }
    }
}

impl RemoteStorageConfig {
    /// Validate the config version. Returns error if version is unsupported.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.v != 1 {
            return Err(format!("unsupported storage config version: {}", self.v));
        }
        Ok(())
    }

    /// Check if a channel switch is in progress
    pub fn is_switching(&self) -> bool {
        self.switching_to.is_some()
    }
}

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

    /// Storage type: managed (Server) or remote (Client)
    pub storage_type: StorageType,

    /// Remote storage configuration (only meaningful when storage_type=Remote)
    pub storage_config: RemoteStorageConfig,

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
            storage_type: StorageType::Managed,
            storage_config: RemoteStorageConfig::default(),
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

    /// Check if this is a remote workspace
    pub fn is_remote(&self) -> bool {
        self.storage_type == StorageType::Remote
    }
}

/// Parameters for creating a workspace
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateWorkspaceParams {
    /// Optional name
    pub name: Option<String>,

    /// Storage type (defaults to Managed if not specified)
    pub storage_type: Option<StorageType>,

    /// Custom metadata
    pub metadata: Option<HashMap<String, String>>,
}
