//! Workspace repository for database operations

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::domain::workspace::{CreateWorkspaceParams, Workspace};
use crate::error::{Error, Result};

/// Database row for workspace
#[derive(Debug, FromRow)]
struct WorkspaceRow {
    id: String,
    name: Option<String>,
    nfs_url: Option<String>,
    metadata: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<WorkspaceRow> for Workspace {
    type Error = Error;

    fn try_from(row: WorkspaceRow) -> Result<Self> {
        let metadata: HashMap<String, String> = serde_json::from_str(&row.metadata)
            .map_err(|e| Error::Internal(format!("Failed to parse metadata: {}", e)))?;

        let created_at = DateTime::parse_from_rfc3339(&row.created_at)
            .map_err(|e| Error::Internal(format!("Failed to parse created_at: {}", e)))?
            .with_timezone(&Utc);

        let updated_at = DateTime::parse_from_rfc3339(&row.updated_at)
            .map_err(|e| Error::Internal(format!("Failed to parse updated_at: {}", e)))?
            .with_timezone(&Utc);

        Ok(Workspace {
            id: row.id,
            name: row.name,
            nfs_url: row.nfs_url,
            metadata,
            created_at,
            updated_at,
        })
    }
}

/// Workspace repository for database operations
pub struct WorkspaceRepository {
    pool: SqlitePool,
}

impl WorkspaceRepository {
    /// Create a new repository with the given pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new workspace
    pub async fn create(&self, params: CreateWorkspaceParams) -> Result<Workspace> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let metadata = serde_json::to_string(&params.metadata.unwrap_or_default())
            .map_err(|e| Error::Internal(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO workspaces (id, name, metadata, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&params.name)
        .bind(&metadata)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;

        self.get(&id).await
    }

    /// Get a workspace by ID
    pub async fn get(&self, id: &str) -> Result<Workspace> {
        let row: WorkspaceRow = sqlx::query_as(
            r#"
            SELECT id, name, nfs_url, metadata, created_at, updated_at
            FROM workspaces
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::WorkspaceNotFound(id.to_string()))?;

        row.try_into()
    }

    /// List all workspaces
    pub async fn list(&self) -> Result<Vec<Workspace>> {
        let rows: Vec<WorkspaceRow> = sqlx::query_as(
            r#"
            SELECT id, name, nfs_url, metadata, created_at, updated_at
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
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE workspaces
            SET nfs_url = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(nfs_url)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Delete a workspace
    pub async fn delete(&self, id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM workspaces WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(Error::WorkspaceNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Check if a workspace has any sandboxes
    pub async fn has_sandboxes(&self, workspace_id: &str) -> Result<bool> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM sandboxes WHERE workspace_id = ?
            "#,
        )
        .bind(workspace_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }

    /// Count sandboxes for a workspace
    pub async fn count_sandboxes(&self, workspace_id: &str) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM sandboxes WHERE workspace_id = ?
            "#,
        )
        .bind(workspace_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_workspace() {
        let pool = create_test_pool().await;
        let repo = WorkspaceRepository::new(pool);

        let params = CreateWorkspaceParams {
            name: Some("test-workspace".to_string()),
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

    #[tokio::test]
    async fn test_list_workspaces() {
        let pool = create_test_pool().await;
        let repo = WorkspaceRepository::new(pool);

        let params1 = CreateWorkspaceParams {
            name: Some("workspace1".to_string()),
            metadata: None,
        };
        let params2 = CreateWorkspaceParams {
            name: Some("workspace2".to_string()),
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

    #[tokio::test]
    async fn test_update_nfs_url() {
        let pool = create_test_pool().await;
        let repo = WorkspaceRepository::new(pool);

        let params = CreateWorkspaceParams {
            name: Some("test".to_string()),
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

    #[tokio::test]
    async fn test_delete_workspace() {
        let pool = create_test_pool().await;
        let repo = WorkspaceRepository::new(pool);

        let params = CreateWorkspaceParams {
            name: None,
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
