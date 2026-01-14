//! SQLite database layer

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePoolOptions, FromRow, SqlitePool};
use uuid::Uuid;

use crate::domain::sandbox::{CreateSandboxParams, Sandbox, SandboxState};
use crate::error::{Error, Result};

/// Database row for sandbox
#[derive(Debug, FromRow)]
struct SandboxRow {
    id: String,
    name: Option<String>,
    template: String,
    state: String,
    container_id: Option<String>,
    env: String,
    metadata: String,
    nfs_url: Option<String>,
    timeout: i64,
    error_message: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<SandboxRow> for Sandbox {
    type Error = Error;

    fn try_from(row: SandboxRow) -> Result<Self> {
        let state = SandboxState::from_str(&row.state)
            .ok_or_else(|| Error::Internal(format!("Invalid sandbox state: {}", row.state)))?;

        let env: HashMap<String, String> = serde_json::from_str(&row.env)
            .map_err(|e| Error::Internal(format!("Failed to parse env: {}", e)))?;

        let metadata: HashMap<String, String> = serde_json::from_str(&row.metadata)
            .map_err(|e| Error::Internal(format!("Failed to parse metadata: {}", e)))?;

        let created_at = DateTime::parse_from_rfc3339(&row.created_at)
            .map_err(|e| Error::Internal(format!("Failed to parse created_at: {}", e)))?
            .with_timezone(&Utc);

        let updated_at = DateTime::parse_from_rfc3339(&row.updated_at)
            .map_err(|e| Error::Internal(format!("Failed to parse updated_at: {}", e)))?
            .with_timezone(&Utc);

        Ok(Sandbox {
            id: row.id,
            name: row.name,
            template: row.template,
            state,
            container_id: row.container_id,
            env,
            metadata,
            nfs_url: row.nfs_url,
            created_at,
            updated_at,
            timeout: row.timeout as u64,
            error_message: row.error_message,
        })
    }
}

/// Sandbox repository for database operations
pub struct SandboxRepository {
    pool: SqlitePool,
}

impl SandboxRepository {
    /// Create a new repository with the given pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Initialize the database connection pool
    pub async fn init(database_url: &str) -> anyhow::Result<SqlitePool> {
        // Ensure parent directory exists
        if let Some(path) = database_url.strip_prefix("sqlite:") {
            if let Some(path) = path.split('?').next() {
                if let Some(parent) = std::path::Path::new(path).parent() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        // Enable WAL mode for better concurrent performance
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(pool)
    }

    /// Get the pool reference
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Create a new sandbox
    pub async fn create(&self, params: CreateSandboxParams) -> Result<Sandbox> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let template = params.template.unwrap_or_else(|| "default".to_string());
        let env = serde_json::to_string(&params.env.unwrap_or_default())
            .map_err(|e| Error::Internal(e.to_string()))?;
        let metadata = serde_json::to_string(&params.metadata.unwrap_or_default())
            .map_err(|e| Error::Internal(e.to_string()))?;
        let timeout = params.timeout.unwrap_or(0) as i64;

        sqlx::query(
            r#"
            INSERT INTO sandboxes (id, name, template, state, env, metadata, timeout, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&params.name)
        .bind(&template)
        .bind(SandboxState::Starting.as_str())
        .bind(&env)
        .bind(&metadata)
        .bind(timeout)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;

        self.get(&id).await
    }

    /// Get a sandbox by ID
    pub async fn get(&self, id: &str) -> Result<Sandbox> {
        let row: SandboxRow = sqlx::query_as(
            r#"
            SELECT id, name, template, state, container_id, env, metadata, nfs_url, timeout, error_message, created_at, updated_at
            FROM sandboxes
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::SandboxNotFound(id.to_string()))?;

        row.try_into()
    }

    /// List all sandboxes with optional state filter
    pub async fn list(&self, state_filter: Option<SandboxState>) -> Result<Vec<Sandbox>> {
        let rows: Vec<SandboxRow> = match state_filter {
            Some(state) => {
                sqlx::query_as(
                    r#"
                    SELECT id, name, template, state, container_id, env, metadata, nfs_url, timeout, error_message, created_at, updated_at
                    FROM sandboxes
                    WHERE state = ?
                    ORDER BY created_at DESC
                    "#,
                )
                .bind(state.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    SELECT id, name, template, state, container_id, env, metadata, nfs_url, timeout, error_message, created_at, updated_at
                    FROM sandboxes
                    ORDER BY created_at DESC
                    "#,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// Update sandbox state
    pub async fn update_state(&self, id: &str, state: SandboxState, error_message: Option<&str>) -> Result<()> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE sandboxes
            SET state = ?, error_message = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(state.as_str())
        .bind(error_message)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::SandboxNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Update sandbox container ID
    pub async fn update_container_id(&self, id: &str, container_id: &str) -> Result<()> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE sandboxes
            SET container_id = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(container_id)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::SandboxNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Update sandbox NFS URL
    pub async fn update_nfs_url(&self, id: &str, nfs_url: &str) -> Result<()> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE sandboxes
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
            return Err(Error::SandboxNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Delete a sandbox
    pub async fn delete(&self, id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM sandboxes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(Error::SandboxNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Count sandboxes by state
    pub async fn count_by_state(&self, state: SandboxState) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM sandboxes WHERE state = ?
            "#,
        )
        .bind(state.as_str())
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// Get sandboxes that have exceeded their timeout
    pub async fn get_expired_sandboxes(&self) -> Result<Vec<Sandbox>> {
        let now = Utc::now();

        let rows: Vec<SandboxRow> = sqlx::query_as(
            r#"
            SELECT id, name, template, state, container_id, env, metadata, nfs_url, timeout, error_message, created_at, updated_at
            FROM sandboxes
            WHERE state = 'running'
              AND timeout > 0
              AND datetime(created_at, '+' || timeout || ' seconds') < datetime(?)
            "#,
        )
        .bind(now.to_rfc3339())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn test_create_and_get_sandbox() {
        let pool = create_test_pool().await;
        let repo = SandboxRepository::new(pool);

        let params = CreateSandboxParams {
            template: Some("python:3.11".to_string()),
            name: Some("test-sandbox".to_string()),
            env: None,
            metadata: None,
            timeout: Some(3600),
        };

        let sandbox = repo.create(params).await.expect("Failed to create sandbox");
        assert_eq!(sandbox.name, Some("test-sandbox".to_string()));
        assert_eq!(sandbox.template, "python:3.11");
        assert_eq!(sandbox.state, SandboxState::Starting);
        assert_eq!(sandbox.timeout, 3600);

        let fetched = repo.get(&sandbox.id).await.expect("Failed to get sandbox");
        assert_eq!(fetched.id, sandbox.id);
        assert_eq!(fetched.name, sandbox.name);
    }

    #[tokio::test]
    async fn test_update_state() {
        let pool = create_test_pool().await;
        let repo = SandboxRepository::new(pool);

        let params = CreateSandboxParams {
            template: None,
            name: None,
            env: None,
            metadata: None,
            timeout: None,
        };

        let sandbox = repo.create(params).await.expect("Failed to create sandbox");

        repo.update_state(&sandbox.id, SandboxState::Running, None)
            .await
            .expect("Failed to update state");

        let fetched = repo.get(&sandbox.id).await.expect("Failed to get sandbox");
        assert_eq!(fetched.state, SandboxState::Running);
    }

    #[tokio::test]
    async fn test_list_sandboxes() {
        let pool = create_test_pool().await;
        let repo = SandboxRepository::new(pool);

        // Create two sandboxes
        let params1 = CreateSandboxParams {
            template: Some("python".to_string()),
            name: Some("sandbox1".to_string()),
            env: None,
            metadata: None,
            timeout: None,
        };
        let params2 = CreateSandboxParams {
            template: Some("node".to_string()),
            name: Some("sandbox2".to_string()),
            env: None,
            metadata: None,
            timeout: None,
        };

        repo.create(params1).await.expect("Failed to create sandbox 1");
        let sandbox2 = repo.create(params2).await.expect("Failed to create sandbox 2");

        // Update one to running
        repo.update_state(&sandbox2.id, SandboxState::Running, None)
            .await
            .expect("Failed to update state");

        // List all
        let all = repo.list(None).await.expect("Failed to list sandboxes");
        assert_eq!(all.len(), 2);

        // List by state
        let starting = repo.list(Some(SandboxState::Starting)).await.expect("Failed to list");
        assert_eq!(starting.len(), 1);

        let running = repo.list(Some(SandboxState::Running)).await.expect("Failed to list");
        assert_eq!(running.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_sandbox() {
        let pool = create_test_pool().await;
        let repo = SandboxRepository::new(pool);

        let params = CreateSandboxParams {
            template: None,
            name: None,
            env: None,
            metadata: None,
            timeout: None,
        };

        let sandbox = repo.create(params).await.expect("Failed to create sandbox");

        repo.delete(&sandbox.id).await.expect("Failed to delete sandbox");

        let result = repo.get(&sandbox.id).await;
        assert!(matches!(result, Err(Error::SandboxNotFound(_))));
    }
}
