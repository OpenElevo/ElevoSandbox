//! Authentication middleware for Axum
//!
//! Supports two authentication paths:
//! - JWT (Admin): `Authorization: Bearer <jwt_token>`
//! - API Key (Tenant): `Authorization: Bearer sk_<token>`
//! - Dev mode: when ADMIN_PASSWORD is not set, all requests are treated as Admin

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::domain::auth::{AuthContext, AuthError, Identity, JwtClaims};
use crate::AppState;

/// Auth configuration extracted from AppState
#[derive(Clone)]
pub struct AuthConfig {
    pub admin_password: Option<String>,
    pub jwt_secret: Option<String>,
    pub jwt_expiration_hours: u64,
    pub dev_mode: bool,
}

impl AuthConfig {
    pub fn from_config(config: &crate::config::Config) -> Self {
        let dev_mode = config.admin_password.is_none();
        Self {
            admin_password: config.admin_password.clone(),
            jwt_secret: config.jwt_secret.clone(),
            jwt_expiration_hours: config.jwt_expiration_hours,
            dev_mode,
        }
    }

    fn jwt_secret_bytes(&self) -> &[u8] {
        match self.jwt_secret.as_deref() {
            Some(secret) => secret.as_bytes(),
            None => {
                if !self.dev_mode {
                    // This should not happen — config validation should catch it.
                    // Log a loud warning so operators notice immediately.
                    warn!(
                        "JWT_SECRET is not configured in production mode! Using insecure fallback."
                    );
                }
                b"dev-secret-do-not-use-in-production"
            }
        }
    }

    /// Create a JWT token for admin login with a freshly generated session ID
    pub fn create_admin_token(&self, ip: Option<String>) -> Result<String, AuthError> {
        self.create_admin_token_with_session(Uuid::new_v4(), ip)
    }

    /// Create a JWT token reusing an existing session ID (used for sliding-window renewal)
    pub fn create_admin_token_with_session(
        &self,
        session_id: Uuid,
        ip: Option<String>,
    ) -> Result<String, AuthError> {
        let now = Utc::now().timestamp();
        let exp = now + (self.jwt_expiration_hours as i64 * 3600);
        let claims = JwtClaims {
            sub: "admin".to_string(),
            session_id,
            login_ip: ip,
            iat: now,
            exp,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret_bytes()),
        )
        .map_err(|e| AuthError::Internal(format!("JWT encode error: {}", e)))
    }

    /// Verify and decode a JWT token (used internally by HTTP middleware)
    fn verify_jwt(&self, token: &str) -> Result<JwtClaims, AuthError> {
        let mut validation = Validation::default();
        validation.set_required_spec_claims(&["exp", "sub"]);
        // Only accept "admin" as valid subject
        validation.sub = Some("admin".to_string());

        let data = decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret_bytes()),
            &validation,
        )
        .map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
            jsonwebtoken::errors::ErrorKind::InvalidSubject => {
                AuthError::InvalidToken("invalid subject claim".to_string())
            }
            _ => AuthError::InvalidToken(format!("{}", e)),
        })?;

        Ok(data.claims)
    }

    /// Verify a JWT token — public interface for gRPC auth layer
    pub fn verify_jwt_public(&self, token: &str) -> Result<JwtClaims, AuthError> {
        self.verify_jwt(token)
    }

    /// Check if a JWT token needs refresh (remaining < 1/3 of total lifetime)
    fn should_refresh(&self, claims: &JwtClaims) -> bool {
        let total = (self.jwt_expiration_hours as i64) * 3600;
        let remaining = claims.exp - Utc::now().timestamp();
        remaining < total / 3
    }
}

/// Authentication middleware
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let auth_config = &state.auth_config;

    // Dev mode: skip authentication, treat as admin
    if auth_config.dev_mode {
        let ctx = AuthContext {
            identity: Identity::Admin {
                session_id: Uuid::nil(),
            },
            ip_address: None,
        };
        request.extensions_mut().insert(ctx);
        return next.run(request).await;
    }

    // Extract Authorization header
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let token = match auth_header {
        Some(ref h) if h.starts_with("Bearer ") => &h[7..],
        _ => {
            return auth_error_response(AuthError::MissingToken);
        }
    };

    // Extract client IP (honouring TRUSTED_PROXY_IPS allowlist)
    let ip_address = extract_client_ip(&request, &state.config.trusted_proxy_ips);

    // Route based on token prefix
    if token.starts_with("sk_") {
        // API Key authentication path
        match authenticate_api_key(&state, token, ip_address).await {
            Ok(ctx) => {
                request.extensions_mut().insert(ctx);
                next.run(request).await
            }
            Err(e) => auth_error_response(e),
        }
    } else {
        // JWT authentication path
        match authenticate_jwt(auth_config, token, ip_address) {
            Ok((ctx, refreshed_token)) => {
                request.extensions_mut().insert(ctx);
                let mut response = next.run(request).await;
                // Sliding window token refresh
                if let Some(new_token) = refreshed_token {
                    if let Ok(val) = axum::http::HeaderValue::from_str(&new_token) {
                        response.headers_mut().insert("X-Refreshed-Token", val);
                    }
                }
                response
            }
            Err(e) => auth_error_response(e),
        }
    }
}

