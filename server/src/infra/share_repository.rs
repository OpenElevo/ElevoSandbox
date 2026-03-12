//! Share repository — PostgreSQL implementation

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::share::{
    CreateShareParams, Share, ShareFilter, UpdateShareParams,
};
use crate::domain::tenant::{Pagination, PaginatedResult};
use crate::error::Error;

#[derive(Clone)]
pub struct ShareRepository {
    pool: PgPool,
}

#[derive(Debug, sqlx::FromRow)]
struct ShareRow {
    id: Uuid,
    owner_tenant_id: Uuid,
    name: String,
    source_path: String,
    description: String,
    visibility: String,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ShareRow> for Share {
    fn from(row: ShareRow) -> Self {
        Share {
            id: row.id,
            owner_tenant_id: row.owner_tenant_id,
            name: row.name,
            source_path: row.source_path,
            description: row.description,
            visibility: crate::domain::share::Visibility::from_str_value(
                &row.visibility,
            )
            .unwrap_or(crate::domain::share::Visibility::Private),
            metadata: row.metadata,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

impl ShareRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_share(
        &self,
        params: &CreateShareParams,
    ) -> Result<Share, Error> {
        let owner_id = params.owner_tenant_id
            .ok_or_else(|| Error::InvalidParameter("owner_tenant_id is required".into()))?;
        let visibility = params.visibility.as_deref().unwrap_or("private");
        let description = params.description.as_deref().unwrap_or("");
        let metadata = params
            .metadata
            .as_ref()
            .cloned()
            .unwrap_or(serde_json::json!({}));

        let row = sqlx::query_as::<_, ShareRow>(
            r#"
            INSERT INTO shares
                (owner_tenant_id, name, source_path, description, visibility, metadata)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, owner_tenant_id, name, source_path, description,
                      visibility, metadata, created_at, updated_at
            "#,
        )
        .bind(owner_id)
        .bind(&params.name)
        .bind(&params.source_path)
        .bind(description)
        .bind(visibility)
        .bind(&metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(ref db_err)
                if db_err.constraint().is_some() =>
            {
                Error::InvalidParameter(
                    "Share with this name or path already exists for this tenant"
                        .into(),
                )
            }
            _ => Error::Internal(format!("Failed to create share: {}", e)),
        })?;

        Ok(row.into())
    }

    pub async fn get_share(&self, id: Uuid) -> Result<Share, Error> {
        let row = sqlx::query_as::<_, ShareRow>(
            "SELECT * FROM shares WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?
        .ok_or_else(|| {
            Error::WorkspaceNotFound(format!("Share {} not found", id))
        })?;

        Ok(row.into())
    }

    pub async fn list_shares(
        &self,
        filter: ShareFilter,
        pagination: Pagination,
    ) -> Result<PaginatedResult<Share>, Error> {
        let pagination = pagination.capped();
        let page = pagination.page;
        let per_page = pagination.page_size;
        let offset = ((page - 1) * per_page) as i64;
        let limit = per_page as i64;
        let search_pattern = filter.search.as_ref().map(|s| format!("%{}%", s));

        // Use parameterized queries that support all filter combinations
        let total: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM shares
               WHERE ($1::uuid IS NULL OR owner_tenant_id = $1)
                 AND ($2::text IS NULL OR visibility = $2)
                 AND ($3::text IS NULL OR name ILIKE $3 OR description ILIKE $3)"#,
        )
        .bind(filter.owner_tenant_id)
        .bind(filter.visibility.as_deref())
        .bind(search_pattern.as_deref())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        let rows = sqlx::query_as::<_, ShareRow>(
            r#"SELECT * FROM shares
               WHERE ($1::uuid IS NULL OR owner_tenant_id = $1)
                 AND ($2::text IS NULL OR visibility = $2)
                 AND ($3::text IS NULL OR name ILIKE $3 OR description ILIKE $3)
               ORDER BY created_at DESC
               LIMIT $4 OFFSET $5"#,
        )
        .bind(filter.owner_tenant_id)
        .bind(filter.visibility.as_deref())
        .bind(search_pattern.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        let items = rows.into_iter().map(Share::from).collect();
        Ok(PaginatedResult { items, total, page, page_size: per_page })
    }

    pub async fn update_share(
        &self,
        id: Uuid,
        params: UpdateShareParams,
    ) -> Result<Share, Error> {
        // Get current share
        let current = self.get_share(id).await?;

        let name = params.name.unwrap_or(current.name);
        let description = params.description.unwrap_or(current.description);
        let visibility = params
            .visibility
            .unwrap_or_else(|| current.visibility.as_str().to_string());
        let metadata = params.metadata.unwrap_or(current.metadata);

        let row = sqlx::query_as::<_, ShareRow>(
            r#"UPDATE shares
               SET name = $1, description = $2, visibility = $3,
                   metadata = $4, updated_at = now()
               WHERE id = $5
               RETURNING id, owner_tenant_id, name, source_path, description,
                         visibility, metadata, created_at, updated_at"#,
        )
        .bind(&name)
        .bind(&description)
        .bind(&visibility)
        .bind(&metadata)
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update share: {}", e)))?;

        Ok(row.into())
    }

    pub async fn delete_share(&self, id: Uuid) -> Result<(), Error> {
        // Only block on active sandbox mounts (running, starting, stopping)
        let active_mount_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM sandbox_mounts sm
               JOIN sandboxes s ON s.id = sm.sandbox_id
               WHERE sm.share_id = $1 AND s.state IN ('running', 'starting', 'stopping')"#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        if active_mount_count > 0 {
            return Err(Error::WorkspaceHasActiveSandboxes);
        }

        // Delete in a transaction: clean up mounts, permissions, then the share
        let mut tx = self.pool.begin().await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        // Delete all sandbox mounts for this share (including stopped/error ones)
        sqlx::query("DELETE FROM sandbox_mounts WHERE share_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        // Delete permissions
        sqlx::query("DELETE FROM share_permissions WHERE share_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        // Delete the share itself
        let result = sqlx::query("DELETE FROM shares WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(format!(
                "Share {} not found",
                id
            )));
        }

        tx.commit().await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        Ok(())
    }

    /// List shares accessible by a specific tenant.
    /// Excludes shares owned by deactivated tenants.
    pub async fn list_accessible_shares(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<Share>, Error> {
        let rows = sqlx::query_as::<_, ShareRow>(
            r#"SELECT DISTINCT s.* FROM shares s
               JOIN tenants t ON t.id = s.owner_tenant_id
               LEFT JOIN share_permissions sp ON s.id = sp.share_id
               WHERE t.is_active = true
                 AND (s.owner_tenant_id = $1
                      OR sp.tenant_id = $1
                      OR s.visibility = 'public')
               ORDER BY s.created_at DESC"#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        Ok(rows.into_iter().map(Share::from).collect())
    }

    /// List shares accessible to a tenant with pagination support.
    pub async fn list_accessible_shares_paginated(
        &self,
        tenant_id: Uuid,
        pagination: Pagination,
    ) -> Result<PaginatedResult<Share>, Error> {
        let pagination = pagination.capped();
        let page = pagination.page;
        let per_page = pagination.page_size;
        let offset = ((page - 1) * per_page) as i64;
        let limit = per_page as i64;

        let base_where = r#"
            FROM shares s
            JOIN tenants t ON t.id = s.owner_tenant_id
            LEFT JOIN share_permissions sp ON s.id = sp.share_id
            WHERE t.is_active = true
              AND (s.owner_tenant_id = $1
                   OR sp.tenant_id = $1
                   OR s.visibility = 'public')
        "#;

        let count_sql = format!("SELECT COUNT(DISTINCT s.id) {}", base_where);
        let total: i64 = sqlx::query_scalar(&count_sql)
            .bind(tenant_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        let data_sql = format!(
            "SELECT DISTINCT s.* {} ORDER BY s.created_at DESC LIMIT $2 OFFSET $3",
            base_where,
        );
        let rows = sqlx::query_as::<_, ShareRow>(&data_sql)
            .bind(tenant_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        let items = rows.into_iter().map(Share::from).collect();
        Ok(PaginatedResult { items, total, page, page_size: per_page })
    }
}
