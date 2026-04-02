//! OIDC authentication flow handlers

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use tracing::{info, warn};
use uuid::Uuid;

use crate::domain::oidc::*;
use crate::domain::auth::AuthContext;
use crate::infra::oidc::pkce::{compute_code_challenge, generate_nonce, generate_state, generate_code_verifier};
use crate::infra::oidc_token_store_repository::CreateTokenStoreParams;
use crate::AppState;

/// Extract client IP from request extensions (set by ConnectInfo<SocketAddr>)
fn client_ip(request: &axum::extract::Request) -> Option<std::net::IpAddr> {
    request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip())
}

/// GET /auth/oidc/config — public, returns OIDC availability
pub async fn get_oidc_config(
    State(state): State<AppState>,
) -> Response {
    let (enabled, disable_password_login) = match state.oidc_service.read().await.as_ref() {
        Some(svc) => svc.get_public_config().await,
        None => (false, false),
    };

    Json(OidcPublicConfig {
        enabled,
        disable_password_login,
    })
    .into_response()
}

/// POST /auth/oidc/authorize — public (rate-limited), starts OIDC flow
pub async fn authorize_oidc(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Response {
    let ip = client_ip(&request);
    let oidc_guard = state.oidc_service.read().await;
    let svc = match oidc_guard.as_ref() {
        Some(svc) => svc,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": {"code": "OIDC_NOT_CONFIGURED", "message": "OIDC is not configured"}
                })),
            )
                .into_response();
        }
    };

    if !svc.is_enabled().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": {"code": "OIDC_DISABLED", "message": "OIDC is disabled"}
            })),
        )
            .into_response();
    }

    let code_verifier = generate_code_verifier();
    let code_challenge = compute_code_challenge(&code_verifier);
    let state_param = generate_state();
    let nonce = generate_nonce();

    // Store auth session
    if let Err(e) = state
        .oidc_auth_session_repo
        .create(crate::infra::oidc_auth_session_repository::CreateAuthSessionParams {
            state: state_param.clone(),
            nonce: nonce.clone(),
            code_verifier: code_verifier.clone(),
            ip_address: ip,
        })
        .await
    {
        tracing::error!("Failed to create OIDC auth session: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": {"code": "INTERNAL_ERROR", "message": "Failed to create auth session"}
            })),
        )
            .into_response();
    }

    match svc.generate_authorize_url(&state_param, &nonce, &code_challenge).await {
        Ok(authorize_url) => Json(OidcAuthorizeResponse { authorize_url }).into_response(),
        Err(e) => {
            tracing::error!("Failed to generate authorize URL: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {"code": "INTERNAL_ERROR", "message": "Failed to generate authorization URL"}
                })),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

