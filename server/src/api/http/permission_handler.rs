//! Share permission management API handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::domain::auth::AuthContext;
use crate::domain::permission::PermissionLevel;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct GrantPermissionRequest {
    pub tenant_id: String,
    pub permission: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePermissionRequest {
    pub permission: String,
}

/// GET /api/v1/shares/:id/permissions
pub async fn list_permissions(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return unauthorized(),
    };

    // Only share owner or admin can view permissions
    if let Err(e) = check_share_admin(&state, &auth, &share_id).await {
        return e;
    }

    match state
        .share_permission_repository
        .list_by_share(&share_id)
        .await
    {
        Ok(permissions) => (
            StatusCode::OK,
            Json(serde_json::json!({"permissions": permissions})),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// POST /api/v1/shares/:id/permissions
pub async fn grant_permission(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return unauthorized(),
    };

    if let Err(e) = check_share_admin(&state, &auth, &share_id).await {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => return bad_request("Body too large"),
    };
    let req: GrantPermissionRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return bad_request(&format!("Invalid JSON: {}", e)),
    };

    let level = match PermissionLevel::from_str_value(&req.permission) {
        Some(l) => l,
        None => return bad_request("Invalid permission level. Must be: read, write, execute, admin"),
    };

    // Cannot grant permission to the share owner
    let share = match state.share_repository.get_share(&share_id).await {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };
    if share.owner_tenant_id == req.tenant_id {
        return bad_request("Cannot grant permission to the share owner");
    }

    match state
        .share_permission_repository
        .grant_permission(&share_id, &req.tenant_id, level)
        .await
    {
        Ok(perm) => {
            state.audit_service.log(
                &auth, "permission.grant", "share", &share_id, "",
                serde_json::json!({"tenant_id": req.tenant_id, "permission": req.permission}),
            );
            (
                StatusCode::CREATED,
                Json(serde_json::json!({"permission": perm})),
            )
                .into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// PUT /api/v1/shares/:id/permissions/:tenant_id
pub async fn update_permission(
    State(state): State<AppState>,
    Path((share_id, tenant_id)): Path<(String, String)>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return unauthorized(),
    };

    if let Err(e) = check_share_admin(&state, &auth, &share_id).await {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => return bad_request("Body too large"),
    };
    let req: UpdatePermissionRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return bad_request(&format!("Invalid JSON: {}", e)),
    };

    let level = match PermissionLevel::from_str_value(&req.permission) {
        Some(l) => l,
        None => return bad_request("Invalid permission level"),
    };

    match state
        .share_permission_repository
        .grant_permission(&share_id, &tenant_id, level)
        .await
    {
        Ok(perm) => {
            state.audit_service.log(
                &auth, "permission.update", "share", &share_id, "",
                serde_json::json!({"tenant_id": tenant_id, "permission": req.permission}),
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({"permission": perm})),
            )
                .into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// DELETE /api/v1/shares/:id/permissions/:tenant_id
pub async fn revoke_permission(
    State(state): State<AppState>,
    Path((share_id, tenant_id)): Path<(String, String)>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return unauthorized(),
    };

    if let Err(e) = check_share_admin(&state, &auth, &share_id).await {
        return e;
    }

    match state
        .share_permission_repository
        .revoke_permission(&share_id, &tenant_id)
        .await
    {
        Ok(()) => {
            state.audit_service.log(
                &auth, "permission.revoke", "share", &share_id, "",
                serde_json::json!({"tenant_id": tenant_id}),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/tenants/:id/permissions — list all permissions for a tenant (Admin only)
pub async fn list_tenant_permissions(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a,
        None => return unauthorized(),
    };

    if !auth.is_admin() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "Admin access required"}})),
        )
            .into_response();
    }

    match state
        .share_permission_repository
        .list_by_tenant(&tenant_id)
        .await
    {
        Ok(permissions) => (
            StatusCode::OK,
            Json(serde_json::json!({"permissions": permissions})),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// Check if the caller is the share owner or admin
async fn check_share_admin(
    state: &AppState,
    auth: &AuthContext,
    share_id: &str,
) -> Result<(), axum::response::Response> {
    if auth.is_admin() {
        return Ok(());
    }

    let share = state
        .share_repository
        .get_share(share_id)
        .await
        .map_err(|e| super::tenant_handler::error_response(e))?;

    if let Some(tid) = auth.tenant_id() {
        if share.owner_tenant_id == tid.to_string() {
            return Ok(());
        }
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "Share admin access required"}})),
    )
        .into_response())
}

fn unauthorized() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": {"code": "UNAUTHORIZED"}})),
    )
        .into_response()
}

fn bad_request(msg: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": msg}})),
    )
        .into_response()
}
