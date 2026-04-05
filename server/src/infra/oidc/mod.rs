//! OIDC Service — core OIDC/SSO integration with ElevoOne
//!
//! This module provides the main OidcService struct and its methods for:
//! - Token verification (RS256 + JWKS)
//! - Authorization code exchange
//! - ID token verification
//! - Token refresh
//! - Connection testing
//! - URL generation (authorize, end_session)

pub mod circuit_breaker;
pub mod crypto;
pub mod jwt_utils;
pub mod jwks;
pub mod pkce;
pub mod types;

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tracing::{debug, info, warn};

use crate::infra::oidc::circuit_breaker::OidcCircuitBreaker;
use crate::infra::oidc::crypto::decrypt_client_secret;
use crate::infra::oidc::jwks::{self as jwks_module, JwksKeySet};
use crate::infra::oidc::types::{ElevoOneClaims, OidcConfig, OidcError, TokenResponse};
use crate::domain::auth::Identity;
use crate::domain::tenant::CreateTenantParams;
use crate::infra::tenant_repository::TenantRepository;

/// HTTP client reuse for OIDC requests
fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build OIDC HTTP client")
}

/// The main OIDC service
pub struct OidcService {
    /// Cached OIDC configuration
    pub config: Arc<tokio::sync::RwLock<OidcConfig>>,
    /// Cached JWKS key set
    jwks: Arc<tokio::sync::RwLock<JwksKeySet>>,
    /// Last JWKS refresh timestamp (unix seconds)
    jwks_last_refresh: AtomicI64,
    /// JWKS refresh interval in seconds
    refresh_interval: AtomicI64,
    /// Circuit breaker
    circuit_breaker: Arc<OidcCircuitBreaker>,
    /// Database pool
    pool: PgPool,
    /// Encryption key for client_secret
    encryption_key: [u8; 32],
    /// HTTP client
    http_client: reqwest::Client,
    /// Workspace root directory (for storage availability check during auto-create)
    workspace_dir: PathBuf,
}

impl OidcService {
    /// Load OIDC configuration from DB and create the service.
    /// Returns None if OIDC is not configured (no config row or not enabled).
    pub async fn new_from_db(
        pool: PgPool,
        encryption_key: [u8; 32],
        workspace_dir: PathBuf,
    ) -> Result<Option<Arc<Self>>, OidcError> {
        let config = Self::load_config_from_db(&pool, &encryption_key).await?;

        let enabled = config.enabled;
        let refresh_interval = config.jwks_refresh_interval_secs as i64;

        let service = Self {
            config: Arc::new(tokio::sync::RwLock::new(config)),
            jwks: Arc::new(tokio::sync::RwLock::new(JwksKeySet::default())),
            jwks_last_refresh: AtomicI64::new(0),
            refresh_interval: AtomicI64::new(refresh_interval),
            circuit_breaker: Arc::new(OidcCircuitBreaker::new()),
            pool,
            encryption_key,
            http_client: build_http_client(),
            workspace_dir,
        };

        if enabled {
            info!("OIDC service initialized (enabled)");
            // Do initial JWKS fetch
            if let Err(e) = service.refresh_jwks().await {
                warn!("Initial JWKS fetch failed (non-fatal): {}", e);
            }
            Ok(Some(Arc::new(service)))
        } else {
            info!("OIDC service initialized (disabled)");
            Ok(None)
        }
    }

    /// Reload configuration from DB (called after config update)
    pub async fn reload_config(&self) -> Result<(), OidcError> {
        let config = Self::load_config_from_db(&self.pool, &self.encryption_key).await?;
        self.refresh_interval
            .store(config.jwks_refresh_interval_secs as i64, Ordering::Release);
        {
            let mut guard = self.config.write().await;
            *guard = config;
        }
        info!("OIDC configuration reloaded");
        // Refresh JWKS with new issuer
        if let Err(e) = self.refresh_jwks().await {
            warn!("JWKS refresh after config reload failed: {}", e);
        }
        Ok(())
    }

