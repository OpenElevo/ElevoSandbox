//! Workspace repository for database operations

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::domain::workspace::{
    CreateWorkspaceParams, RemoteStorageConfig, StorageType, Workspace,
};
use crate::error::{Error, Result};

/// Database row for workspace
#[derive(Debug, FromRow)]
struct WorkspaceRow {
    id: uuid::Uuid,
    name: Option<String>,
    nfs_url: Option<String>,
    storage_type: String,
    storage_config: serde_json::Value,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<WorkspaceRow> for Workspace {
    type Error = Error;

    fn try_from(row: WorkspaceRow) -> Result<Self> {
        let metadata: HashMap<String, String> = serde_json::from_value(row.metadata)
            .map_err(|e| Error::Internal(format!("Failed to parse metadata: {}", e)))?;

        let storage_type = StorageType::from_str(&row.storage_type)
            .map_err(|e| Error::Internal(format!("Failed to parse storage_type: {}", e)))?;

        let storage_config: RemoteStorageConfig =
            if row.storage_config == serde_json::json!({}) {
                RemoteStorageConfig::default()
            } else {
                serde_json::from_value(row.storage_config).map_err(|e| {
                    Error::Internal(format!("Failed to parse storage_config: {}", e))
                })?
            };

        if let Err(e) = storage_config.validate() {
            return Err(Error::Internal(format!("Invalid storage_config: {}", e)));
        }

        Ok(Workspace {
            id: row.id.to_string(),
            name: row.name,
            nfs_url: row.nfs_url,
            storage_type,
            storage_config,
            metadata,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// Workspace repository for database operations
pub struct WorkspaceRepository {
    pool: PgPool,
}

impl WorkspaceRepository {
    /// Create a new repository with the given pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new workspace
    pub async fn create(&self, params: CreateWorkspaceParams) -> Result<Workspace> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let metadata = serde_json::to_value(&params.metadata.unwrap_or_default())
            .map_err(|e| Error::Internal(e.to_string()))?;

        let storage_type = params.storage_type.unwrap_or(StorageType::Managed);
        let storage_config = if storage_type == StorageType::Remote {
            serde_json::to_value(&RemoteStorageConfig::default())
                .map_err(|e| Error::Internal(e.to_string()))?
        } else {
            serde_json::json!({})
        };

        sqlx::query(
            r#"
            INSERT INTO workspaces (id, name, storage_type, storage_config, metadata, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(&params.name)
        .bind(storage_type.as_str())
        .bind(&storage_config)
        .bind(&metadata)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;

        self.get(&id.to_string()).await
    }

    /// Get a workspace by ID
    pub async fn get(&self, id: &str) -> Result<Workspace> {
        let uuid = Uuid::parse_str(id)
            .map_err(|_| Error::WorkspaceNotFound(id.to_string()))?;

        let row: WorkspaceRow = sqlx::query_as(
            r#"
            SELECT id, name, nfs_url, storage_type, storage_config, metadata, created_at, updated_at
            FROM workspaces
            WHERE id = $1
            "#,
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::WorkspaceNotFound(id.to_string()))?;

        row.try_into()
    }

    /// List all workspaces
    pub async fn list(&self) -> Result<Vec<Workspace>> {
        let rows: Vec<WorkspaceRow> = sqlx::query_as(
            r#"
            SELECT id, name, nfs_url, storage_type, storage_config, metadata, created_at, updated_at
            FROM workspaces
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// Update workspace NFS URL
    pub async fn update_nfs_url(&self, id: &str, nfs_url: &str) -> Result<()> {
        let uuid = Uuid::parse_str(id)
            .map_err(|_| Error::WorkspaceNotFound(id.to_string()))?;
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE workspaces
            SET nfs_url = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(nfs_url)
        .bind(now)
        .bind(uuid)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Delete a workspace
    pub async fn delete(&self, id: &str) -> Result<()> {
        let uuid = Uuid::parse_str(id)
            .map_err(|_| Error::WorkspaceNotFound(id.to_string()))?;

        let result = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Update the storage_config JSON for a workspace
    pub async fn update_storage_config(
        &self,
        id: &str,
        config: &RemoteStorageConfig,
    ) -> Result<()> {
        let uuid = Uuid::parse_str(id)
            .map_err(|_| Error::WorkspaceNotFound(id.to_string()))?;
        let config_json = serde_json::to_value(config)
            .map_err(|e| Error::Internal(format!("Failed to serialize storage_config: {}", e)))?;
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE workspaces
            SET storage_config = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(&config_json)
        .bind(now)
        .bind(uuid)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Count the number of remote workspaces
    pub async fn count_remote(&self) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM workspaces WHERE storage_type = 'remote'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// List all remote workspaces (storage_type='remote')
    pub async fn list_remote(&self) -> Result<Vec<Workspace>> {
        let rows: Vec<WorkspaceRow> = sqlx::query_as(
            r#"
            SELECT id, name, nfs_url, storage_type, storage_config, metadata, created_at, updated_at
            FROM workspaces
            WHERE storage_type = 'remote'
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// Check if a workspace/namespace has any sandboxes
    pub async fn has_sandboxes(&self, workspace_id: &str) -> Result<bool> {
        let uuid = Uuid::parse_str(workspace_id)
            .map_err(|_| Error::WorkspaceNotFound(workspace_id.to_string()))?;

        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM sandboxes WHERE namespace_id = $1
            "#,
        )
        .bind(uuid)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }

    /// Count sandboxes for a workspace/namespace
    pub async fn count_sandboxes(&self, workspace_id: &str) -> Result<i64> {
        let uuid = Uuid::parse_str(workspace_id)
            .map_err(|_| Error::WorkspaceNotFound(workspace_id.to_string()))?;

        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM sandboxes WHERE namespace_id = $1
            "#,
        )
        .bind(uuid)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn test_create_and_get_workspace(pool: PgPool) {
        let repo = WorkspaceRepository::new(pool);

        let params = CreateWorkspaceParams {
            name: Some("test-workspace".to_string()),
            storage_type: None,
            metadata: None,
        };

        let workspace = repo
            .create(params)
            .await
            .expect("Failed to create workspace");
        assert_eq!(workspace.name, Some("test-workspace".to_string()));

        let fetched = repo
            .get(&workspace.id)
            .await
            .expect("Failed to get workspace");
        assert_eq!(fetched.id, workspace.id);
        assert_eq!(fetched.name, workspace.name);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_workspaces(pool: PgPool) {
        let repo = WorkspaceRepository::new(pool);

        let params1 = CreateWorkspaceParams {
            name: Some("workspace1".to_string()),
            storage_type: None,
            metadata: None,
        };
        let params2 = CreateWorkspaceParams {
            name: Some("workspace2".to_string()),
            storage_type: None,
            metadata: None,
        };

        repo.create(params1)
            .await
            .expect("Failed to create workspace 1");
        repo.create(params2)
            .await
            .expect("Failed to create workspace 2");

        let all = repo.list().await.expect("Failed to list workspaces");
        assert_eq!(all.len(), 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_update_nfs_url(pool: PgPool) {
        let repo = WorkspaceRepository::new(pool);

        let params = CreateWorkspaceParams {
            name: Some("test".to_string()),
            storage_type: None,
            metadata: None,
        };

        let workspace = repo
            .create(params)
            .await
            .expect("Failed to create workspace");

        repo.update_nfs_url(&workspace.id, "nfs://localhost:2049/test")
            .await
            .expect("Failed to update nfs_url");

        let fetched = repo
            .get(&workspace.id)
            .await
            .expect("Failed to get workspace");
        assert_eq!(
            fetched.nfs_url,
            Some("nfs://localhost:2049/test".to_string())
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_delete_workspace(pool: PgPool) {
        let repo = WorkspaceRepository::new(pool);

        let params = CreateWorkspaceParams {
            name: None,
            storage_type: None,
            metadata: None,
        };

        let workspace = repo
            .create(params)
            .await
            .expect("Failed to create workspace");

        repo.delete(&workspace.id)
            .await
            .expect("Failed to delete workspace");

        let result = repo.get(&workspace.id).await;
        assert!(matches!(result, Err(Error::WorkspaceNotFound(_))));
    }
}
