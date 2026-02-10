//! Workspace lease-based concurrency control
//!
//! Provides single-instance workspace locking via SQLite. When a server instance
//! acquires a lease on a workspace, no other server instance may modify it until
//! the lease expires or is released.
//!
//! The distributed (PostgreSQL) version is deferred to the HA design doc.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Lease configuration constants
const DEFAULT_LEASE_DURATION_SECS: i64 = 60;
const DEFAULT_RENEWAL_INTERVAL_SECS: u64 = 20;

/// Workspace lease record
#[derive(Debug, Clone)]
pub struct WorkspaceLease {
    pub workspace_id: String,
    pub holder_id: String,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub renewed_at: DateTime<Utc>,
}

/// Workspace lease operations
#[async_trait]
pub trait WorkspaceLeaseManager: Send + Sync + 'static {
    /// Try to acquire a lease on a workspace.
    /// Returns `Ok(lease)` if acquired, `Err` if already held by another holder.
    async fn acquire(
        &self,
        workspace_id: &str,
        holder_id: &str,
    ) -> Result<WorkspaceLease, LeaseError>;

    /// Renew an existing lease held by the given holder.
    async fn renew(
        &self,
        workspace_id: &str,
        holder_id: &str,
    ) -> Result<WorkspaceLease, LeaseError>;

    /// Release a lease held by the given holder.
    async fn release(&self, workspace_id: &str, holder_id: &str) -> Result<(), LeaseError>;

    /// Check if a workspace has an active (non-expired) lease.
    async fn check(&self, workspace_id: &str) -> Result<Option<WorkspaceLease>, LeaseError>;
}

/// Lease error types
#[derive(Debug, thiserror::Error)]
pub enum LeaseError {
    #[error("workspace '{workspace_id}' is already leased by '{holder_id}' until {expires_at}")]
    AlreadyHeld {
        workspace_id: String,
        holder_id: String,
        expires_at: DateTime<Utc>,
    },

    #[error("no active lease found for workspace '{0}'")]
    NotFound(String),

    #[error("lease holder mismatch: expected '{expected}', got '{actual}'")]
    HolderMismatch { expected: String, actual: String },

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// SQLite-based workspace lease manager (single-instance)
pub struct SqliteLeaseManager {
    pool: SqlitePool,
    lease_duration: Duration,
}

impl SqliteLeaseManager {
    /// Create a new SQLite lease manager and ensure the table exists.
    pub async fn new(pool: SqlitePool) -> Result<Self, sqlx::Error> {
        let manager = Self {
            pool,
            lease_duration: Duration::from_secs(DEFAULT_LEASE_DURATION_SECS as u64),
        };
        manager.create_table().await?;
        Ok(manager)
    }