    /// Verify an ElevoOne RS256 token and resolve the tenant identity.
    /// Used by both HTTP and gRPC auth middleware.
    pub async fn verify_and_resolve_tenant(
        &self,
        token: &str,
        tenant_repo: &TenantRepository,
    ) -> Result<Identity, OidcError> {
        if self.circuit_breaker.is_tripped() {
            return Err(OidcError::CircuitBreakerTripped);
        }

        let config = self.config.read().await;
        if !config.enabled {
            return Err(OidcError::Disabled);
        }

        let claims = {
            // Try verifying with cached JWKS, force refresh if kid not found
            let jwks_guard = self.jwks.read().await;
            match jwks_module::verify_access_token(
                token,
                &jwks_guard,
                &config.issuer_url,
                &config.client_id,
            ) {
                Ok(claims) => claims,
                Err(e) => {
                    // Check if it's a "kid not found" error — try refreshing JWKS
                    drop(jwks_guard);
                    match self.force_refresh_jwks_if_needed(token, &config).await {
                        Ok(claims) => claims,
                        Err(_) => {
                            self.circuit_breaker.record_failure();
                            return Err(e);
                        }
                    }
                }
            }
        };
        self.circuit_breaker.record_success();

        // Resolve tenant from org_id
        if let Some(org_id) = claims.oid {
            match tenant_repo.find_by_elevoone_org_id(org_id).await {
                Ok(Some(tenant)) => {
                    if !tenant.is_active {
                        return Err(OidcError::TenantNotFound(org_id));
                    }
                    return Ok(Identity::Tenant {
                        id: tenant.id,
                        name: tenant.name,
                    });
                }
                Ok(None) => {
                    // Auto-create tenant if enabled
                    if config.auto_create_tenant {
                        return self
                            .auto_create_tenant(tenant_repo, org_id, &claims)
                            .await;
                    }
                    return Err(OidcError::TenantNotFound(org_id));
                }
                Err(e) => {
                    warn!("DB error looking up tenant by org_id {}: {}", org_id, e);
                    return Err(OidcError::Internal(format!(
                        "tenant lookup failed: {}",
                        e
                    )));
                }
            }
        }

        // No org_id — cannot map to tenant
        Err(OidcError::TenantNotFound(0))
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<TokenResponse, OidcError> {
        let config = self.config.read().await;
        if !config.enabled {
            return Err(OidcError::Disabled);
        }

        let token_url = if config.issuer_url.ends_with('/') {
            format!("{}oauth/token", config.issuer_url)
        } else {
            format!("{}/oauth/token", config.issuer_url)
        };

        let params: Vec<(&str, &str)> = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &config.redirect_uri),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
            ("code_verifier", code_verifier),
        ];