/// GET /auth/oidc/callback — public, handles OIDC callback
pub async fn oidc_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
) -> Response {
    // Note: client IP is not captured here since this is a redirect callback from the IdP.
    // The IP is available from the initial authorize_oidc call stored in the auth session.

    // Handle IdP error
    if let Some(error) = &params.error {
        warn!("OIDC callback error: {} - {}", error, params.error_description.as_deref().unwrap_or(""));
        let mut url = build_login_error_url("sso_error");
        if let Some(desc) = &params.error_description {
            url.push_str(&format!("&desc={}", urlencoding::encode(desc)));
        }
        return Redirect::to(&url).into_response();
    }

    let code = match &params.code {
        Some(c) => c.clone(),
        None => {
            return Redirect::to(&build_login_error_url("missing_code")).into_response();
        }
    };

    let state_param = match &params.state {
        Some(s) => s.clone(),
        None => {
            return Redirect::to(&build_login_error_url("missing_state")).into_response();
        }
    };

    let oidc_guard = state.oidc_service.read().await;
    let svc = match oidc_guard.as_ref() {
        Some(svc) => svc,
        None => {
            return Redirect::to(&build_login_error_url("not_configured")).into_response();
        }
    };

    // 1. Consume the auth session (atomic, prevents replay)
    let session = match state.oidc_auth_session_repo.consume_by_state(&state_param).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            warn!("OIDC callback: invalid or expired state");
            state.audit_service.log_anonymous(
                "oidc_login_failed",
                "session",
                Uuid::nil(),
                "oidc",
                serde_json::json!({"reason": "invalid_state"}),
                None,
            );
            return Redirect::to(&build_login_error_url("invalid_state")).into_response();
        }
        Err(e) => {
            tracing::error!("DB error consuming auth session: {}", e);
            state.audit_service.log_anonymous(
                "oidc_login_failed",
                "session",
                Uuid::nil(),
                "oidc",
                serde_json::json!({"reason": "internal_error", "error": e.to_string()}),
                None,
            );
            return Redirect::to(&build_login_error_url("internal_error")).into_response();
        }
    };

    // Capture IP from the auth session (set during authorize_oidc)
    let ip = session.ip_address;

    // 2. Exchange code for tokens
    let token_response = match svc.exchange_code(&code, &session.code_verifier).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("OIDC token exchange failed: {}", e);
            state.audit_service.log_anonymous(
                "oidc_login_failed",
                "session",
                Uuid::nil(),
                "oidc",
                serde_json::json!({"reason": "token_exchange_failed", "error": e.to_string()}),
                ip,
            );
            return Redirect::to(&build_login_error_url("token_exchange_failed")).into_response();
        }
    };

    // 3. Verify ID token
    let claims = match svc.verify_id_token(&token_response.id_token, &session.nonce).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("OIDC ID token verification failed: {}", e);
            state.audit_service.log_anonymous(
                "oidc_login_failed",
                "session",
                Uuid::nil(),
                "oidc",
                serde_json::json!({"reason": "invalid_token", "error": e.to_string()}),
                ip,
            );
            return Redirect::to(&build_login_error_url("invalid_token")).into_response();
        }
    };

    // 4. Check org_role — non-admin users get redirected
    let org_role = claims.org_role.as_deref().unwrap_or("");
    if org_role != "admin" {
        info!(
            "OIDC login: non-admin user (sub={}, org_role={})",
            claims.sub, org_role
        );
        return Redirect::to("/admin/login?activated=true").into_response();
    }

    // 5. Issue local admin JWT
    let session_id = Uuid::now_v7();
    let token = match state
        .auth_config
        .create_admin_token_with_session(session_id, None)
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to create admin token: {}", e);
            return Redirect::to(&build_login_error_url("internal_error")).into_response();
        }
    };

    // 6. Generate session_code and store tokens + user profile
    let session_code = generate_state(); // reuse as random code
    let user_id = claims.sub.parse::<i64>().unwrap_or(0);

    if let Err(e) = state
        .oidc_token_store_repo
        .create(CreateTokenStoreParams {
            local_session_id: session_id,
            user_id,
            org_id: claims.org_id,
            org_role: claims.org_role.clone(),
            email: claims.email.clone(),
            name: claims.name.clone(),
            picture: claims.picture.clone(),
            local_jwt: token.clone(),
            access_token: Some(token_response.access_token.clone()),
            refresh_token: token_response.refresh_token.clone(),
            id_token: token_response.id_token.clone(),
            session_code: Some(session_code.clone()),
            ip_address: ip,
        })
        .await
    {
        tracing::error!("Failed to store OIDC tokens: {}", e);
        return Redirect::to(&build_login_error_url("internal_error")).into_response();
    }

    // 7. Log successful login using AuthContext (not anonymous)
    let auth = AuthContext {
        identity: crate::domain::auth::Identity::Admin { session_id },
        ip_address: ip,
    };
    let detail = serde_json::json!({
        "login_method": "oidc",
        "email": claims.email,
        "org_id": claims.org_id,
        "org_role": claims.org_role,
    });
    state.audit_service.log(
        &auth,
        "oidc_login",
        "session",
        session_id,
        &format!("oidc:{}", claims.sub),
        detail,
    );

    // 8. Redirect to LoginSuccess page
    Redirect::temporary(&format!(
        "/admin/login/success?code={}",
        urlencoding::encode(&session_code)
    ))
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct SessionParams {
    pub code: Option<String>,
}

/// GET /auth/oidc/session — public, exchanges session_code for token + user info
pub async fn exchange_session_code(
    State(state): State<AppState>,
    Query(params): Query<SessionParams>,
) -> Response {
    let code = match &params.code {
        Some(c) => c.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {"code": "MISSING_CODE", "message": "session code is required"}
                })),
            )
                .into_response();
        }
    };

    // Consume the session_code (atomic, one-time use)
    let entry = match state.oidc_token_store_repo.consume_session_code(&code).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {"code": "INVALID_CODE", "message": "invalid or expired session code"}
                })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB error consuming session code: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {"code": "INTERNAL_ERROR", "message": "internal error"}
                })),
            )
                .into_response();
        }
    };

    // Read user info directly from stored record fields
    let user_info = OidcUserInfo {
        name: entry.name.unwrap_or_else(|| "Unknown".to_string()),
        email: entry.email,
        picture: entry.picture,
        is_admin: entry.org_role.as_deref() == Some("admin"),
    };

    Json(OidcSessionResponse {
        token: entry.local_jwt,
        user: user_info,
    })
    .into_response()
}

