//! PostgreSQL database layer

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use uuid::Uuid;

use crate::domain::sandbox::{CreateSandboxParams, Sandbox, SandboxState};
use crate::error::{Error, Result};

/// Database row for sandbox (plain SELECT, no JOIN)
#[derive(Debug, FromRow)]
struct SandboxRow {
    id: uuid::Uuid,
    namespace_id: uuid::Uuid,
    root_path: String,
    name: Option<String>,
    template: String,
    state: String,
    container_id: Option<String>,
    env: serde_json::Value,
    metadata: serde_json::Value,
    timeout: i32,
    error_message: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Database row for sandbox with tenant name JOIN
#[derive(Debug, FromRow)]
struct SandboxWithNameRow {
    id: uuid::Uuid,
    namespace_id: uuid::Uuid,
    namespace_name: Option<String>,
    root_path: String,
    name: Option<String>,
    template: String,
    state: String,
    container_id: Option<String>,
    env: serde_json::Value,
    metadata: serde_json::Value,
    timeout: i32,
    error_message: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<SandboxRow> for Sandbox {
    type Error = Error;

    fn try_from(row: SandboxRow) -> Result<Self> {
        let state = SandboxState::from_str(&row.state)
            .ok_or_else(|| Error::Internal(format!("Invalid sandbox state: {}", row.state)))?;

        let env: HashMap<String, String> = serde_json::from_value(row.env)
            .map_err(|e| Error::Internal(format!("Failed to parse env: {}", e)))?;

        let metadata: HashMap<String, String> = serde_json::from_value(row.metadata)
            .map_err(|e| Error::Internal(format!("Failed to parse metadata: {}", e)))?;

        Ok(Sandbox {
            id: row.id,
            namespace_id: row.namespace_id,
            namespace_name: None,
            root_path: row.root_path,
            name: row.name,
            template: row.template,
            state,
            container_id: row.container_id,
            env,
            metadata,
            created_at: row.created_at,
            updated_at: row.updated_at,
            timeout: row.timeout,
            error_message: row.error_message,
        })
    }
}

impl TryFrom<SandboxWithNameRow> for Sandbox {
    type Error = Error;

    fn try_from(row: SandboxWithNameRow) -> Result<Self> {
        let state = SandboxState::from_str(&row.state)
            .ok_or_else(|| Error::Internal(format!("Invalid sandbox state: {}", row.state)))?;

        let env: HashMap<String, String> = serde_json::from_value(row.env)
            .map_err(|e| Error::Internal(format!("Failed to parse env: {}", e)))?;

        let metadata: HashMap<String, String> = serde_json::from_value(row.metadata)
            .map_err(|e| Error::Internal(format!("Failed to parse metadata: {}", e)))?;

        Ok(Sandbox {
            id: row.id,
            namespace_id: row.namespace_id,
            namespace_name: row.namespace_name,
            root_path: row.root_path,
            name: row.name,
            template: row.template,
            state,
            container_id: row.container_id,
            env,
            metadata,
            created_at: row.created_at,
            updated_at: row.updated_at,
            timeout: row.timeout,
            error_message: row.error_message,
        })
    }
}

/// Sandbox repository for database operations
pub struct SandboxRepository {
    pool: PgPool,
}

impl SandboxRepository {
    /// Create a new repository with the given pool
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Initialize the database connection pool
    pub async fn init(database_url: &str) -> anyhow::Result<PgPool> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(pool)
    }

    /// Get the pool reference
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Create a new sandbox
    pub async fn create(&self, params: CreateSandboxParams) -> Result<Sandbox> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let template = params.template.unwrap_or_else(|| "default".to_string());
        let env = serde_json::to_value(params.env.unwrap_or_default())
            .map_err(|e| Error::Internal(e.to_string()))?;
        let metadata = serde_json::to_value(params.metadata.unwrap_or_default())
            .map_err(|e| Error::Internal(e.to_string()))?;
        let timeout = params.timeout.unwrap_or(0);

        // Use a transaction to insert sandbox + mounts atomically
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO sandboxes (id, namespace_id, root_path, name, template, state, env, metadata, timeout, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(id)
        .bind(params.namespace_id)
        .bind(&params.root_path)
        .bind(&params.name)
        .bind(&template)
        .bind(SandboxState::Starting.as_str())
        .bind(&env)
        .bind(&metadata)
        .bind(timeout)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        // Insert sandbox mounts
        for mount in &params.mounts {
            sqlx::query(
                "INSERT INTO sandbox_mounts (sandbox_id, share_id, mount_path) VALUES ($1, $2, $3)",
            )
            .bind(id)
            .bind(mount.share_id)
            .bind(&mount.mount_path)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Internal(format!("Failed to insert mount: {}", e)))?;
        }

        tx.commit().await?;

        self.get(id).await
    }

