//! Tenant and API Key repository (PostgreSQL)

use chrono::{DateTime, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::tenant::{
    ApiKey, CreateApiKeyParams, CreateTenantParams, PaginatedResult, Pagination, Tenant,
    TenantFilter, TenantListItem, UpdateTenantParams,
};
use crate::domain::workspace::StorageType;
use crate::error::{Error, Result};

/// Repository for tenant and API key operations
#[derive(Clone)]
pub struct TenantRepository {
    pool: PgPool,
}

// ── Row types ──

#[derive(Debug, FromRow)]
struct TenantRow {
    id: Uuid,
    name: String,
    description: String,
    is_active: bool,
    storage_type: String,
    storage_config: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct TenantListRow {
    id: Uuid,
    name: String,
    description: String,
    is_active: bool,
    storage_type: String,
    storage_config: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    share_count: Option<i64>,
    active_api_key_count: Option<i64>,
}

#[derive(Debug, FromRow)]
struct ApiKeyRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    token_prefix: String,
    token_plaintext: Option<String>,
    is_active: bool,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

// ── Conversions ──

fn tenant_from_row(row: TenantRow) -> Tenant {
    Tenant {
        id: row.id,
        name: row.name,
        description: row.description,
        is_active: row.is_active,
        storage_type: StorageType::from_str(&row.storage_type).unwrap_or(StorageType::Managed),
        storage_config: row.storage_config,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn api_key_from_row(row: ApiKeyRow) -> ApiKey {
    ApiKey {
        id: row.id,
        tenant_id: row.tenant_id,
        name: row.name,
        token_prefix: row.token_prefix,
        token_plaintext: row.token_plaintext,
        is_active: row.is_active,
        expires_at: row.expires_at,
        last_used_at: row.last_used_at,
        created_at: row.created_at,
    }
}

/// Generate a new API key token with `sk_` prefix
fn generate_api_token() -> String {
    use rand::distributions::Alphanumeric;
    let random_part: String = rand::thread_rng()
        .sample_iter(Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    format!("sk_{random_part}")
}

/// Hash a token using SHA-256
fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Get the display prefix of a token (first 12 chars + "...")
fn token_prefix(token: &str) -> String {
    let prefix: String = token.chars().take(12).collect();
    format!("{prefix}...")
}

impl TenantRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // ── Tenant CRUD ──

    /// Create a tenant, optionally with an initial API key.
    /// Returns (Tenant, Option<(ApiKey, plaintext_token)>).
    pub async fn create_tenant(
        &self,
        params: CreateTenantParams,
    ) -> Result<(Tenant, Option<(ApiKey, String)>)> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let storage_type = params.storage_type.as_deref().unwrap_or("managed");
        let storage_config = params.storage_config.unwrap_or(serde_json::json!({}));
        let description = params.description.unwrap_or_default();

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO tenants (id, name, description, is_active, storage_type, storage_config, created_at, updated_at)
            VALUES ($1, $2, $3, true, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(&params.name)
        .bind(&description)
        .bind(storage_type)
        .bind(&storage_config)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        let api_key_result = if let Some(key_params) = params.initial_api_key {
            let (key, token) = self.create_api_key_in_tx(&mut tx, id, key_params).await?;
            Some((key, token))
        } else {
            None
        };

        tx.commit().await?;

        let tenant = self.get_tenant(id).await?;
        Ok((tenant, api_key_result))
    }

    /// Get a tenant by ID
    pub async fn get_tenant(&self, id: Uuid) -> Result<Tenant> {
        let row = sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, description, is_active, storage_type, storage_config, created_at, updated_at FROM tenants WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::WorkspaceNotFound(format!("Tenant not found: {}", id)))?;

        Ok(tenant_from_row(row))
    }

    /// List tenants with filtering and pagination.
    ///
    /// Uses parameterized queries to prevent SQL injection.
    pub async fn list_tenants(
        &self,
        filter: TenantFilter,
        pagination: Pagination,
    ) -> Result<PaginatedResult<TenantListItem>> {
        let pagination = pagination.capped();
        let offset = pagination.offset() as i64;
        let limit = pagination.page_size as i64;

        // Build parameterized filter
        let search_pattern = filter.search.as_ref().map(|s| format!("%{}%", s));

        // Count query with parameterized filters
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM tenants t
            WHERE ($1::boolean IS NULL OR t.is_active = $1)
              AND ($2::text IS NULL OR t.storage_type = $2)
              AND ($3::text IS NULL OR t.name ILIKE $3 OR t.description ILIKE $3 OR t.id::text = $4)
            "#,
        )
        .bind(filter.is_active)
        .bind(filter.storage_type.as_deref())
        .bind(search_pattern.as_deref())
        .bind(filter.search.as_deref().unwrap_or(""))
        .fetch_one(&self.pool)
        .await?;

        // Data query with aggregated counts
        let rows = sqlx::query_as::<_, TenantListRow>(
            r#"
            SELECT t.id, t.name, t.description, t.is_active, t.storage_type, t.storage_config,
                   t.created_at, t.updated_at,
                   (SELECT COUNT(*) FROM shares s WHERE s.owner_tenant_id = t.id) as share_count,
                   (SELECT COUNT(*) FROM api_keys k WHERE k.tenant_id = t.id AND k.is_active = true AND (k.expires_at IS NULL OR k.expires_at > now())) as active_api_key_count
            FROM tenants t
            WHERE ($1::boolean IS NULL OR t.is_active = $1)
              AND ($2::text IS NULL OR t.storage_type = $2)
              AND ($3::text IS NULL OR t.name ILIKE $3 OR t.description ILIKE $3 OR t.id::text = $4)
            ORDER BY t.created_at DESC
            LIMIT $5 OFFSET $6
            "#,
        )
        .bind(filter.is_active)
        .bind(filter.storage_type.as_deref())
        .bind(search_pattern.as_deref())
        .bind(filter.search.as_deref().unwrap_or(""))
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let items = rows
            .into_iter()
            .map(|row| TenantListItem {
                tenant: tenant_from_row(TenantRow {
                    id: row.id,
                    name: row.name,
                    description: row.description,
                    is_active: row.is_active,
                    storage_type: row.storage_type,
                    storage_config: row.storage_config,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                }),
                share_count: row.share_count.unwrap_or(0),
                active_api_key_count: row.active_api_key_count.unwrap_or(0),
            })
            .collect();

        Ok(PaginatedResult {
            items,
            total: count,
            page: pagination.page,
            page_size: pagination.page_size,
        })
    }

    /// Update a tenant
    pub async fn update_tenant(&self, id: Uuid, params: UpdateTenantParams) -> Result<Tenant> {
        // Build dynamic SET clause
        let mut sets = Vec::new();
        let mut idx = 2u32; // $1 is id

        if params.name.is_some() {
            sets.push(format!("name = ${idx}"));
            idx += 1;
        }
        if params.description.is_some() {
            sets.push(format!("description = ${idx}"));
            idx += 1;
        }
        if params.storage_type.is_some() {
            sets.push(format!("storage_type = ${idx}"));
            idx += 1;
        }
        sets.push(format!("updated_at = ${idx}"));

        if sets.len() == 1 {
            // Only updated_at, nothing to change
            return self.get_tenant(id).await;
        }

        let sql = format!("UPDATE tenants SET {} WHERE id = $1", sets.join(", "));
        let now = Utc::now();

        let mut query = sqlx::query(&sql).bind(id);
        if let Some(ref name) = params.name {
            query = query.bind(name);
        }
        if let Some(ref desc) = params.description {
            query = query.bind(desc);
        }
        if let Some(ref st) = params.storage_type {
            query = query.bind(st);
        }
        query = query.bind(now);

        query.execute(&self.pool).await?;
        self.get_tenant(id).await
    }

    /// Activate a tenant
    pub async fn activate_tenant(&self, id: Uuid) -> Result<Tenant> {
        let result =
            sqlx::query("UPDATE tenants SET is_active = true, updated_at = $2 WHERE id = $1")
                .bind(id)
                .bind(Utc::now())
                .execute(&self.pool)
                .await?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(format!(
                "Tenant not found: {}",
                id
            )));
        }

        self.get_tenant(id).await
    }

    /// Deactivate a tenant
    pub async fn deactivate_tenant(&self, id: Uuid) -> Result<Tenant> {
        let result =
            sqlx::query("UPDATE tenants SET is_active = false, updated_at = $2 WHERE id = $1")
                .bind(id)
                .bind(Utc::now())
                .execute(&self.pool)
                .await?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(format!(
                "Tenant not found: {}",
                id
            )));
        }

        self.get_tenant(id).await
    }

    /// Delete a tenant with full precondition checks.
    ///
    /// Always blocks on:
    /// - Active shares (owned by this tenant)
    /// - Active sandboxes (running, starting, or stopping)
    ///
    /// If `force=false`, also blocks on active API keys.
    /// If `force=true`, cascades deletion of stopped sandboxes, mounts, permissions, and keys.
    ///
    /// All precondition checks run inside the transaction to prevent TOCTOU races.
    pub async fn delete_tenant(&self, id: Uuid, force: bool) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Check for active shares — always blocks
        let share_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM shares WHERE owner_tenant_id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;

        if share_count > 0 {
            return Err(Error::HasActiveShares);
        }

        // Check for active sandboxes (running, starting, stopping) — always blocks
        let active_sandbox_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sandboxes WHERE namespace_id = $1 AND state IN ('starting', 'running', 'stopping')",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;

        if active_sandbox_count > 0 {
            return Err(Error::WorkspaceHasActiveSandboxes);
        }

        if !force {
            // Check for active (non-expired) API keys — blocks unless forced
            let key_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM api_keys WHERE tenant_id = $1 AND is_active = true AND (expires_at IS NULL OR expires_at > now())",
            )
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;

            if key_count > 0 {
                return Err(Error::HasActiveApiKeys(key_count));
            }
        }

        // Delete sandbox mounts for stopped/error sandboxes
        sqlx::query(
            r#"DELETE FROM sandbox_mounts
               WHERE sandbox_id IN (
                   SELECT id FROM sandboxes
                   WHERE namespace_id = $1 AND state IN ('stopped', 'error')
               )"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        // Delete stopped/error sandboxes
        sqlx::query(
            "DELETE FROM sandboxes WHERE namespace_id = $1 AND state IN ('stopped', 'error')",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        // Delete API keys
        sqlx::query("DELETE FROM api_keys WHERE tenant_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        // Delete share permissions for shares owned by this tenant
        sqlx::query(
            r#"DELETE FROM share_permissions
               WHERE share_id IN (SELECT id FROM shares WHERE owner_tenant_id = $1)"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        // Delete share permissions granted TO this tenant on other shares
        sqlx::query("DELETE FROM share_permissions WHERE tenant_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        // Delete shares owned by this tenant
        sqlx::query("DELETE FROM shares WHERE owner_tenant_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        // Delete the tenant
        sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }

    // ── API Key operations ──

    /// Create an API key for a tenant. Returns (ApiKey, plaintext_token).
    pub async fn create_api_key(
        &self,
        tenant_id: Uuid,
        params: CreateApiKeyParams,
    ) -> Result<(ApiKey, String)> {
        // Verify tenant exists
        let _ = self.get_tenant(tenant_id).await?;

        let mut tx = self.pool.begin().await?;
        let result = self
            .create_api_key_in_tx(&mut tx, tenant_id, params)
            .await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Internal: create API key within a transaction
    async fn create_api_key_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: Uuid,
        params: CreateApiKeyParams,
    ) -> Result<(ApiKey, String)> {
        let id = Uuid::now_v7();
        let token = generate_api_token();
        let hash = hash_token(&token);
        let prefix = token_prefix(&token);
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO api_keys (id, tenant_id, name, token_hash, token_prefix, token_plaintext, is_active, expires_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, true, $7, $8)
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&params.name)
        .bind(&hash)
        .bind(&prefix)
        .bind(&token)
        .bind(params.expires_at)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        let key = ApiKey {
            id,
            tenant_id,
            name: params.name,
            token_prefix: prefix,
            token_plaintext: Some(token.clone()),
            is_active: true,
            expires_at: params.expires_at,
            last_used_at: None,
            created_at: now,
        };

        Ok((key, token))
    }

    /// Get a single API key by its ID (includes token_plaintext)
    pub async fn get_api_key(&self, key_id: Uuid) -> Result<ApiKey> {
        let row = sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, tenant_id, name, token_prefix, token_plaintext, is_active, expires_at, last_used_at, created_at FROM api_keys WHERE id = $1",
        )
        .bind(key_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::WorkspaceNotFound(format!("API key not found: {}", key_id)))?;

        Ok(api_key_from_row(row))
    }

    /// Get plaintext token for an API key
    pub async fn get_api_key_plaintext(&self, key_id: Uuid) -> Result<String> {
        let plaintext: Option<String> =
            sqlx::query_scalar("SELECT token_plaintext FROM api_keys WHERE id = $1")
                .bind(key_id)
                .fetch_optional(&self.pool)
                .await?
                .flatten();

        plaintext.ok_or_else(|| {
            Error::WorkspaceNotFound(format!("API key not found: {}", key_id))
        })
    }

    /// List API keys for a tenant (token_plaintext is excluded)
    pub async fn list_api_keys(&self, tenant_id: Uuid) -> Result<Vec<ApiKey>> {
        let rows = sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, tenant_id, name, token_prefix, NULL as token_plaintext, is_active, expires_at, last_used_at, created_at FROM api_keys WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(api_key_from_row).collect())
    }

    /// Revoke (deactivate) an API key
    pub async fn revoke_api_key(&self, key_id: Uuid) -> Result<()> {
        let result = sqlx::query("UPDATE api_keys SET is_active = false WHERE id = $1")
            .bind(key_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(format!(
                "API key not found: {}",
                key_id
            )));
        }

        Ok(())
    }

    /// Find tenant + API key by token hash (authentication path)
    pub async fn find_by_token_hash(&self, token: &str) -> Result<Option<(ApiKey, Tenant)>> {
        let hash = hash_token(token);

        let row = sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, tenant_id, name, token_prefix, NULL as token_plaintext, is_active, expires_at, last_used_at, created_at FROM api_keys WHERE token_hash = $1",
        )
        .bind(&hash)
        .fetch_optional(&self.pool)
        .await?;

        let key = match row {
            Some(r) => api_key_from_row(r),
            None => return Ok(None),
        };

        let tenant = self.get_tenant(key.tenant_id).await?;
        Ok(Some((key, tenant)))
    }

    /// Update last_used_at for an API key
    pub async fn update_last_used(&self, key_id: Uuid) -> Result<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = $2 WHERE id = $1")
            .bind(key_id)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