/// Authenticate via API Key (SHA-256 hash lookup)
async fn authenticate_api_key(
    state: &AppState,
    token: &str,
    ip_address: Option<std::net::IpAddr>,
) -> Result<AuthContext, AuthError> {
    let result = state
        .tenant_repository
        .find_by_token_hash(token)
        .await
        .map_err(|e| AuthError::Internal(format!("DB error: {}", e)))?;

    let (key, tenant) = match result {
        Some(pair) => pair,
        None => return Err(AuthError::InvalidToken("unknown API key".to_string())),
    };

    if !key.is_usable() {
        return Err(AuthError::ApiKeyInvalid);
    }

    if !tenant.is_active {
        return Err(AuthError::TenantDeactivated);
    }

    // Update last_used_at via batching tracker (coalesces writes)
    state.api_key_usage.update(key.id);

    Ok(AuthContext {
        identity: Identity::Tenant {
            id: tenant.id,
            name: tenant.name,
        },
        ip_address,
    })
}

/// Authenticate via JWT
fn authenticate_jwt(
    config: &AuthConfig,
    token: &str,
    ip_address: Option<std::net::IpAddr>,
) -> Result<(AuthContext, Option<String>), AuthError> {
    let claims = config.verify_jwt(token)?;

    let ctx = AuthContext {
        identity: Identity::Admin {
            session_id: claims.session_id,
        },
        ip_address,
    };

    // Check if token needs refresh — preserve session_id, use current request IP
    let refreshed = if config.should_refresh(&claims) {
        debug!("JWT token nearing expiry, issuing refresh");
        config
            .create_admin_token_with_session(claims.session_id, ip_address.map(|ip| ip.to_string()))
            .ok()
    } else {
        None
    };

    Ok((ctx, refreshed))
}

/// Extract the direct connection IP from request extensions.
fn socket_ip(request: &Request) -> Option<std::net::IpAddr> {
    request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip())
}

/// Extract client IP from request, respecting the trusted-proxy allowlist.
///
/// When `trusted_proxy_ips` is non-empty, proxy headers (X-Forwarded-For,
/// X-Real-IP) are only honoured when the direct connection originates from
/// one of the listed trusted proxy IPs.  When the list is empty, the legacy
/// behaviour (always trust XFF/XRI) is preserved for backward compatibility.
pub(crate) fn extract_client_ip(
    request: &Request,
    trusted_proxy_ips: &[String],
) -> Option<std::net::IpAddr> {
    let direct_ip = socket_ip(request);

    // Determine whether proxy headers should be trusted for this connection.
    let should_trust_proxy = if trusted_proxy_ips.is_empty() {
        // Backward-compatible: always trust proxy headers when no list is configured.
        true
    } else {
        // Only trust proxy headers when the direct connection comes from a
        // configured trusted proxy IP.
        direct_ip.map_or(false, |ip| {
            trusted_proxy_ips
                .iter()
                .any(|trusted| trusted.trim().parse::<std::net::IpAddr>().ok() == Some(ip))
        })
    };

    if should_trust_proxy {
        // Try X-Forwarded-For first
        if let Some(xff) = request.headers().get("x-forwarded-for") {
            if let Ok(s) = xff.to_str() {
                if let Some(first) = s.split(',').next() {
                    if let Ok(ip) = first.trim().parse() {
                        return Some(ip);
                    }
                }
            }
        }
        // Try X-Real-IP
        if let Some(xri) = request.headers().get("x-real-ip") {
            if let Ok(s) = xri.to_str() {
                if let Ok(ip) = s.trim().parse() {
                    return Some(ip);
                }
            }
        }
    }

    // Fall back to the direct socket address.
    direct_ip
}

