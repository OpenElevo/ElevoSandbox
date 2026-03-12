//! Share permission repository — PostgreSQL implementation

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::permission::{PermissionLevel, SharePermission};
use crate::error::Error;

#[derive(Clone)]
pub struct SharePermissionRepository {
    pool: PgPool,
}

#[derive(Debug, sqlx::FromRow)]
struct PermissionRow {
    tenant_id: Uuid,
    share_id: Uuid,
    permission: String,
    created_at: DateTime<Utc>,
}

impl From<PermissionRow> for SharePermission {
    fn from(row: PermissionRow) -> Self {
        SharePermission {
            tenant_id: row.tenant_id,
            share_id: row.share_id,
            permission: PermissionLevel::from_str_value(&row.permission)
                .unwrap_or(PermissionLevel::Read),
            created_at: row.created_at,
        }
    }
}

impl SharePermissionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Grant or update permission for a tenant on a share
    pub async fn grant_permission(
        &self,
        share_id: Uuid,
        tenant_id: Uuid,
        level: PermissionLevel,
    ) -> Result<SharePermission, Error> {
        let row = sqlx::query_as::<_, PermissionRow>(
            r#"
            INSERT INTO share_permissions (tenant_id, share_id, permission)
            VALUES ($1, $2, $3)
            ON CONFLICT (tenant_id, share_id)
            DO UPDATE SET permission = $3
            RETURNING tenant_id, share_id, permission, created_at
            "#,
        )
        .bind(tenant_id)
        .bind(share_id)
        .bind(level.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Failed to grant permission: {}", e)))?;

        Ok(row.into())
    }

    /// Revoke permission for a tenant on a share
    pub async fn revoke_permission(
        &self,
        share_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<(), Error> {
        let result = sqlx::query(
            "DELETE FROM share_permissions WHERE share_id = $1 AND tenant_id = $2",
        )
        .bind(share_id)
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(
                "Permission not found".into(),
            ));
        }

        Ok(())
    }

    /// Get permission level for a tenant on a share
    pub async fn get_permission(
        &self,
        share_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<Option<PermissionLevel>, Error> {
        let row = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM share_permissions WHERE share_id = $1 AND tenant_id = $2",
        )
        .bind(share_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        Ok(row.map(|r| {
            let perm: SharePermission = r.into();
            perm.permission
        }))
    }

    /// List all permissions for a share
    pub async fn list_by_share(
        &self,
        share_id: Uuid,
    ) -> Result<Vec<SharePermission>, Error> {
        let rows = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM share_permissions WHERE share_id = $1 ORDER BY created_at",
        )
        .bind(share_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        Ok(rows.into_iter().map(SharePermission::from).collect())
    }

    /// List all permissions for a tenant
    pub async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<SharePermission>, Error> {
        let rows = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM share_permissions WHERE tenant_id = $1 ORDER BY created_at",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        Ok(rows.into_iter().map(SharePermission::from).collect())
    }

    /// Update an existing permission (plain UPDATE — returns NOT_FOUND if no rows affected).
    ///
    /// Unlike `grant_permission`, this does not upsert: it only updates an already-granted
    /// permission and returns an error if the permission record does not exist.
    pub async fn update_permission(
        &self,
        share_id: Uuid,
        tenant_id: Uuid,
        level: PermissionLevel,
    ) -> Result<SharePermission, Error> {
        let row = sqlx::query_as::<_, PermissionRow>(
            r#"
            UPDATE share_permissions
            SET permission = $3
            WHERE share_id = $1 AND tenant_id = $2
            RETURNING tenant_id, share_id, permission, created_at
            "#,
        )
        .bind(share_id)
        .bind(tenant_id)
        .bind(level.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update permission: {}", e)))?;

        match row {
            Some(r) => Ok(r.into()),
            None => Err(Error::WorkspaceNotFound(
                "Permission not found".to_string(),
            )),
        }
    }
}
