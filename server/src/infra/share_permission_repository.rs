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
            tenant_id: row.tenant_id.to_string(),
            share_id: row.share_id.to_string(),
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
        share_id: &str,
        tenant_id: &str,
        level: PermissionLevel,
    ) -> Result<SharePermission, Error> {
        let share_uuid = Uuid::parse_str(share_id)
            .map_err(|_| Error::InvalidParameter("Invalid share ID".into()))?;
        let tenant_uuid = Uuid::parse_str(tenant_id)
            .map_err(|_| Error::InvalidParameter("Invalid tenant ID".into()))?;

        let row = sqlx::query_as::<_, PermissionRow>(
            r#"
            INSERT INTO share_permissions (tenant_id, share_id, permission)
            VALUES ($1, $2, $3)
            ON CONFLICT (tenant_id, share_id)
            DO UPDATE SET permission = $3
            RETURNING tenant_id, share_id, permission, created_at
            "#,
        )
        .bind(tenant_uuid)
        .bind(share_uuid)
        .bind(level.as_str())
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("Failed to grant permission: {}", e)))?;

        Ok(row.into())
    }

    /// Revoke permission for a tenant on a share
    pub async fn revoke_permission(
        &self,
        share_id: &str,
        tenant_id: &str,
    ) -> Result<(), Error> {
        let share_uuid = Uuid::parse_str(share_id)
            .map_err(|_| Error::InvalidParameter("Invalid share ID".into()))?;
        let tenant_uuid = Uuid::parse_str(tenant_id)
            .map_err(|_| Error::InvalidParameter("Invalid tenant ID".into()))?;

        let result = sqlx::query(
            "DELETE FROM share_permissions WHERE share_id = $1 AND tenant_id = $2",
        )
        .bind(share_uuid)
        .bind(tenant_uuid)
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
        share_id: &str,
        tenant_id: &str,
    ) -> Result<Option<PermissionLevel>, Error> {
        let share_uuid = Uuid::parse_str(share_id)
            .map_err(|_| Error::InvalidParameter("Invalid share ID".into()))?;
        let tenant_uuid = Uuid::parse_str(tenant_id)
            .map_err(|_| Error::InvalidParameter("Invalid tenant ID".into()))?;

        let row = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM share_permissions WHERE share_id = $1 AND tenant_id = $2",
        )
        .bind(share_uuid)
        .bind(tenant_uuid)
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
        share_id: &str,
    ) -> Result<Vec<SharePermission>, Error> {
        let share_uuid = Uuid::parse_str(share_id)
            .map_err(|_| Error::InvalidParameter("Invalid share ID".into()))?;

        let rows = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM share_permissions WHERE share_id = $1 ORDER BY created_at",
        )
        .bind(share_uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        Ok(rows.into_iter().map(SharePermission::from).collect())
    }

    /// List all permissions for a tenant
    pub async fn list_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<SharePermission>, Error> {
        let tenant_uuid = Uuid::parse_str(tenant_id)
            .map_err(|_| Error::InvalidParameter("Invalid tenant ID".into()))?;

        let rows = sqlx::query_as::<_, PermissionRow>(
            "SELECT * FROM share_permissions WHERE tenant_id = $1 ORDER BY created_at",
        )
        .bind(tenant_uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Internal(format!("DB error: {}", e)))?;

        Ok(rows.into_iter().map(SharePermission::from).collect())
    }
}