/// Convert AuthError to HTTP response
fn auth_error_response(err: AuthError) -> Response {
    let (status, code) = match &err {
        AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
        AuthError::InvalidToken(_) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
        AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "TOKEN_EXPIRED"),
        AuthError::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),
        AuthError::TenantDeactivated => (StatusCode::FORBIDDEN, "TENANT_DEACTIVATED"),
        AuthError::ApiKeyInvalid => (StatusCode::UNAUTHORIZED, "API_KEY_INVALID"),
        AuthError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
    };

    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": err.to_string(),
        }
    });

    (status, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AuthConfig {
        AuthConfig {
            admin_password: Some("test-password".to_string()),
            jwt_secret: Some("test-jwt-secret-at-least-32-bytes-long!!".to_string()),
            jwt_expiration_hours: 24,
            dev_mode: false,
        }
    }

    fn dev_config() -> AuthConfig {
        AuthConfig {
            admin_password: None,
            jwt_secret: None,
            jwt_expiration_hours: 24,
            dev_mode: true,
        }
    }

    #[test]
    fn test_create_and_verify_jwt() {
        let config = test_config();
        let token = config
            .create_admin_token(Some("127.0.0.1".to_string()))
            .expect("Failed to create token");

        let claims = config
            .verify_jwt_public(&token)
            .expect("Failed to verify token");
        assert_eq!(claims.sub, "admin");
        assert_eq!(claims.login_ip, Some("127.0.0.1".to_string()));
    }

    #[test]
    fn test_jwt_invalid_secret_fails() {
        let config = test_config();
        let token = config
            .create_admin_token(None)
            .expect("Failed to create token");

        let other_config = AuthConfig {
            jwt_secret: Some("different-secret-at-least-32-bytes-long!!".to_string()),
            ..test_config()
        };
        let result = other_config.verify_jwt_public(&token);
        assert!(result.is_err());
    }

    #[test]
    fn test_jwt_expired_token() {
        let config = test_config();
        let now = Utc::now().timestamp();
        // Create a token that expired 120 seconds ago (well beyond any leeway)
        let claims = JwtClaims {
            sub: "admin".to_string(),
            session_id: Uuid::new_v4(),
            login_ip: None,
            iat: now - 7200,
            exp: now - 120,
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(config.jwt_secret_bytes()),
        )
        .unwrap();

        let result = config.verify_jwt_public(&token);
        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }

    #[test]
    fn test_jwt_garbage_token() {
        let config = test_config();
        let result = config.verify_jwt_public("not-a-valid-jwt-token");
        assert!(matches!(result, Err(AuthError::InvalidToken(_))));
    }

    #[test]
    fn test_should_refresh_when_near_expiry() {
        let config = test_config();
        let now = Utc::now().timestamp();
        let total_seconds = (config.jwt_expiration_hours as i64) * 3600;

        // Token with lots of time remaining — should NOT refresh
        let fresh_claims = JwtClaims {
            sub: "admin".to_string(),
            session_id: Uuid::new_v4(),
            login_ip: None,
            iat: now,
            exp: now + total_seconds,
        };
        assert!(!config.should_refresh(&fresh_claims));

        // Token near expiry (remaining < 1/3 of total) — should refresh
        let stale_claims = JwtClaims {
            sub: "admin".to_string(),
            session_id: Uuid::new_v4(),
            login_ip: None,
            iat: now - total_seconds,
            exp: now + 60, // Only 1 minute left
        };
        assert!(config.should_refresh(&stale_claims));
    }

    #[test]
    fn test_dev_mode_uses_fallback_secret() {
        let config = dev_config();
        // Dev mode should still create valid tokens using fallback secret
        let token = config
            .create_admin_token(None)
            .expect("Failed to create token in dev mode");
        let claims = config
            .verify_jwt_public(&token)
            .expect("Failed to verify dev mode token");
        assert_eq!(claims.sub, "admin");
    }

    #[test]
    fn test_jwt_invalid_subject_rejected() {
        let config = test_config();
        let now = Utc::now().timestamp();
        // Create a token with sub="hacker" instead of "admin"
        let claims = JwtClaims {
            sub: "hacker".to_string(),
            session_id: Uuid::new_v4(),
            login_ip: None,
            iat: now,
            exp: now + 3600,
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(config.jwt_secret_bytes()),
        )
        .unwrap();

        let result = config.verify_jwt_public(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken(_))));
    }

    #[test]
    fn test_auth_error_response_status_codes() {
        let cases = vec![
            (AuthError::MissingToken, StatusCode::UNAUTHORIZED),
            (
                AuthError::InvalidToken("bad".into()),
                StatusCode::UNAUTHORIZED,
            ),
            (AuthError::TokenExpired, StatusCode::UNAUTHORIZED),
            (AuthError::Forbidden("nope".into()), StatusCode::FORBIDDEN),
            (AuthError::TenantDeactivated, StatusCode::FORBIDDEN),
            (AuthError::ApiKeyInvalid, StatusCode::UNAUTHORIZED),
            (
                AuthError::Internal("oops".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];

        for (err, expected_status) in cases {
            let response = auth_error_response(err);
            assert_eq!(response.status(), expected_status);
        }
    }
}