        let response = self
            .http_client
            .post(&token_url)
            .form(&params)
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| {
                self.circuit_breaker.record_failure();
                OidcError::TokenExchangeFailed(format!("request: {}", e))
            })?
            .error_for_status()
            .map_err(|e| {
                self.circuit_breaker.record_failure();
                OidcError::TokenExchangeFailed(format!("response: {}", e))
            })?;

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| OidcError::TokenExchangeFailed(format!("parse: {}", e)))?;

        self.circuit_breaker.record_success();
        Ok(token_response)
    }

    /// Verify an ID token (RS256 + claims validation)
    pub async fn verify_id_token(
        &self,
        id_token: &str,
        nonce: &str,
    ) -> Result<ElevoOneClaims, OidcError> {
        let config = self.config.read().await;

        // Verify RS256 signature via JWKS
        let claims = {
            let jwks_guard = self.jwks.read().await;
            match jwks_module::verify_access_token(
                id_token,
                &jwks_guard,
                &config.issuer_url,
                &config.client_id,
            ) {
                Ok(c) => c,
                Err(e) => {
                    drop(jwks_guard);
                    match self.force_refresh_jwks_if_needed(id_token, &config).await {
                        Ok(c) => c,
                        Err(_) => return Err(OidcError::IdTokenVerificationFailed(
                            e.to_string(),
                        )),
                    }
                }
            }
        };

        // Verify nonce
        match &claims.nonce {
            Some(n) if n == nonce => {}
            _ => {
                return Err(OidcError::IdTokenVerificationFailed(
                    "nonce mismatch".to_string(),
                ));
            }
        }

        Ok(claims)
    }

    /// Refresh ElevoOne tokens
    pub async fn refresh_elevoone_token(
        &self,
        refresh_token: &str,
    ) -> Result<TokenResponse, OidcError> {
        let config = self.config.read().await;
        let token_url = if config.issuer_url.ends_with('/') {
            format!("{}oauth/token", config.issuer_url)
        } else {
            format!("{}/oauth/token", config.issuer_url)
        };

        let params: Vec<(&str, &str)> = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
        ];

        let response = self
            .http_client
            .post(&token_url)
            .form(&params)
            .timeout(Duration::from_secs(15))
            .send()
            .await
            .map_err(|e| OidcError::TokenExchangeFailed(format!("request: {}", e)))?
            .error_for_status()
            .map_err(|e| OidcError::TokenExchangeFailed(format!("response: {}", e)))?;

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| OidcError::TokenExchangeFailed(format!("parse: {}", e)))?;

        Ok(token_response)
    }

    /// Test connection to the OIDC issuer
    pub async fn test_connection(&self) -> Result<(), OidcError> {
        let config = self.config.read().await;

        // Fetch the well-known OpenID configuration as a connectivity test
        let discovery_uri = if config.issuer_url.ends_with('/') {
            format!("{}.well-known/openid-configuration", config.issuer_url)
        } else {
            format!("{}/.well-known/openid-configuration", config.issuer_url)
        };

        self.http_client
            .get(&discovery_uri)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| OidcError::ConnectionTestFailed(format!("network: {}", e)))?
            .error_for_status()
            .map_err(|e| OidcError::ConnectionTestFailed(format!("HTTP {}", e.status().map_or_else(|| "unknown".to_string(), |s| s.to_string()))))?;

        Ok(())
    }

    /// Build the end_session (logout) URL for the IdP
    pub async fn build_end_session_url(&self, id_token: &str) -> Result<String, OidcError> {
        let config = self.config.read().await;

        let end_session_url = if config.issuer_url.ends_with('/') {
            format!("{}oauth/end_session", config.issuer_url)
        } else {
            format!("{}/oauth/end_session", config.issuer_url)
        };

        // Derive post_logout_redirect_uri from redirect_uri's origin + /admin/login
        let post_logout_redirect_uri = url::Url::parse(&config.redirect_uri)
            .ok()
            .map(|u| {
                let mut redirect = url::Url::parse(&format!("{}://{}", u.scheme(), u.host_str().unwrap_or(""))).unwrap();
                redirect.set_path("/admin/login");
                redirect.to_string()
            })
            .unwrap_or_else(|| "/admin/login".to_string());

        Ok(format!(
            "{}?id_token_hint={}&client_id={}&post_logout_redirect_uri={}",
            end_session_url,
            urlencoding::encode(id_token),
            urlencoding::encode(&config.client_id),
            urlencoding::encode(&post_logout_redirect_uri),
        ))
    }

    /// Generate the authorization URL with PKCE.
    /// If `redirect_uri_override` is Some, it overrides the config value (useful
    /// when the admin left redirect_uri empty and we derive it from the request).
    pub async fn generate_authorize_url(
        &self,
        state: &str,
        nonce: &str,
        code_challenge: &str,
        redirect_uri_override: Option<&str>,
    ) -> Result<String, OidcError> {
        let config = self.config.read().await;

        let authorize_url = if config.issuer_url.ends_with('/') {
            format!("{}oauth/authorize", config.issuer_url)
        } else {
            format!("{}/oauth/authorize", config.issuer_url)
        };

        let redirect_uri = redirect_uri_override
            .filter(|s| !s.is_empty())
            .unwrap_or(&config.redirect_uri);

        Ok(format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope=openid+profile+email&state={}&nonce={}&code_challenge={}&code_challenge_method=S256",
            authorize_url,
            urlencoding::encode(&config.client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(state),
            urlencoding::encode(nonce),
            urlencoding::encode(code_challenge),
        ))
    }

    /// Get the public config (enabled, disable_password_login, circuit breaker status)
    pub async fn get_public_config(&self) -> (bool, bool) {
        let config = self.config.read().await;
        let mut disable_password = config.disable_password_login;

        // If circuit breaker is tripped, force password login available
        if self.circuit_breaker.is_tripped() && disable_password {
            warn!("OIDC circuit breaker tripped — re-enabling password login as fallback");
            disable_password = false;
        }

        (config.enabled, disable_password)
    }

    /// Get the full config (for admin display, client_secret masked)
    pub async fn get_full_config(&self) -> OidcConfig {
        self.config.read().await.clone()
    }

    /// Check if OIDC is enabled
    pub async fn is_enabled(&self) -> bool {
        self.config.read().await.enabled
    }

    /// Get auto_create_tenant setting
    pub async fn auto_create_tenant_enabled(&self) -> bool {
        self.config.read().await.auto_create_tenant
    }

    /// Get a reference to the circuit breaker
    pub fn circuit_breaker(&self) -> &OidcCircuitBreaker {
        &self.circuit_breaker
    }

    /// Auto-create a tenant from ElevoOne claims when org_id has no mapping.
    /// Uses elevoone_org_id unique index for concurrency protection.
    async fn auto_create_tenant(
        &self,
        tenant_repo: &TenantRepository,
        org_id: i64,
        claims: &ElevoOneClaims,
    ) -> Result<Identity, OidcError> {
        // Verify storage backend is accessible before creating tenant
        match tokio::fs::metadata(&self.workspace_dir).await {
            Ok(_) => {}
            Err(e) => {
                warn!(
                    "Storage backend not accessible at {:?}: {} — refusing to auto-create tenant",
                    self.workspace_dir, e
                );
                return Err(OidcError::Internal(format!(
                    "storage backend unavailable: {}",
                    e
                )));
            }
        }

        // Derive tenant name from email prefix or org_id
        let tenant_name = claims
            .email
            .as_ref()
            .and_then(|e| e.split('@').next())
            .filter(|prefix| !prefix.is_empty())
            .map(|prefix| format!("{}-org", prefix))
            .unwrap_or_else(|| format!("org-{}", org_id));

        let params = CreateTenantParams {
            name: tenant_name,
            description: Some(format!(
                "Auto-created from ElevoOne org_id {}",
                org_id
            )),
            elevoone_org_id: Some(org_id),
            ..Default::default()
        };

        match tenant_repo.create_tenant(params).await {
            Ok((tenant, _api_key)) => {
                info!(
                    "Auto-created tenant {} (id={}) for ElevoOne org_id={}",
                    tenant.name, tenant.id, org_id
                );
                Ok(Identity::Tenant {
                    id: tenant.id,
                    name: tenant.name,
                })
            }
            // Concurrent insert with same org_id — select the existing one
            Err(e) => {
                warn!(
                    "Auto-create tenant for org_id {} failed ({}), trying to find existing",
                    org_id, e
                );
                match tenant_repo.find_by_elevoone_org_id(org_id).await {
                    Ok(Some(tenant)) if tenant.is_active => Ok(Identity::Tenant {
                        id: tenant.id,
                        name: tenant.name,
                    }),
                    Ok(_) => Err(OidcError::TenantNotFound(org_id)),
                    Err(_) => Err(OidcError::Internal(format!(
                        "auto-create tenant failed: {}",
                        e
                    ))),
                }
            }
        }
    }

    // ── Internal methods ──

    /// Load and decrypt OIDC config from DB
    async fn load_config_from_db(
        pool: &PgPool,
        encryption_key: &[u8; 32],
    ) -> Result<OidcConfig, OidcError> {
        let row = sqlx::query_as::<_, OidcConfigRow>(
            "SELECT enabled, issuer_url, client_id, client_secret_encrypted, redirect_uri, \
             jwks_refresh_interval_secs, disable_password_login, auto_create_tenant \
             FROM oidc_config WHERE id = 1",
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| OidcError::Internal(format!("DB error: {}", e)))?;

        match row {
            Some(r) => {
                let client_secret = if r.client_secret_encrypted.is_empty() {
                    String::new()
                } else {
                    decrypt_client_secret(&r.client_secret_encrypted, encryption_key)?
                };
                Ok(OidcConfig {
                    enabled: r.enabled,
                    issuer_url: r.issuer_url,
                    client_id: r.client_id,
                    client_secret,
                    redirect_uri: r.redirect_uri,
                    jwks_refresh_interval_secs: r.jwks_refresh_interval_secs as u64,
                    disable_password_login: r.disable_password_login,
                    auto_create_tenant: r.auto_create_tenant,
                })
            }
            None => Ok(OidcConfig {
                enabled: false,
                issuer_url: String::new(),
                client_id: String::new(),
                client_secret: String::new(),
                redirect_uri: String::new(),
                jwks_refresh_interval_secs: 3600,
                disable_password_login: false,
                auto_create_tenant: false,
            }),
        }
    }

    /// Refresh JWKS if the refresh interval has elapsed
    pub async fn refresh_jwks(&self) -> Result<(), OidcError> {
        let config = self.config.read().await;
        let issuer = config.issuer_url.clone();

        // Check if refresh is needed
        let now = chrono::Utc::now().timestamp();
        let last_refresh = self.jwks_last_refresh.load(Ordering::Acquire);
        let interval = self.refresh_interval.load(Ordering::Acquire);
        if now - last_refresh < interval / 2 {
            return Ok(());
        }

        match jwks_module::fetch_jwks(&self.http_client, &issuer).await {
            Ok(key_set) => {
                {
                    let mut guard = self.jwks.write().await;
                    *guard = key_set;
                }
                self.jwks_last_refresh.store(now, Ordering::Release);
                debug!("JWKS refreshed successfully");
                Ok(())
            }
            Err(e) => {
                warn!("JWKS refresh failed: {}", e);
                Err(e)
            }
        }
    }

    /// Force refresh JWKS when a kid is not found (with backoff)
    async fn force_refresh_jwks_if_needed(
        &self,
        token: &str,
        config: &OidcConfig,
    ) -> Result<ElevoOneClaims, OidcError> {
        // Try refreshing JWKS
        match jwks_module::fetch_jwks(&self.http_client, &config.issuer_url).await {
            Ok(key_set) => {
                let now = chrono::Utc::now().timestamp();
                {
                    let mut guard = self.jwks.write().await;
                    *guard = key_set;
                }
                self.jwks_last_refresh.store(now, Ordering::Release);
                debug!("JWKS force-refreshed successfully");

                // Retry verification with new keys
                let jwks_guard = self.jwks.read().await;
                match jwks_module::verify_access_token(
                    token,
                    &jwks_guard,
                    &config.issuer_url,
                    &config.client_id,
                ) {
                    Ok(claims) => Ok(claims),
                    Err(first_err) => {
                        // Key rotation may not have propagated yet — wait and retry once
                        debug!(
                            "JWKS refresh succeeded but verification failed ({}), \
                             retrying after 100ms backoff",
                            first_err
                        );
                        drop(jwks_guard);
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                        // Re-fetch JWKS in case keys changed between our first fetch and now
                        match jwks_module::fetch_jwks(&self.http_client, &config.issuer_url).await {
                            Ok(key_set2) => {
                                let now2 = chrono::Utc::now().timestamp();
                                {
                                    let mut guard = self.jwks.write().await;
                                    *guard = key_set2;
                                }
                                self.jwks_last_refresh.store(now2, Ordering::Release);
                            }
                            Err(e) => {
                                warn!("Second JWKS fetch failed during backoff retry: {}", e);
                            }
                        }

                        let jwks_guard = self.jwks.read().await;
                        jwks_module::verify_access_token(
                            token,
                            &jwks_guard,
                            &config.issuer_url,
                            &config.client_id,
                        )
                    }
                }
            }
            Err(e) => {
                warn!("JWKS force refresh failed: {}", e);
                Err(OidcError::JwksError(format!(
                    "force refresh failed: {}",
                    e
                )))
            }
        }
    }
}

/// Row type for reading from oidc_config table
#[derive(Debug, sqlx::FromRow)]
struct OidcConfigRow {
    enabled: bool,
    issuer_url: String,
    client_id: String,
    client_secret_encrypted: String,
    redirect_uri: String,
    jwks_refresh_interval_secs: i32,
    disable_password_login: bool,
    auto_create_tenant: bool,
}
