//! Share repository — PostgreSQL implementation

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::share::{
    CreateShareParams, Share, ShareFilter, UpdateShareParams,
};
use crate::domain::tenant::Pagination;
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
            id: row.id.to_string(),
            owner_tenant_id: row.owner_tenant_id.to_string(),
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
        let owner_id = Uuid::parse_str(&params.owner_tenant_id)
            .map_err(|_| Error::InvalidParameter("Invalid owner tenant ID".into()))?;
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

    pub async fn get_share(&self, id: &str) -> Result<Share, Error> {
        let uuid = Uuid::parse_str(id)
            .map_err(|_| Error::InvalidParameter("Invalid share ID".into()))?;

        let row = sqlx::query_as::<_, ShareRow>(
            "SELECT * FROM shares WHERE id = $1",
        )
        .bind(uuid)
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
    ) -> Result<(Vec<Share>, i64), Error> {
        let page = pagination.page.max(1);
        let per_page = pagination.page_size.min(100);
        let offset = ((page - 1) * per_page) as i64;
        let limit = per_page as i64;

        // Count query
        let count: (i64,) = if let Some(ref owner) = filter.owner_tenant_id {
            let owner_uuid = Uuid::parse_str(owner).map_err(|_| {
                Error::InvalidParameter("Invalid owner ID".into())
            })?;
            sqlx::query_as(
                "SELECT COUNT(*) FROM shares WHERE owner_tenant_id = $1",
            )
            .bind(owner_uuid)
            .fetch_one(&self.pool)
            .await
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM shares")
                .fetch_one(&self.pool)
                .await
        }
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        // Data query
        let rows: Vec<ShareRow> = if let Some(ref owner) = filter.owner_tenant_id
        {
            let owner_uuid = Uuid::parse_str(owner).unwrap();
            if let Some(ref search) = filter.search {
                let pattern = format!("%{}%", search);
                sqlx::query_as::<_, ShareRow>(
                    r#"SELECT * FROM shares
                       WHERE owner_tenant_id = $1
                         AND (name ILIKE $2 OR description ILIKE $3)
                       ORDER BY created_at DESC LIMIT $4 OFFSET $5"#,
                )
                .bind(owner_uuid)
                .bind(&pattern)
                .bind(&pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
            } else {
                sqlx::query_as::<_, ShareRow>(
                    r#"SELECT * FROM shares
                       WHERE owner_tenant_id = $1
                       ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
                )
                .bind(owner_uuid)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await
            }
        } else if let Some(ref search) = filter.search {
            let pattern = format!("%{}%", search);
            sqlx::query_as::<_, ShareRow>(
                r#"SELECT * FROM shares
                   WHERE name ILIKE $1 OR description ILIKE $2
                   ORDER BY created_at DESC LIMIT $3 OFFSET $4"#,
            )
            .bind(&pattern)
            .bind(&pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, ShareRow>(
                "SELECT * FROM shares ORDER BY created_at DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        let shares = rows.into_iter().map(Share::from).collect();
        Ok((shares, count.0))
    }

    pub async fn update_share(
        &self,
        id: &str,
        params: UpdateShareParams,
    ) -> Result<Share, Error> {
        let uuid = Uuid::parse_str(id)
            .map_err(|_| Error::InvalidParameter("Invalid share ID".into()))?;

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
        .bind(uuid)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update share: {}", e)))?;

        Ok(row.into())
    }

    pub async fn delete_share(&self, id: &str) -> Result<(), Error> {
        let uuid = Uuid::parse_str(id)
            .map_err(|_| Error::InvalidParameter("Invalid share ID".into()))?;

        // Check for active sandbox mounts
        let mount_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sandbox_mounts WHERE share_id = $1",
        )
        .bind(uuid)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        if mount_count.0 > 0 {
            return Err(Error::WorkspaceHasActiveSandboxes);
        }

        // Delete permissions first, then the share
        sqlx::query("DELETE FROM share_permissions WHERE share_id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        let result = sqlx::query("DELETE FROM shares WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(format!(
                "Share {} not found",
                id
            )));
        }

        Ok(())
    }

    /// List shares accessible by a specific tenant
    pub async fn list_accessible_shares(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<Share>, Error> {
        let uuid = Uuid::parse_str(tenant_id)
            .map_err(|_| Error::InvalidParameter("Invalid tenant ID".into()))?;

        let rows = sqlx::query_as::<_, ShareRow>(
            r#"SELECT DISTINCT s.* FROM shares s
               LEFT JOIN share_permissions sp ON s.id = sp.share_id
               WHERE s.owner_tenant_id = $1
                  OR sp.tenant_id = $1
                  OR s.visibility = 'public'
               ORDER BY s.created_at DESC"#,
        )
        .bind(uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        Ok(rows.into_iter().map(Share::from).collect())
    }
}