    /// Get a sandbox by ID
    pub async fn get(&self, id: Uuid) -> Result<Sandbox> {
        let row: SandboxRow = sqlx::query_as(
            r#"
            SELECT id, namespace_id, root_path, name, template, state, container_id, env, metadata, timeout, error_message, created_at, updated_at
            FROM sandboxes
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::SandboxNotFound(id.to_string()))?;

        row.try_into()
    }

    /// List all sandboxes with optional state filter (JOINs tenants for namespace_name)
    pub async fn list(&self, state_filter: Option<SandboxState>) -> Result<Vec<Sandbox>> {
        let rows: Vec<SandboxWithNameRow> = match state_filter {
            Some(state) => {
                sqlx::query_as(
                    r#"
                    SELECT s.id, s.namespace_id, t.name AS namespace_name,
                           s.root_path, s.name, s.template, s.state, s.container_id,
                           s.env, s.metadata, s.timeout, s.error_message,
                           s.created_at, s.updated_at
                    FROM sandboxes s
                    LEFT JOIN tenants t ON t.id = s.namespace_id
                    WHERE s.state = $1
                    ORDER BY s.created_at DESC
                    "#,
                )
                .bind(state.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    SELECT s.id, s.namespace_id, t.name AS namespace_name,
                           s.root_path, s.name, s.template, s.state, s.container_id,
                           s.env, s.metadata, s.timeout, s.error_message,
                           s.created_at, s.updated_at
                    FROM sandboxes s
                    LEFT JOIN tenants t ON t.id = s.namespace_id
                    ORDER BY s.created_at DESC
                    "#,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// List sandboxes by namespace ID with optional state filter (JOINs tenants for namespace_name)
    pub async fn list_by_namespace(
        &self,
        namespace_id: Uuid,
        state_filter: Option<SandboxState>,
    ) -> Result<Vec<Sandbox>> {
        let rows: Vec<SandboxWithNameRow> = match state_filter {
            Some(state) => {
                sqlx::query_as(
                    r#"
                    SELECT s.id, s.namespace_id, t.name AS namespace_name,
                           s.root_path, s.name, s.template, s.state, s.container_id,
                           s.env, s.metadata, s.timeout, s.error_message,
                           s.created_at, s.updated_at
                    FROM sandboxes s
                    LEFT JOIN tenants t ON t.id = s.namespace_id
                    WHERE s.namespace_id = $1 AND s.state = $2
                    ORDER BY s.created_at DESC
                    "#,
                )
                .bind(namespace_id)
                .bind(state.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    SELECT s.id, s.namespace_id, t.name AS namespace_name,
                           s.root_path, s.name, s.template, s.state, s.container_id,
                           s.env, s.metadata, s.timeout, s.error_message,
                           s.created_at, s.updated_at
                    FROM sandboxes s
                    LEFT JOIN tenants t ON t.id = s.namespace_id
                    WHERE s.namespace_id = $1
                    ORDER BY s.created_at DESC
                    "#,
                )
                .bind(namespace_id)
                .fetch_all(&self.pool)
                .await?
            }
        };

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// Update sandbox state
    pub async fn update_state(
        &self,
        id: Uuid,
        state: SandboxState,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE sandboxes
            SET state = $1, error_message = $2, updated_at = $3
            WHERE id = $4
            "#,
        )
        .bind(state.as_str())
        .bind(error_message)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::SandboxNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Update sandbox container ID
    pub async fn update_container_id(&self, id: Uuid, container_id: &str) -> Result<()> {
        let now = Utc::now();

        let result = sqlx::query(
            r#"
            UPDATE sandboxes
            SET container_id = $1, updated_at = $2
            WHERE id = $3
            "#,
        )
        .bind(container_id)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::SandboxNotFound(id.to_string()));
        }

        Ok(())
    }

    /// Delete a sandbox
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM sandboxes WHERE id = $1")
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
            SELECT COUNT(*) FROM sandboxes WHERE state = $1
            "#,
        )
        .bind(state.as_str())
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// Get sandboxes that have exceeded their timeout
    pub async fn get_expired_sandboxes(&self) -> Result<Vec<Sandbox>> {
        let rows: Vec<SandboxRow> = sqlx::query_as(
            r#"
            SELECT id, namespace_id, root_path, name, template, state, container_id, env, metadata, timeout, error_message, created_at, updated_at
            FROM sandboxes
            WHERE state = 'running'
              AND timeout > 0
              AND created_at + (timeout * interval '1 second') < now()
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// Get a sandbox including tenant name (via JOIN)
    pub async fn get_with_name(&self, id: Uuid) -> Result<Sandbox> {
        let row: SandboxWithNameRow = sqlx::query_as(
            r#"
            SELECT s.id, s.namespace_id, t.name AS namespace_name,
                   s.root_path, s.name, s.template, s.state, s.container_id,
                   s.env, s.metadata, s.timeout, s.error_message,
                   s.created_at, s.updated_at
            FROM sandboxes s
            LEFT JOIN tenants t ON t.id = s.namespace_id
            WHERE s.id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::SandboxNotFound(id.to_string()))?;

        row.try_into()
    }

    /// Get mounts for a sandbox
    pub async fn get_mounts(
        &self,
        sandbox_id: Uuid,
    ) -> Result<Vec<crate::domain::share::SandboxMount>> {
        #[derive(Debug, FromRow)]
        struct MountRow {
            sandbox_id: Uuid,
            share_id: Uuid,
            mount_path: String,
        }

        let rows: Vec<MountRow> = sqlx::query_as(
            "SELECT sandbox_id, share_id, mount_path FROM sandbox_mounts WHERE sandbox_id = $1",
        )
        .bind(sandbox_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| crate::domain::share::SandboxMount {
                sandbox_id: r.sandbox_id,
                share_id: r.share_id,
                mount_path: r.mount_path,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a test tenant (namespace)
    async fn create_test_tenant(pool: &PgPool) -> Uuid {
        let tenant_id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            r#"INSERT INTO tenants (id, name, description, is_active, storage_type, storage_config, created_at, updated_at)
               VALUES ($1, $2, '', true, 'managed', '{}', $3, $4)"#,
        )
        .bind(tenant_id)
        .bind("test-tenant")
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .expect("Failed to create test tenant");
        tenant_id
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_create_and_get_sandbox(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let repo = SandboxRepository::new(pool);

        let params = CreateSandboxParams {
            namespace_id,
            root_path: "/".to_string(),
            template: Some("python:3.11".to_string()),
            name: Some("test-sandbox".to_string()),
            env: None,
            metadata: None,
            timeout: Some(3600),
            mounts: vec![],
        };

        let sandbox = repo.create(params).await.expect("Failed to create sandbox");
        assert_eq!(sandbox.namespace_id, namespace_id);
        assert_eq!(sandbox.name, Some("test-sandbox".to_string()));
        assert_eq!(sandbox.template, "python:3.11");
        assert_eq!(sandbox.state, SandboxState::Starting);
        assert_eq!(sandbox.timeout, 3600);

        let fetched = repo.get(sandbox.id).await.expect("Failed to get sandbox");
        assert_eq!(fetched.id, sandbox.id);
        assert_eq!(fetched.name, sandbox.name);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_update_state(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let repo = SandboxRepository::new(pool);

        let params = CreateSandboxParams {
            namespace_id,
            root_path: "/".to_string(),
            template: None,
            name: None,
            env: None,
            metadata: None,
            timeout: None,
            mounts: vec![],
        };

        let sandbox = repo.create(params).await.expect("Failed to create sandbox");

        repo.update_state(sandbox.id, SandboxState::Running, None)
            .await
            .expect("Failed to update state");

        let fetched = repo.get(sandbox.id).await.expect("Failed to get sandbox");
        assert_eq!(fetched.state, SandboxState::Running);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_sandboxes(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let repo = SandboxRepository::new(pool);

        // Create two sandboxes
        let params1 = CreateSandboxParams {
            namespace_id,
            root_path: "/".to_string(),
            template: Some("python".to_string()),
            name: Some("sandbox1".to_string()),
            env: None,
            metadata: None,
            timeout: None,
            mounts: vec![],
        };
        let params2 = CreateSandboxParams {
            namespace_id,
            root_path: "/".to_string(),
            template: Some("node".to_string()),
            name: Some("sandbox2".to_string()),
            env: None,
            metadata: None,
            timeout: None,
            mounts: vec![],
        };

        repo.create(params1)
            .await
            .expect("Failed to create sandbox 1");
        let sandbox2 = repo
            .create(params2)
            .await
            .expect("Failed to create sandbox 2");

        // Update one to running
        repo.update_state(sandbox2.id, SandboxState::Running, None)
            .await
            .expect("Failed to update state");

        // List all
        let all = repo.list(None).await.expect("Failed to list sandboxes");
        assert_eq!(all.len(), 2);

        // List by state
        let starting = repo
            .list(Some(SandboxState::Starting))
            .await
            .expect("Failed to list");
        assert_eq!(starting.len(), 1);

        let running = repo
            .list(Some(SandboxState::Running))
            .await
            .expect("Failed to list");
        assert_eq!(running.len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_delete_sandbox(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let repo = SandboxRepository::new(pool);

        let params = CreateSandboxParams {
            namespace_id,
            root_path: "/".to_string(),
            template: None,
            name: None,
            env: None,
            metadata: None,
            timeout: None,
            mounts: vec![],
        };

        let sandbox = repo.create(params).await.expect("Failed to create sandbox");

        repo.delete(sandbox.id)
            .await
            .expect("Failed to delete sandbox");

        let result = repo.get(sandbox.id).await;
        assert!(matches!(result, Err(Error::SandboxNotFound(_))));
    }
}
