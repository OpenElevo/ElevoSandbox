//! OIDC authorization session repository

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::net::IpAddr;
use uuid::Uuid;

use crate::error::Result;
use sqlx::types::ipnetwork::IpNetwork;

/// OIDC authorization session domain type
#[derive(Debug, Clone)]
pub struct OidcAuthSession {
    pub id: Uuid,
    pub state: String,
    pub nonce: String,
    pub code_verifier: String,
    pub consumed: bool,
    pub ip_address: Option<IpAddr>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Repository for OIDC authorization sessions
#[derive(Clone)]
pub struct OidcAuthSessionRepository {
    pool: PgPool,
}

/// Parameters for creating a new auth session
pub struct CreateAuthSessionParams {
    pub state: String,
    pub nonce: String,
    pub code_verifier: String,
    pub ip_address: Option<IpAddr>,
}

impl OidcAuthSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new authorization session
    pub async fn create(&self, params: CreateAuthSessionParams) -> Result<()> {
        // Convert IpAddr to ipnetwork::IpNetwork for sqlx INET compatibility
        let ip_network: Option<IpNetwork> = params.ip_address.map(|ip| {
            IpNetwork::from(ip)
        });

        sqlx::query(
            r#"
            INSERT INTO oidc_auth_sessions (state, nonce, code_verifier, ip_address, created_at, expires_at)
            VALUES ($1, $2, $3, $4, now(), now() + INTERVAL '10 minutes')
            "#,
        )
        .bind(&params.state)
        .bind(&params.nonce)
        .bind(&params.code_verifier)
        .bind(ip_network)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Atomically consume a session by state.
    /// Returns None if not found, already consumed, or expired.
    pub async fn consume_by_state(&self, state: &str) -> Result<Option<OidcAuthSession>> {
        let row = sqlx::query_as::<_, OidcAuthSessionRow>(
            r#"
            UPDATE oidc_auth_sessions
            SET consumed = true
            WHERE state = $1 AND NOT consumed AND expires_at > now()
            RETURNING id, state, nonce, code_verifier, consumed, ip_address, created_at, expires_at
            "#,
        )
        .bind(state)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| OidcAuthSession {
            id: r.id,
            state: r.state,
            nonce: r.nonce,
            code_verifier: r.code_verifier,
            consumed: r.consumed,
            ip_address: r.ip_address.map(|n: IpNetwork| n.ip()),
            created_at: r.created_at,
            expires_at: r.expires_at,
        }))
    }

    /// Clean up expired sessions. Returns the number of deleted rows.
    pub async fn cleanup_expired(&self) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM oidc_auth_sessions WHERE expires_at < now() OR consumed = true",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

#[derive(Debug, sqlx::FromRow)]
struct OidcAuthSessionRow {
    id: Uuid,
    state: String,
    nonce: String,
    code_verifier: String,
    consumed: bool,
    ip_address: Option<IpNetwork>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}