/// POST /auth/oidc/refresh — requires admin JWT, refreshes ElevoOne tokens
pub async fn refresh_oidc_token(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let extensions = request.extensions();
    let auth = match extensions.get::<crate::domain::auth::AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": {"code": "UNAUTHORIZED", "message": "Not authenticated"}
                })),
            )
                .into_response();
        }
    };

    let oidc_guard = state.oidc_service.read().await;
    let svc = match oidc_guard.as_ref() {
        Some(svc) => svc,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": {"code": "OIDC_NOT_CONFIGURED", "message": "OIDC is not configured"}
                })),
            )
                .into_response();
        }
    };

    let session_id = match &auth.identity {
        crate::domain::auth::Identity::Admin { session_id } => *session_id,
        _ => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": {"code": "FORBIDDEN", "message": "admin access required"}
                })),
            )
                .into_response();
        }
    };

    // Find stored tokens
    let entry = match state.oidc_token_store_repo.find_by_session_id(session_id).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": {"code": "NO_OIDC_SESSION", "message": "no OIDC session found"}
                })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("DB error finding OIDC token store: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {"code": "INTERNAL_ERROR", "message": "internal error"}
                })),
            )
                .into_response();
        }
    };

    let refresh_token = match &entry.refresh_token {
        Some(t) => t.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {"code": "NO_REFRESH_TOKEN", "message": "no refresh token available"}
                })),
            )
                .into_response();
        }
    };

    // Refresh tokens
    match svc.refresh_elevoone_token(&refresh_token).await {
        Ok(token_response) => {
            // Update stored tokens
            if let Err(e) = state
                .oidc_token_store_repo
                .update_tokens(
                    entry.id,
                    &token_response.access_token,
                    token_response.refresh_token.as_deref(),
                    &token_response.id_token,
                )
                .await
            {
                tracing::error!("Failed to update OIDC tokens: {}", e);
            }

            // Audit log
            let detail = serde_json::json!({
                "login_method": "oidc",
                "session_id": session_id,
            });
            state.audit_service.log(
                &auth,
                "oidc_token_refresh",
                "session",
                session_id,
                &format!("session:{}", session_id),
                detail,
            );

            Json(OidcRefreshResponse { success: true }).into_response()
        }
        Err(e) => {
            tracing::warn!("OIDC token refresh failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {"code": "REFRESH_FAILED", "message": "token refresh failed"}
                })),
            )
                .into_response()
        }
    }
}

/// POST /auth/logout — clears OIDC session and returns IdP logout URL
pub async fn oidc_logout(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Response {
    let mut idp_logout_url: Option<String> = None;

    if let Some(auth) = request.extensions().get::<crate::domain::auth::AuthContext>() {
        if let crate::domain::auth::Identity::Admin { session_id } = auth.identity {
            // Build IdP logout URL before deleting the token store entry
            if let Some(svc) = state.oidc_service.read().await.as_ref() {
                if let Ok(Some(entry)) = state.oidc_token_store_repo.find_by_session_id(session_id).await {
                    if let Some(id_token) = &entry.id_token {
                        match svc.build_end_session_url(id_token).await {
                            Ok(url) => idp_logout_url = Some(url),
                            Err(e) => tracing::warn!("Failed to build IdP logout URL: {}", e),
                        }
                    }
                }
            }

            // Clean up OIDC token store entry
            if let Err(e) = state.oidc_token_store_repo.delete_by_session_id(session_id).await {
                tracing::warn!("Failed to clean up OIDC token store on logout: {}", e);
            }

            // Audit log
            let detail = serde_json::json!({
                "login_method": "oidc",
                "session_id": session_id,
            });
            state.audit_service.log(
                auth,
                "logout",
                "session",
                session_id,
                &format!("session:{}", session_id),
                detail,
            );
        }
    }

    Json(OidcLogoutResponse { idp_logout_url }).into_response()
}

/// Build a login page redirect URL with an error parameter
fn build_login_error_url(error_code: &str) -> String {
    format!("/admin/login?error={}", urlencoding::encode(error_code))
}
