//! Tenant and API Key repository (PostgreSQL)

use chrono::{DateTime, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::tenant::{
    ApiKey, CreateApiKeyParams, CreateTenantParams, Pagination, PaginatedResult,
    Tenant, TenantFilter, TenantListItem, UpdateTenantParams,
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
    is_active: bool,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct CountRow {
    count: i64,
}

// ── Conversions ──

fn tenant_from_row(row: TenantRow) -> Tenant {
    Tenant {
        id: row.id.to_string(),
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
        id: row.id.to_string(),
        tenant_id: row.tenant_id.to_string(),
        name: row.name,
        token_prefix: row.token_prefix,
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
        let id = Uuid::new_v4();
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

        let tenant = self.get_tenant(&id.to_string()).await?;
        Ok((tenant, api_key_result))
    }

    /// Get a tenant by ID
    pub async fn get_tenant(&self, id: &str) -> Result<Tenant> {
        let uuid = Uuid::parse_str(id)
            .map_err(|e| Error::InvalidParameter(format!("Invalid tenant ID: {}", e)))?;

        let row = sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, description, is_active, storage_type, storage_config, created_at, updated_at FROM tenants WHERE id = $1",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::WorkspaceNotFound(format!("Tenant not found: {}", id)))?;

        Ok(tenant_from_row(row))
    }

    /// List tenants with filtering and pagination
    pub async fn list_tenants(
        &self,
        filter: TenantFilter,
        pagination: Pagination,
    ) -> Result<PaginatedResult<TenantListItem>> {
        let (where_clause, count_clause) = self.build_tenant_filter(&filter);
        let offset = pagination.offset() as i64;
        let limit = pagination.page_size as i64;

        // Count query
        let count_sql = format!("SELECT COUNT(*) as count FROM tenants t {}", count_clause);
        let count_row = sqlx::query_as::<_, CountRow>(&count_sql)
            .fetch_one(&self.pool)
            .await?;

        // List query with aggregated counts
        let list_sql = format!(
            r#"
            SELECT t.id, t.name, t.description, t.is_active, t.storage_type, t.storage_config,
                   t.created_at, t.updated_at,
                   (SELECT COUNT(*) FROM shares s WHERE s.owner_tenant_id = t.id) as share_count,
                   (SELECT COUNT(*) FROM api_keys k WHERE k.tenant_id = t.id AND k.is_active = true) as active_api_key_count
            FROM tenants t
            {}
            ORDER BY t.created_at DESC
            LIMIT {} OFFSET {}
            "#,
            where_clause, limit, offset
        );

        let rows = sqlx::query_as::<_, TenantListRow>(&list_sql)
            .fetch_all(&self.pool)
            .await;

        // If shares table doesn't exist yet (Phase 2b), fall back
        let rows = match rows {
            Ok(r) => r,
            Err(_) => {
                let fallback_sql = format!(
                    r#"
                    SELECT t.id, t.name, t.description, t.is_active, t.storage_type, t.storage_config,
                           t.created_at, t.updated_at,
                           0::bigint as share_count,
                           (SELECT COUNT(*) FROM api_keys k WHERE k.tenant_id = t.id AND k.is_active = true) as active_api_key_count
                    FROM tenants t
                    {}
                    ORDER BY t.created_at DESC
                    LIMIT {} OFFSET {}
                    "#,
                    where_clause, limit, offset
                );
                sqlx::query_as::<_, TenantListRow>(&fallback_sql)
                    .fetch_all(&self.pool)
                    .await?
            }
        };

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
            total: count_row.count,
            page: pagination.page,
            page_size: pagination.page_size,
        })
    }

    /// Update a tenant
    pub async fn update_tenant(&self, id: &str, params: UpdateTenantParams) -> Result<Tenant> {
        let uuid = Uuid::parse_str(id)
            .map_err(|e| Error::InvalidParameter(format!("Invalid tenant ID: {}", e)))?;

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
        sets.push(format!("updated_at = ${idx}"));

        if sets.len() == 1 {
            // Only updated_at, nothing to change
            return self.get_tenant(id).await;
        }

        let sql = format!("UPDATE tenants SET {} WHERE id = $1", sets.join(", "));
        let now = Utc::now();

        let mut query = sqlx::query(&sql).bind(uuid);
        if let Some(ref name) = params.name {
            query = query.bind(name);
        }
        if let Some(ref desc) = params.description {
            query = query.bind(desc);
        }
        query = query.bind(now);

        query.execute(&self.pool).await?;
        self.get_tenant(id).await
    }

    /// Activate a tenant
    pub async fn activate_tenant(&self, id: &str) -> Result<Tenant> {
        let uuid = Uuid::parse_str(id)
            .map_err(|e| Error::InvalidParameter(format!("Invalid tenant ID: {}", e)))?;

        sqlx::query("UPDATE tenants SET is_active = true, updated_at = $2 WHERE id = $1")
            .bind(uuid)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;

        self.get_tenant(id).await
    }

    /// Deactivate a tenant
    pub async fn deactivate_tenant(&self, id: &str) -> Result<Tenant> {
        let uuid = Uuid::parse_str(id)
            .map_err(|e| Error::InvalidParameter(format!("Invalid tenant ID: {}", e)))?;

        sqlx::query("UPDATE tenants SET is_active = false, updated_at = $2 WHERE id = $1")
            .bind(uuid)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;

        self.get_tenant(id).await
    }

    /// Delete a tenant. If force=false, checks for active sandboxes.
    pub async fn delete_tenant(&self, id: &str, force: bool) -> Result<()> {
        let uuid = Uuid::parse_str(id)
            .map_err(|e| Error::InvalidParameter(format!("Invalid tenant ID: {}", e)))?;

        if !force {
            let count = sqlx::query_as::<_, CountRow>(
                "SELECT COUNT(*) as count FROM sandboxes WHERE namespace_id = $1 AND state IN ('starting', 'running')",
            )
            .bind(uuid)
            .fetch_one(&self.pool)
            .await?;

            if count.count > 0 {
                return Err(Error::WorkspaceHasActiveSandboxes);
            }
        }

        sqlx::query("DELETE FROM tenants WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ── API Key operations ──

    /// Create an API key for a tenant. Returns (ApiKey, plaintext_token).
    pub async fn create_api_key(
        &self,
        tenant_id: &str,
        params: CreateApiKeyParams,
    ) -> Result<(ApiKey, String)> {
        let uuid = Uuid::parse_str(tenant_id)
            .map_err(|e| Error::InvalidParameter(format!("Invalid tenant ID: {}", e)))?;

        // Verify tenant exists
        let _ = self.get_tenant(tenant_id).await?;

        let mut tx = self.pool.begin().await?;
        let result = self.create_api_key_in_tx(&mut tx, uuid, params).await?;
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
        let id = Uuid::new_v4();
        let token = generate_api_token();
        let hash = hash_token(&token);
        let prefix = token_prefix(&token);
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO api_keys (id, tenant_id, name, token_hash, token_prefix, is_active, expires_at, created_at)
            VALUES ($1, $2, $3, $4, $5, true, $6, $7)
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&params.name)
        .bind(&hash)
        .bind(&prefix)
        .bind(params.expires_at)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        let key = ApiKey {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            name: params.name,
            token_prefix: prefix,
            is_active: true,
            expires_at: params.expires_at,
            last_used_at: None,
            created_at: now,
        };

        Ok((key, token))
    }

    /// List API keys for a tenant
    pub async fn list_api_keys(&self, tenant_id: &str) -> Result<Vec<ApiKey>> {
        let uuid = Uuid::parse_str(tenant_id)
            .map_err(|e| Error::InvalidParameter(format!("Invalid tenant ID: {}", e)))?;

        let rows = sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, tenant_id, name, token_prefix, is_active, expires_at, last_used_at, created_at FROM api_keys WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(uuid)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(api_key_from_row).collect())
    }

    /// Revoke (deactivate) an API key
    pub async fn revoke_api_key(&self, key_id: &str) -> Result<()> {
        let uuid = Uuid::parse_str(key_id)
            .map_err(|e| Error::InvalidParameter(format!("Invalid key ID: {}", e)))?;

        let result = sqlx::query("UPDATE api_keys SET is_active = false WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(format!("API key not found: {}", key_id)));
        }

        Ok(())
    }

    /// Find tenant + API key by token hash (authentication path)
    pub async fn find_by_token_hash(&self, token: &str) -> Result<Option<(ApiKey, Tenant)>> {
        let hash = hash_token(token);

        let row = sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, tenant_id, name, token_prefix, is_active, expires_at, last_used_at, created_at FROM api_keys WHERE token_hash = $1",
        )
        .bind(&hash)
        .fetch_optional(&self.pool)
        .await?;

        let key = match row {
            Some(r) => api_key_from_row(r),
            None => return Ok(None),
        };

        let tenant = self.get_tenant(&key.tenant_id).await?;
        Ok(Some((key, tenant)))
    }

    /// Update last_used_at for an API key
    pub async fn update_last_used(&self, key_id: &str) -> Result<()> {
        let uuid = Uuid::parse_str(key_id)
            .map_err(|e| Error::InvalidParameter(format!("Invalid key ID: {}", e)))?;

        sqlx::query("UPDATE api_keys SET last_used_at = $2 WHERE id = $1")
            .bind(uuid)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // ── Helpers ──

    fn build_tenant_filter(&self, filter: &TenantFilter) -> (String, String) {
        let mut conditions = Vec::new();

        if let Some(active) = filter.is_active {
            conditions.push(format!("t.is_active = {}", active));
        }
        if let Some(ref st) = filter.storage_type {
            conditions.push(format!("t.storage_type = '{}'", st.replace('\'', "''")));
        }
        if let Some(ref search) = filter.search {
            let escaped = search.replace('\'', "''");
            conditions.push(format!(
                "(t.name ILIKE '%{}%' OR t.description ILIKE '%{}%' OR t.id::text = '{}')",
                escaped, escaped, escaped
            ));
        }

        if conditions.is_empty() {
            (String::new(), String::new())
        } else {
            let clause = format!("WHERE {}", conditions.join(" AND "));
            (clause.clone(), clause)
        }
    }
}
