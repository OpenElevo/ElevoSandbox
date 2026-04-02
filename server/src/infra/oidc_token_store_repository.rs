//! OIDC token store repository

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::net::IpAddr;
use uuid::Uuid;

use crate::error::Result;
use sqlx::types::ipnetwork::IpNetwork;

/// OIDC token store entry
#[derive(Debug, Clone)]
pub struct OidcTokenStoreEntry {
    pub id: Uuid,
    pub local_session_id: Uuid,
    pub user_id: i64,
    pub org_id: Option<i64>,
    pub org_role: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub local_jwt: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub session_code: Option<String>,
    pub session_code_consumed: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub ip_address: Option<IpAddr>,
    pub last_refreshed_at: Option<DateTime<Utc>>,
}

/// Repository for OIDC token storage
#[derive(Clone)]
pub struct OidcTokenStoreRepository {
    pool: PgPool,
}

/// Parameters for creating a token store entry
pub struct CreateTokenStoreParams {
    pub local_session_id: Uuid,
    pub user_id: i64,
    pub org_id: Option<i64>,
    pub org_role: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub local_jwt: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub id_token: String,
    pub session_code: Option<String>,
    pub ip_address: Option<IpAddr>,
}

/// Map from domain IpAddr to ipnetwork::IpNetwork for sqlx INET compatibility
fn to_ip_network(ip: Option<IpAddr>) -> Option<IpNetwork> {
    ip.map(|ip| IpNetwork::from(ip))
}

/// Map from ipnetwork::IpNetwork to domain IpAddr
fn from_ip_network(network: Option<IpNetwork>) -> Option<IpAddr> {
    network.map(|n| n.ip())
}

impl OidcTokenStoreRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new token store entry
    pub async fn create(&self, params: CreateTokenStoreParams) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO oidc_token_store (local_session_id, user_id, org_id, org_role, email, name, picture,
                                          local_jwt, access_token, refresh_token, id_token,
                                          session_code, ip_address, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, now(), now() + INTERVAL '7 days')
            "#,
        )
        .bind(params.local_session_id)
        .bind(params.user_id)
        .bind(params.org_id)
        .bind(&params.org_role)
        .bind(&params.email)
        .bind(&params.name)
        .bind(&params.picture)
        .bind(&params.local_jwt)
        .bind(&params.access_token)
        .bind(&params.refresh_token)
        .bind(&params.id_token)
        .bind(&params.session_code)
        .bind(to_ip_network(params.ip_address))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Atomically consume a session_code (one-time use). Returns None if not found, expired, or already used.
    pub async fn consume_session_code(
        &self,
        code: &str,
    ) -> Result<Option<OidcTokenStoreEntry>> {
        let row = sqlx::query_as::<_, OidcTokenStoreRow>(
            r#"
            UPDATE oidc_token_store
            SET session_code_consumed = true
            WHERE session_code = $1 AND NOT session_code_consumed AND session_code_expires_at > now()
            RETURNING id, local_session_id, user_id, org_id, org_role, email, name, picture,
                      local_jwt, access_token, refresh_token, id_token,
                      session_code, session_code_consumed, created_at, expires_at,
                      ip_address, last_refreshed_at
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(row_to_entry))
    }

    /// Find a token store entry by local_session_id
    pub async fn find_by_session_id(
        &self,
        local_session_id: Uuid,
    ) -> Result<Option<OidcTokenStoreEntry>> {
        let row = sqlx::query_as::<_, OidcTokenStoreRow>(
            r#"
            SELECT id, local_session_id, user_id, org_id, org_role, email, name, picture,
                   local_jwt, access_token, refresh_token, id_token,
                   session_code, session_code_consumed, created_at, expires_at,
                   ip_address, last_refreshed_at
            FROM oidc_token_store
            WHERE local_session_id = $1 AND expires_at > now()
            "#,
        )
        .bind(local_session_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(row_to_entry))
    }

    /// Update tokens for an existing entry
    pub async fn update_tokens(
        &self,
        id: Uuid,
        access_token: &str,
        refresh_token: Option<&str>,
        id_token: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE oidc_token_store
            SET access_token = $2, refresh_token = $3, id_token = $4, last_refreshed_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(access_token)
        .bind(refresh_token)
        .bind(id_token)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Delete token store entry by local_session_id
    pub async fn delete_by_session_id(&self, local_session_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM oidc_token_store WHERE local_session_id = $1")
            .bind(local_session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Clean up expired entries. Returns the number of deleted rows.
    pub async fn cleanup_expired(&self) -> Result<u64> {
        let result =
            sqlx::query("DELETE FROM oidc_token_store WHERE expires_at < now()")
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }
}

fn row_to_entry(r: OidcTokenStoreRow) -> OidcTokenStoreEntry {
    OidcTokenStoreEntry {
        id: r.id,
        local_session_id: r.local_session_id,
        user_id: r.user_id,
        org_id: r.org_id,
        org_role: r.org_role,
        email: r.email,
        name: r.name,
        picture: r.picture,
        local_jwt: r.local_jwt,
        access_token: r.access_token,
        refresh_token: r.refresh_token,
        id_token: r.id_token,
        session_code: r.session_code,
        session_code_consumed: r.session_code_consumed,
        created_at: r.created_at,
        expires_at: r.expires_at,
        ip_address: from_ip_network(r.ip_address),
        last_refreshed_at: r.last_refreshed_at,
    }
}

#[derive(Debug, sqlx::FromRow)]
struct OidcTokenStoreRow {
    id: Uuid,
    local_session_id: Uuid,
    user_id: i64,
    org_id: Option<i64>,
    org_role: Option<String>,
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
    local_jwt: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    session_code: Option<String>,
    session_code_consumed: bool,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    ip_address: Option<IpNetwork>,
    last_refreshed_at: Option<DateTime<Utc>>,
}
