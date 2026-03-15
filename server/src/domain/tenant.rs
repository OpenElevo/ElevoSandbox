//! Tenant and API Key domain models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::workspace::StorageType;

/// Tenant entity (also serves as Namespace, 1:1 relationship)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub storage_type: StorageType,
    pub storage_config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// API Key entity (token_hash is never exposed outside infra layer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub token_prefix: String,
    pub is_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ApiKey {
    /// Check if this key has expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| exp < Utc::now()).unwrap_or(false)
    }

    /// Check if this key is usable (active and not expired)
    pub fn is_usable(&self) -> bool {
        self.is_active && !self.is_expired()
    }
}

/// Parameters for creating a tenant
#[derive(Debug, Clone, Deserialize)]
pub struct CreateTenantParams {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub storage_type: Option<String>,
    #[serde(default)]
    pub storage_config: Option<serde_json::Value>,
    #[serde(default)]
    pub initial_api_key: Option<CreateApiKeyParams>,
}

/// Parameters for updating a tenant
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateTenantParams {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Parameters for creating an API key
#[derive(Debug, Clone, Deserialize)]
pub struct CreateApiKeyParams {
    pub name: String,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Tenant list item with aggregated counts
#[derive(Debug, Clone, Serialize)]
pub struct TenantListItem {
    #[serde(flatten)]
    pub tenant: Tenant,
    pub share_count: i64,
    pub active_api_key_count: i64,
}

/// Filter for listing tenants
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TenantFilter {
    pub search: Option<String>,
    pub is_active: Option<bool>,
    pub storage_type: Option<String>,
}

/// Pagination parameters
#[derive(Debug, Clone, Deserialize)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    20
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: default_page(),
            page_size: default_page_size(),
        }
    }
}

/// Maximum allowed page_size to prevent oversized queries
pub const MAX_PAGE_SIZE: u32 = 100;

impl Pagination {
    pub fn offset(&self) -> u32 {
        (self.page.saturating_sub(1)) * self.effective_page_size()
    }

    /// Return the effective page_size, capped at MAX_PAGE_SIZE
    pub fn effective_page_size(&self) -> u32 {
        self.page_size.min(MAX_PAGE_SIZE)
    }

    /// Return a new Pagination with page_size capped at MAX_PAGE_SIZE
    pub fn capped(self) -> Self {
        Self {
            page: self.page.max(1),
            page_size: self.effective_page_size(),
        }
    }
}

/// Paginated result
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}
