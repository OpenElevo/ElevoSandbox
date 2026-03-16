//! Audit log domain model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    #[serde(with = "super::simple_uuid")]
    pub id: Uuid,
    pub actor_type: String,
    #[serde(serialize_with = "super::simple_uuid::serialize_option", deserialize_with = "super::simple_uuid::deserialize_option")]
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    #[serde(with = "super::simple_uuid")]
    pub resource_id: Uuid,
    pub resource_name: String,
    pub detail: serde_json::Value,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Parameters for creating an audit log entry
#[derive(Debug, Clone)]
pub struct CreateAuditLogParams {
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Uuid,
    pub resource_name: String,
    pub detail: serde_json::Value,
    pub ip_address: Option<String>,
}

/// Filter for querying audit logs
#[derive(Debug, Default, Deserialize)]
pub struct AuditLogFilter {
    pub action: Option<String>,
    pub actor_type: Option<String>,
    pub actor_id: Option<String>,
    pub resource_type: Option<String>,
    #[serde(serialize_with = "super::simple_uuid::serialize_option", deserialize_with = "super::simple_uuid::deserialize_option")]
    pub resource_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}
