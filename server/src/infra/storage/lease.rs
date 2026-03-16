//! Workspace lease-based concurrency control
//!
//! Provides single-instance workspace locking via PostgreSQL. When a server instance
//! acquires a lease on a workspace, no other server instance may modify it until
//! the lease expires or is released.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
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

    #[error("invalid workspace ID '{0}': expected UUID format")]
    InvalidWorkspaceId(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}

/// PostgreSQL-based workspace lease manager
pub struct PgLeaseManager {
    pool: PgPool,
    lease_duration: Duration,
}

impl PgLeaseManager {
    /// Create a new PostgreSQL lease manager.
    /// Table is created by migration, no runtime DDL needed.
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            lease_duration: Duration::from_secs(DEFAULT_LEASE_DURATION_SECS as u64),
        }
    }

    /// Compute the expiration time from now based on configured lease duration
    fn compute_expires_at(&self) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::seconds(self.lease_duration.as_secs() as i64)
    }
}

#[async_trait]
impl WorkspaceLeaseManager for PgLeaseManager {
    async fn acquire(
        &self,
        workspace_id: &str,
        holder_id: &str,
    ) -> Result<WorkspaceLease, LeaseError> {
        let namespace_id = uuid::Uuid::parse_str(workspace_id)
            .map_err(|_| LeaseError::InvalidWorkspaceId(workspace_id.to_string()))?;
        let now = Utc::now();
        let expires_at = self.compute_expires_at();

        let mut tx = self.pool.begin().await?;

        // Try to insert a new lease (ignored if workspace_id already exists)
        let insert_result = sqlx::query(
            r#"
            INSERT INTO namespace_leases
                (namespace_id, holder_id, acquired_at, expires_at, renewed_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (namespace_id) DO NOTHING
            "#,
        )
        .bind(namespace_id)
        .bind(holder_id)
        .bind(now)
        .bind(expires_at)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        if insert_result.rows_affected() == 1 {
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
            UPDATE namespace_leases
            SET holder_id = $1, acquired_at = $2, expires_at = $3, renewed_at = $4
            WHERE namespace_id = $5
              AND (holder_id = $6 OR expires_at < $7)
            "#,
        )
        .bind(holder_id)
        .bind(now)
        .bind(expires_at)
        .bind(now)
        .bind(namespace_id)
        .bind(holder_id)
        .bind(now)
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

        // Lease is active and held by another holder
        let (existing_holder, existing_expires): (String, DateTime<Utc>) = sqlx::query_as(
            "SELECT holder_id, expires_at FROM namespace_leases WHERE namespace_id = $1",
        )
        .bind(namespace_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

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
        let namespace_id = uuid::Uuid::parse_str(workspace_id)
            .map_err(|_| LeaseError::InvalidWorkspaceId(workspace_id.to_string()))?;
        let now = Utc::now();
        let expires_at = self.compute_expires_at();

        let result = sqlx::query_as::<_, (String,)>(
            "SELECT holder_id FROM namespace_leases WHERE namespace_id = $1",
        )
        .bind(namespace_id)
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
            UPDATE namespace_leases
            SET expires_at = $1, renewed_at = $2
            WHERE namespace_id = $3 AND holder_id = $4
            "#,
        )
        .bind(expires_at)
        .bind(now)
        .bind(namespace_id)
        .bind(holder_id)
        .execute(&self.pool)
        .await?;

        let (acquired_at,): (DateTime<Utc>,) =
            sqlx::query_as("SELECT acquired_at FROM namespace_leases WHERE namespace_id = $1")
                .bind(namespace_id)
                .fetch_one(&self.pool)
                .await?;

        Ok(WorkspaceLease {
            workspace_id: workspace_id.to_string(),
            holder_id: holder_id.to_string(),
            acquired_at,
            expires_at,
            renewed_at: now,
        })
    }

    async fn release(&self, workspace_id: &str, holder_id: &str) -> Result<(), LeaseError> {
        let namespace_id = uuid::Uuid::parse_str(workspace_id)
            .map_err(|_| LeaseError::InvalidWorkspaceId(workspace_id.to_string()))?;

        let result = sqlx::query_as::<_, (String,)>(
            "SELECT holder_id FROM namespace_leases WHERE namespace_id = $1",
        )
        .bind(namespace_id)
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

        sqlx::query("DELETE FROM namespace_leases WHERE namespace_id = $1 AND holder_id = $2")
            .bind(namespace_id)
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
        let namespace_id = uuid::Uuid::parse_str(workspace_id)
            .map_err(|_| LeaseError::InvalidWorkspaceId(workspace_id.to_string()))?;

        let row = sqlx::query_as::<_, (String, DateTime<Utc>, DateTime<Utc>, DateTime<Utc>)>(
            "SELECT holder_id, acquired_at, expires_at, renewed_at FROM namespace_leases WHERE namespace_id = $1",
        )
        .bind(namespace_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some((holder_id, acquired_at, expires_at, renewed_at)) = row else {
            return Ok(None);
        };

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
    use crate::domain::UuidSimple;
    use uuid::Uuid;

    /// Helper: create a test tenant and return its UUID as namespace_id
    async fn create_test_tenant(pool: &PgPool) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO tenants (id, name, description, is_active)
            VALUES ($1, $2, $3, $4)"#,
        )
        .bind(id)
        .bind(format!("test-tenant-{}", id))
        .bind(format!("Test tenant for {}", id))
        .bind(true)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    /// Helper: manually expire a lease by setting expires_at to past time
    async fn expire_lease(pool: &PgPool, namespace_id: Uuid) {
        let past = Utc::now() - chrono::Duration::seconds(10);
        sqlx::query("UPDATE namespace_leases SET expires_at = $1 WHERE namespace_id = $2")
            .bind(past)
            .bind(namespace_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_acquire_and_check(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let mgr = PgLeaseManager::new(pool);
        let namespace_id_str = namespace_id.simple_string();

        let lease = mgr.acquire(&namespace_id_str, "server-1").await.unwrap();
        assert_eq!(lease.workspace_id, namespace_id_str);
        assert_eq!(lease.holder_id, "server-1");

        let checked = mgr.check(&namespace_id_str).await.unwrap();
        assert!(checked.is_some());
        assert_eq!(checked.unwrap().holder_id, "server-1");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_acquire_already_held(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let mgr = PgLeaseManager::new(pool);
        let namespace_id_str = namespace_id.simple_string();

        mgr.acquire(&namespace_id_str, "server-1").await.unwrap();

        let err = mgr
            .acquire(&namespace_id_str, "server-2")
            .await
            .unwrap_err();
        assert!(matches!(err, LeaseError::AlreadyHeld { .. }));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_acquire_same_holder_re_acquire(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let mgr = PgLeaseManager::new(pool);
        let namespace_id_str = namespace_id.simple_string();

        mgr.acquire(&namespace_id_str, "server-1").await.unwrap();

        let lease = mgr.acquire(&namespace_id_str, "server-1").await.unwrap();
        assert_eq!(lease.holder_id, "server-1");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_acquire_expired_lease(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let mgr = PgLeaseManager::new(pool.clone());
        let namespace_id_str = namespace_id.simple_string();

        mgr.acquire(&namespace_id_str, "server-1").await.unwrap();
        expire_lease(&pool, namespace_id).await;

        let lease = mgr.acquire(&namespace_id_str, "server-2").await.unwrap();
        assert_eq!(lease.holder_id, "server-2");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_renew(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let mgr = PgLeaseManager::new(pool);
        let namespace_id_str = namespace_id.simple_string();

        mgr.acquire(&namespace_id_str, "server-1").await.unwrap();

        let renewed = mgr.renew(&namespace_id_str, "server-1").await.unwrap();
        assert_eq!(renewed.holder_id, "server-1");
        assert!(renewed.expires_at > Utc::now());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_renew_wrong_holder(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let mgr = PgLeaseManager::new(pool);
        let namespace_id_str = namespace_id.simple_string();

        mgr.acquire(&namespace_id_str, "server-1").await.unwrap();

        let err = mgr.renew(&namespace_id_str, "server-2").await.unwrap_err();
        assert!(matches!(err, LeaseError::HolderMismatch { .. }));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_release(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let mgr = PgLeaseManager::new(pool);
        let namespace_id_str = namespace_id.simple_string();

        mgr.acquire(&namespace_id_str, "server-1").await.unwrap();
        mgr.release(&namespace_id_str, "server-1").await.unwrap();

        let checked = mgr.check(&namespace_id_str).await.unwrap();
        assert!(checked.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_release_wrong_holder(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let mgr = PgLeaseManager::new(pool);
        let namespace_id_str = namespace_id.simple_string();

        mgr.acquire(&namespace_id_str, "server-1").await.unwrap();

        let err = mgr
            .release(&namespace_id_str, "server-2")
            .await
            .unwrap_err();
        assert!(matches!(err, LeaseError::HolderMismatch { .. }));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_check_nonexistent(pool: PgPool) {
        let mgr = PgLeaseManager::new(pool);
        // Use a valid UUID that doesn't exist in tenants table
        // This will return None since no lease exists
        let nonexistent_uuid = Uuid::now_v7();
        let checked = mgr.check(&nonexistent_uuid.simple_string()).await.unwrap();
        assert!(checked.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_check_expired_returns_none(pool: PgPool) {
        let namespace_id = create_test_tenant(&pool).await;
        let mgr = PgLeaseManager::new(pool.clone());
        let namespace_id_str = namespace_id.simple_string();

        mgr.acquire(&namespace_id_str, "server-1").await.unwrap();
        expire_lease(&pool, namespace_id).await;

        let checked = mgr.check(&namespace_id_str).await.unwrap();
        assert!(checked.is_none());
    }
}
