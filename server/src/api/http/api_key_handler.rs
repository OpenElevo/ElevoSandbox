//! API Key management handlers (Admin only)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

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

    let tenant_uuid = match Uuid::parse_str(&tenant_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    match state.tenant_repository.list_api_keys(tenant_uuid).await {
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

    let tenant_uuid = match Uuid::parse_str(&tenant_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

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
    let params: CreateApiKeyParams =
        match serde_json::from_slice(&body) {
            Ok(p) => p,
            Err(e) => return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "BAD_REQUEST", "message": format!("Invalid JSON: {}", e) }
                })),
            )
                .into_response(),
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
        .create_api_key(tenant_uuid, params)
        .await
    {
        Ok((key, token)) => {
            state.audit_service.log(
                &auth,
                "api_key.create",
                "api_key",
                key.id,
                &key.name,
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

    let tenant_uuid = match Uuid::parse_str(&tenant_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    let key_uuid = match Uuid::parse_str(&key_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid key ID"),
    };

    // Fetch key info and verify it belongs to the specified tenant
    let key = match state.tenant_repository.get_api_key(key_uuid).await {
        Ok(k) => k,
        Err(e) => return super::tenant_handler::error_response(e),
    };
    if key.tenant_id != tenant_uuid {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": { "code": "NOT_FOUND", "message": "API key not found for this tenant" }
            })),
        )
            .into_response();
    }
    let key_name = key.name;

    match state.tenant_repository.revoke_api_key(key_uuid).await {
        Ok(()) => {
            state.audit_service.log(
                &auth,
                "api_key.revoke",
                "api_key",
                key_uuid,
                &key_name,
                serde_json::json!({"tenant_id": tenant_id}),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

fn bad_request(msg: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": { "code": "BAD_REQUEST", "message": msg }
        })),
    )
        .into_response()
}