    /// Create the workspace_leases table if it doesn't exist
    async fn create_table(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workspace_leases (
                workspace_id TEXT PRIMARY KEY,
                holder_id    TEXT NOT NULL,
                acquired_at  TEXT NOT NULL,
                expires_at   TEXT NOT NULL,
                renewed_at   TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Compute the expiration time from now based on configured lease duration
    fn compute_expires_at(&self) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::seconds(self.lease_duration.as_secs() as i64)
    }
}

#[async_trait]
impl WorkspaceLeaseManager for SqliteLeaseManager {
    async fn acquire(
        &self,
        workspace_id: &str,
        holder_id: &str,
    ) -> Result<WorkspaceLease, LeaseError> {
        let now = Utc::now();
        let expires_at = self.compute_expires_at();
        let now_str = now.to_rfc3339();
        let expires_str = expires_at.to_rfc3339();

        // INSERT-first strategy: attempt to insert, handling conflict atomically.
        // Uses INSERT OR IGNORE to avoid errors on UNIQUE constraint violation,
        // then conditionally UPDATE if the row is expired or held by the same holder.
        let mut tx = self.pool.begin().await?;

        // Try to insert a new lease (ignored if workspace_id already exists)
        let insert_result = sqlx::query(
            r#"
            INSERT OR IGNORE INTO workspace_leases
                (workspace_id, holder_id, acquired_at, expires_at, renewed_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(workspace_id)
        .bind(holder_id)
        .bind(&now_str)
        .bind(&expires_str)
        .bind(&now_str)
        .execute(&mut *tx)
        .await?;

        if insert_result.rows_affected() == 1 {
            // New lease inserted successfully
            tx.commit().await?;
            debug!(
                "Lease acquired for workspace '{}' by '{}'",
                workspace_id, holder_id
            );
            return Ok(WorkspaceLease {
                workspace_id: workspace_id.to_string(),
                holder_id: holder_id.to_string(),
                acquired_at: now,
                expires_at,
                renewed_at: now,
            });
        }

        // Row already exists — try to take over if expired or same holder
        let updated = sqlx::query(
            r#"
            UPDATE workspace_leases
            SET holder_id = ?, acquired_at = ?, expires_at = ?, renewed_at = ?
            WHERE workspace_id = ?
              AND (holder_id = ? OR expires_at < ?)
            "#,
        )
        .bind(holder_id)
        .bind(&now_str)
        .bind(&expires_str)
        .bind(&now_str)
        .bind(workspace_id)
        .bind(holder_id)
        .bind(&now_str)
        .execute(&mut *tx)
        .await?;

        if updated.rows_affected() == 1 {
            tx.commit().await?;
            debug!(
                "Lease re-acquired for workspace '{}' by '{}'",
                workspace_id, holder_id
            );
            return Ok(WorkspaceLease {
                workspace_id: workspace_id.to_string(),
                holder_id: holder_id.to_string(),
                acquired_at: now,
                expires_at,
                renewed_at: now,
            });
        }

        // Lease is active and held by another holder — fetch details for error message
        let (existing_holder, existing_expires_str): (String, String) = sqlx::query_as(
            "SELECT holder_id, expires_at FROM workspace_leases WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        let existing_expires: DateTime<Utc> = existing_expires_str
            .parse()
            .map_err(|e| LeaseError::Database(sqlx::Error::Protocol(format!("{}", e))))?;

        Err(LeaseError::AlreadyHeld {
            workspace_id: workspace_id.to_string(),
            holder_id: existing_holder,
            expires_at: existing_expires,
        })
    }

    async fn renew(
        &self,
        workspace_id: &str,
        holder_id: &str,
    ) -> Result<WorkspaceLease, LeaseError> {
        let now = Utc::now();
        let expires_at = self.compute_expires_at();

        let result = sqlx::query_as::<_, (String,)>(
            "SELECT holder_id FROM workspace_leases WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;

        let (current_holder,) =
            result.ok_or_else(|| LeaseError::NotFound(workspace_id.to_string()))?;

        if current_holder != holder_id {
            return Err(LeaseError::HolderMismatch {
                expected: holder_id.to_string(),
                actual: current_holder,
            });
        }

        sqlx::query(
            r#"
            UPDATE workspace_leases
            SET expires_at = ?, renewed_at = ?
            WHERE workspace_id = ? AND holder_id = ?
            "#,
        )
        .bind(expires_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(workspace_id)
        .bind(holder_id)
        .execute(&self.pool)
        .await?;

        // Fetch the updated lease
        let (acquired_str,): (String,) =
            sqlx::query_as("SELECT acquired_at FROM workspace_leases WHERE workspace_id = ?")
                .bind(workspace_id)
                .fetch_one(&self.pool)
                .await?;

        let acquired_at: DateTime<Utc> = acquired_str
            .parse()
            .map_err(|e| LeaseError::Database(sqlx::Error::Protocol(format!("{}", e))))?;

        Ok(WorkspaceLease {
            workspace_id: workspace_id.to_string(),
            holder_id: holder_id.to_string(),
            acquired_at,
            expires_at,
            renewed_at: now,
        })
    }

    async fn release(&self, workspace_id: &str, holder_id: &str) -> Result<(), LeaseError> {
        let result = sqlx::query_as::<_, (String,)>(
            "SELECT holder_id FROM workspace_leases WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;

        let (current_holder,) =
            result.ok_or_else(|| LeaseError::NotFound(workspace_id.to_string()))?;

        if current_holder != holder_id {
            return Err(LeaseError::HolderMismatch {
                expected: holder_id.to_string(),
                actual: current_holder,
            });
        }

        sqlx::query("DELETE FROM workspace_leases WHERE workspace_id = ? AND holder_id = ?")
            .bind(workspace_id)
            .bind(holder_id)
            .execute(&self.pool)
            .await?;

        debug!(
            "Lease released for workspace '{}' by '{}'",
            workspace_id, holder_id
        );

        Ok(())
    }

    async fn check(&self, workspace_id: &str) -> Result<Option<WorkspaceLease>, LeaseError> {
        let row = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT holder_id, acquired_at, expires_at, renewed_at FROM workspace_leases WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some((holder_id, acquired_str, expires_str, renewed_str)) = row else {
            return Ok(None);
        };

        let acquired_at: DateTime<Utc> = acquired_str
            .parse()
            .map_err(|e| LeaseError::Database(sqlx::Error::Protocol(format!("{}", e))))?;
        let expires_at: DateTime<Utc> = expires_str
            .parse()
            .map_err(|e| LeaseError::Database(sqlx::Error::Protocol(format!("{}", e))))?;
        let renewed_at: DateTime<Utc> = renewed_str
            .parse()
            .map_err(|e| LeaseError::Database(sqlx::Error::Protocol(format!("{}", e))))?;

        // Check if expired
        if expires_at < Utc::now() {
            return Ok(None);
        }

        Ok(Some(WorkspaceLease {
            workspace_id: workspace_id.to_string(),
            holder_id,
            acquired_at,
            expires_at,
            renewed_at,
        }))
    }
}

/// Background task that periodically renews leases held by this server instance.
///
/// Keeps a set of workspace_ids that should be renewed. When a workspace is
/// in active use, add it to the renewal set; when released, remove it.
/// Also updates the `workspace_lease_active` Prometheus gauge on each tick.
pub struct LeaseRenewalTask {
    lease_manager: Arc<dyn WorkspaceLeaseManager>,
    holder_id: String,
    /// Workspace IDs to keep renewed
    active_leases: Arc<RwLock<HashSet<String>>>,
    interval: Duration,
}

impl LeaseRenewalTask {
    pub fn new(lease_manager: Arc<dyn WorkspaceLeaseManager>, holder_id: String) -> Self {
        Self {
            lease_manager,
            holder_id,
            active_leases: Arc::new(RwLock::new(HashSet::new())),
            interval: Duration::from_secs(DEFAULT_RENEWAL_INTERVAL_SECS),
        }
    }

    /// Get a handle to the active leases set for adding/removing workspaces
    pub fn active_leases(&self) -> Arc<RwLock<HashSet<String>>> {
        self.active_leases.clone()
    }

    /// Start the background renewal loop
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(self.interval);
            loop {
                ticker.tick().await;

                let leases = self.active_leases.read().await.clone();

                // Update active lease count metric
                crate::infra::metrics::set_active_lease_count(leases.len() as u64);

                for workspace_id in &leases {
                    match self
                        .lease_manager
                        .renew(workspace_id, &self.holder_id)
                        .await
                    {
                        Ok(_) => {
                            debug!("Renewed lease for workspace '{}'", workspace_id);
                        }
                        Err(e) => {
                            warn!(
                                "Failed to renew lease for workspace '{}': {}",
                                workspace_id, e
                            );
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    /// Helper: manually expire a lease by setting expires_at to past time
    async fn expire_lease(pool: &SqlitePool, workspace_id: &str) {
        let past = (Utc::now() - chrono::Duration::seconds(10)).to_rfc3339();
        sqlx::query("UPDATE workspace_leases SET expires_at = ? WHERE workspace_id = ?")
            .bind(&past)
            .bind(workspace_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_acquire_and_check() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool).await.unwrap();

        // Acquire lease
        let lease = mgr.acquire("ws1", "server-1").await.unwrap();
        assert_eq!(lease.workspace_id, "ws1");
        assert_eq!(lease.holder_id, "server-1");

        // Check should return it
        let checked = mgr.check("ws1").await.unwrap();
        assert!(checked.is_some());
        assert_eq!(checked.unwrap().holder_id, "server-1");
    }

    #[tokio::test]
    async fn test_acquire_already_held() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool).await.unwrap();

        mgr.acquire("ws1", "server-1").await.unwrap();

        // Another holder should fail
        let err = mgr.acquire("ws1", "server-2").await.unwrap_err();
        assert!(matches!(err, LeaseError::AlreadyHeld { .. }));
    }

    #[tokio::test]
    async fn test_acquire_same_holder_re_acquire() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool).await.unwrap();

        mgr.acquire("ws1", "server-1").await.unwrap();

        // Same holder should succeed (re-acquire)
        let lease = mgr.acquire("ws1", "server-1").await.unwrap();
        assert_eq!(lease.holder_id, "server-1");
    }

    #[tokio::test]
    async fn test_acquire_expired_lease() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool.clone()).await.unwrap();

        mgr.acquire("ws1", "server-1").await.unwrap();

        // Manually expire the lease
        expire_lease(&pool, "ws1").await;

        // Another holder should now be able to acquire
        let lease = mgr.acquire("ws1", "server-2").await.unwrap();
        assert_eq!(lease.holder_id, "server-2");
    }

    #[tokio::test]
    async fn test_renew() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool).await.unwrap();

        mgr.acquire("ws1", "server-1").await.unwrap();

        let renewed = mgr.renew("ws1", "server-1").await.unwrap();
        assert_eq!(renewed.holder_id, "server-1");
        assert!(renewed.expires_at > Utc::now());
    }

    #[tokio::test]
    async fn test_renew_wrong_holder() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool).await.unwrap();

        mgr.acquire("ws1", "server-1").await.unwrap();

        let err = mgr.renew("ws1", "server-2").await.unwrap_err();
        assert!(matches!(err, LeaseError::HolderMismatch { .. }));
    }

    #[tokio::test]
    async fn test_release() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool).await.unwrap();

        mgr.acquire("ws1", "server-1").await.unwrap();
        mgr.release("ws1", "server-1").await.unwrap();

        // Check should return None
        let checked = mgr.check("ws1").await.unwrap();
        assert!(checked.is_none());
    }

    #[tokio::test]
    async fn test_release_wrong_holder() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool).await.unwrap();

        mgr.acquire("ws1", "server-1").await.unwrap();

        let err = mgr.release("ws1", "server-2").await.unwrap_err();
        assert!(matches!(err, LeaseError::HolderMismatch { .. }));
    }

    #[tokio::test]
    async fn test_check_nonexistent() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool).await.unwrap();

        let checked = mgr.check("ws_not_exist").await.unwrap();
        assert!(checked.is_none());
    }

    #[tokio::test]
    async fn test_check_expired_returns_none() {
        let pool = create_pool().await;
        let mgr = SqliteLeaseManager::new(pool.clone()).await.unwrap();

        mgr.acquire("ws1", "server-1").await.unwrap();

        // Manually expire the lease
        expire_lease(&pool, "ws1").await;

        let checked = mgr.check("ws1").await.unwrap();
        assert!(checked.is_none());
    }
}
