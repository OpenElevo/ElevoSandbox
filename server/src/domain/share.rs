//! Share domain model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Share visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

impl Visibility {
    pub fn from_str_value(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Self::Public),
            "private" => Some(Self::Private),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
        }
    }
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Share resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    #[serde(with = "super::simple_uuid")]
    pub id: Uuid,
    #[serde(with = "super::simple_uuid")]
    pub owner_tenant_id: Uuid,
    pub name: String,
    pub source_path: String,
    pub description: String,
    pub visibility: Visibility,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Parameters for creating a share
#[derive(Debug, Deserialize)]
pub struct CreateShareParams {
    #[serde(default, serialize_with = "super::simple_uuid::serialize_option", deserialize_with = "super::simple_uuid::deserialize_option")]
    pub owner_tenant_id: Option<Uuid>,
    pub name: String,
    pub source_path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Parameters for updating a share
#[derive(Debug, Deserialize)]
pub struct UpdateShareParams {
    pub name: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Filter for listing shares
#[derive(Debug, Default, Deserialize)]
pub struct ShareFilter {
    #[serde(serialize_with = "super::simple_uuid::serialize_option", deserialize_with = "super::simple_uuid::deserialize_option")]
    pub owner_tenant_id: Option<Uuid>,
    pub visibility: Option<String>,
    pub search: Option<String>,
}

/// Sandbox mount record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxMount {
    #[serde(with = "super::simple_uuid")]
    pub sandbox_id: Uuid,
    #[serde(with = "super::simple_uuid")]
    pub share_id: Uuid,
    pub mount_path: String,
}

/// Mount request when creating a sandbox
#[derive(Debug, Clone, Deserialize)]
pub struct MountRequest {
    #[serde(with = "super::simple_uuid")]
    pub share_id: Uuid,
    pub mount_path: String,
}
