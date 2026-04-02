//! OIDC configuration management handlers (admin-only)

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use tracing::{info, warn};
use uuid;

use crate::domain::oidc::*;
use crate::infra::oidc::crypto::encrypt_client_secret;
use crate::infra::oidc::OidcService;
use crate::infra::oidc_config_repository::OidcConfigRepository;
use crate::AppState;

/// Helper to parse JSON body from raw request
async fn parse_json_body<T: serde::de::DeserializeOwned>(
    request: axum::extract::Request,
) -> Result<T, Response> {
    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {"code": "BAD_REQUEST", "message": "Invalid request body"}
                })),
            )
                .into_response());
        }
    };
    serde_json::from_slice(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": {"code": "BAD_REQUEST", "message": format!("Invalid JSON: {}", e)}
            })),
        )
            .into_response()
    })
}

/// GET /system/oidc-config — admin, returns OIDC configuration
pub async fn get_oidc_config(
    State(state): State<AppState>,
) -> Response {
    let config = match state.oidc_service.read().await.as_ref() {
        Some(svc) => svc.get_full_config().await,
        None => {
            return Json(OidcConfigDisplay {
                enabled: false,
                issuer_url: String::new(),
                client_id: String::new(),
                client_secret: String::new(),
                redirect_uri: String::new(),
                jwks_refresh_interval_secs: 3600,
                disable_password_login: false,
                auto_create_tenant: false,
            })
            .into_response();
        }
    };

    // Check circuit breaker — override disable_password_login if tripped
    let mut disable_password_login = config.disable_password_login;
    if let Some(svc) = state.oidc_service.read().await.as_ref() {
        if svc.circuit_breaker().should_force_password_login() {
            disable_password_login = false;
        }
    }

    // Mask client_secret
    let client_secret = if config.client_secret.is_empty() {
        String::new()
    } else {
        "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}".to_string()
    };

    Json(OidcConfigDisplay {
        enabled: config.enabled,
        issuer_url: config.issuer_url,
        client_id: config.client_id,
        client_secret,
        redirect_uri: config.redirect_uri,
        jwks_refresh_interval_secs: config.jwks_refresh_interval_secs as i32,
        disable_password_login,
        auto_create_tenant: config.auto_create_tenant,
    })
    .into_response()
}

/// PUT /system/oidc-config — admin, updates OIDC configuration
pub async fn update_oidc_config(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = request.extensions().get::<crate::domain::auth::AuthContext>().cloned();
    let params: OidcConfigUpdateParams = match parse_json_body(request).await {
        Ok(p) => p,
        Err(e) => return e,
    };

    // Validate: disable_password_login requires enabled=true
    if params.disable_password_login && !params.enabled {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": {"code": "INVALID_CONFIG", "message": "disable_password_login requires enabled=true"}
            })),
        )
            .into_response();
    }

    // Encrypt client_secret if provided, otherwise keep existing
    let encryption_key = match state.config.get_oidc_encryption_key() {
        Some(k) => k,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {"code": "NO_ENCRYPTION_KEY", "message": "OIDC encryption key not available"}
                })),
            )
                .into_response();
        }
    };

    let client_secret_enc = if let Some(secret) = &params.client_secret {
        if secret.is_empty() {
            String::new()
        } else {
            match encrypt_client_secret(secret, &encryption_key) {
                Ok(enc) => enc,
                Err(e) => {
                    tracing::error!("Failed to encrypt client_secret: {}", e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error": {"code": "ENCRYPTION_ERROR", "message": "Failed to encrypt client secret"}
                        })),
                    )
                        .into_response();
                }
            }
        }
    } else {
        // Keep existing encrypted secret
        match state.oidc_config_repo.get_client_secret_encrypted().await {
            Ok(Some(enc)) => enc,
            Ok(None) => String::new(),
            Err(e) => {
                tracing::error!("Failed to get existing client_secret: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": {"code": "INTERNAL_ERROR", "message": "Failed to read existing config"}
                    })),
                )
                    .into_response();
            }
        }
    };

    // Upsert config
    if let Err(e) = state
        .oidc_config_repo
        .upsert_config(OidcConfigRepository::upsert_params(
            params.enabled,
            &params.issuer_url,
            &params.client_id,
            &client_secret_enc,
            &params.redirect_uri,
            params.jwks_refresh_interval_secs,
            params.disable_password_login,
            params.auto_create_tenant,
        ))
        .await
    {
        tracing::error!("Failed to upsert OIDC config: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": {"code": "INTERNAL_ERROR", "message": "Failed to save configuration"}
            })),
        )
            .into_response();
    }

    // Reload OIDC service (or lazily initialize if it was None)
    if let Some(svc) = state.oidc_service.read().await.as_ref() {
        if let Err(e) = svc.reload_config().await {
            tracing::error!("Failed to reload OIDC config: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": {"code": "RELOAD_ERROR", "message": "Configuration saved but service reload failed"}
                })),
            )
                .into_response();
        }
    } else if params.enabled {
        // Service doesn't exist yet but admin wants OIDC enabled — try to initialize
        match OidcService::new_from_db(state.pool.clone(), encryption_key, state.config.storage.workspace_dir().to_path_buf()).await {
            Ok(Some(svc)) => {
                info!("OIDC service lazily initialized after config upsert");
                *state.oidc_service.write().await = Some(svc);
                state.ensure_oidc_background_tasks().await;
            }
            Ok(None) => {
                // Config exists but not enabled (shouldn't happen here since params.enabled is true)
                info!("OIDC config saved but service returned None (not enabled in DB)");
            }
            Err(e) => {
                tracing::error!("OIDC service lazy initialization failed (non-fatal): {}", e);
                // Config is still saved — admin can retry or check logs
            }
        }
    }

    info!("OIDC configuration updated");
    if let Some(ref auth) = auth {
        state.audit_service.log(
            auth,
            "oidc_config_update",
            "oidc_config",
            uuid::Uuid::nil(),
            "OIDC Configuration",
            serde_json::json!({
                "enabled": params.enabled,
                "issuer_url": params.issuer_url,
                "client_id": params.client_id,
            }),
        );
    }
    Json(serde_json::json!({ "success": true })).into_response()
}

