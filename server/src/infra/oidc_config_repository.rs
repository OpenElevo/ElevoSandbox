//! OIDC configuration repository

use sqlx::PgPool;

use crate::error::Result;

/// Repository for OIDC configuration CRUD
#[derive(Clone)]
pub struct OidcConfigRepository {
    pool: PgPool,
}

/// Parameters for upserting OIDC config
pub struct UpsertOidcConfigParams {
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret_encrypted: String,
    pub redirect_uri: String,
    pub jwks_refresh_interval_secs: i32,
    pub disable_password_login: bool,
    pub auto_create_tenant: bool,
}

impl OidcConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Check if OIDC config exists
    pub async fn exists(&self) -> Result<bool> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM oidc_config WHERE id = 1)")
                .fetch_one(&self.pool)
                .await?;
        Ok(exists)
    }

    /// Upsert OIDC configuration (INSERT ... ON CONFLICT DO UPDATE)
    pub async fn upsert_config(&self, params: UpsertOidcConfigParams) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO oidc_config (id, enabled, issuer_url, client_id, client_secret_encrypted, redirect_uri,
                                     jwks_refresh_interval_secs, disable_password_login, auto_create_tenant, updated_at)
            VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, now())
            ON CONFLICT (id) DO UPDATE SET
                enabled = EXCLUDED.enabled,
                issuer_url = EXCLUDED.issuer_url,
                client_id = EXCLUDED.client_id,
                client_secret_encrypted = EXCLUDED.client_secret_encrypted,
                redirect_uri = EXCLUDED.redirect_uri,
                jwks_refresh_interval_secs = EXCLUDED.jwks_refresh_interval_secs,
                disable_password_login = EXCLUDED.disable_password_login,
                auto_create_tenant = EXCLUDED.auto_create_tenant,
                updated_at = now()
            "#,
        )
        .bind(params.enabled)
        .bind(&params.issuer_url)
        .bind(&params.client_id)
        .bind(&params.client_secret_encrypted)
        .bind(&params.redirect_uri)
        .bind(params.jwks_refresh_interval_secs)
        .bind(params.disable_password_login)
        .bind(params.auto_create_tenant)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get the raw (encrypted) client_secret from DB
    pub async fn get_client_secret_encrypted(&self) -> Result<Option<String>> {
        let val: Option<String> =
            sqlx::query_scalar("SELECT client_secret_encrypted FROM oidc_config WHERE id = 1")
                .fetch_optional(&self.pool)
                .await?;
        Ok(val)
    }
}
