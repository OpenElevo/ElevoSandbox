//! API Key management handlers (Admin only)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use crate::domain::auth::AuthContext;
use crate::domain::tenant::CreateApiKeyParams;
use crate::AppState;

/// GET /api/v1/tenants/:id/keys
pub async fn list_api_keys(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": { "code": "UNAUTHORIZED", "message": "Not authenticated" }
                })),
            )
                .into_response()
        }
    };
    if !auth.is_admin() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": { "code": "FORBIDDEN", "message": "Admin access required" }
            })),
        )
            .into_response();
    }

    match state.tenant_repository.list_api_keys(&tenant_id).await {
        Ok(keys) => (StatusCode::OK, Json(serde_json::json!({ "keys": keys }))).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// POST /api/v1/tenants/:id/keys
pub async fn create_api_key(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": { "code": "UNAUTHORIZED", "message": "Not authenticated" }
                })),
            )
                .into_response()
        }
    };
    if !auth.is_admin() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": { "code": "FORBIDDEN", "message": "Admin access required" }
            })),
        )
            .into_response();
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "BAD_REQUEST", "message": "Invalid request body" }
                })),
            )
                .into_response()
        }
    };
    let params: CreateApiKeyParams = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "BAD_REQUEST", "message": format!("Invalid JSON: {}", e) }
                })),
            )
                .into_response()
        }
    };

    if params.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": { "code": "BAD_REQUEST", "message": "Key name is required" }
            })),
        )
            .into_response();
    }

    match state
        .tenant_repository
        .create_api_key(&tenant_id, params)
        .await
    {
        Ok((key, token)) => {
            state.audit_service.log(
                &auth, "api_key.create", "api_key", &key.id, &key.name,
                serde_json::json!({"tenant_id": tenant_id}),
            );
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "key": key,
                    "token": token,
                })),
            )
                .into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// DELETE /api/v1/tenants/:id/keys/:key_id
pub async fn revoke_api_key(
    State(state): State<AppState>,
    Path((tenant_id, key_id)): Path<(String, String)>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": { "code": "UNAUTHORIZED", "message": "Not authenticated" }
                })),
            )
                .into_response()
        }
    };
    if !auth.is_admin() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": { "code": "FORBIDDEN", "message": "Admin access required" }
            })),
        )
            .into_response();
    }

    match state.tenant_repository.revoke_api_key(&key_id).await {
        Ok(()) => {
            state.audit_service.log(
                &auth, "api_key.revoke", "api_key", &key_id, "",
                serde_json::json!({"tenant_id": tenant_id}),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}