/// POST /system/oidc-config/test — admin, tests OIDC connection
pub async fn test_oidc_config(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Response {
    let auth = request.extensions().get::<crate::domain::auth::AuthContext>().cloned();
    let oidc_guard = state.oidc_service.read().await;
    let svc = match oidc_guard.as_ref() {
        Some(svc) => svc,
        None => {
            return Json(OidcTestResponse {
                success: false,
                message: "OIDC is not configured".to_string(),
            })
            .into_response();
        }
    };

    match svc.test_connection().await {
        Ok(()) => {
            if let Some(ref auth) = auth {
                state.audit_service.log(
                    auth,
                    "oidc_config_test",
                    "oidc_config",
                    uuid::Uuid::nil(),
                    "OIDC Configuration",
                    serde_json::json!({"success": true}),
                );
            }
            Json(OidcTestResponse {
                success: true,
                message: "Connection successful".to_string(),
            })
            .into_response()
        }
        Err(e) => {
            warn!("OIDC connection test failed: {}", e);
            if let Some(ref auth) = auth {
                state.audit_service.log(
                    auth,
                    "oidc_config_test",
                    "oidc_config",
                    uuid::Uuid::nil(),
                    "OIDC Configuration",
                    serde_json::json!({"success": false, "error": e.to_string()}),
                );
            }
            Json(OidcTestResponse {
                success: false,
                message: format!("Connection failed: {}", e),
            })
            .into_response()
        }
    }
}

// ── Helper impl for OidcConfigRepository ──

impl OidcConfigRepository {
    /// Helper to create UpsertOidcConfigParams
    pub fn upsert_params(
        enabled: bool,
        issuer_url: &str,
        client_id: &str,
        client_secret_encrypted: &str,
        redirect_uri: &str,
        jwks_refresh_interval_secs: i32,
        disable_password_login: bool,
        auto_create_tenant: bool,
    ) -> crate::infra::oidc_config_repository::UpsertOidcConfigParams {
        crate::infra::oidc_config_repository::UpsertOidcConfigParams {
            enabled,
            issuer_url: issuer_url.to_string(),
            client_id: client_id.to_string(),
            client_secret_encrypted: client_secret_encrypted.to_string(),
            redirect_uri: redirect_uri.to_string(),
            jwks_refresh_interval_secs,
            disable_password_login,
            auto_create_tenant,
        }
    }
}
