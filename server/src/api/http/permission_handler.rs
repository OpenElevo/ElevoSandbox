//! Share permission management API handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

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
    Query(pagination): Query<crate::domain::tenant::Pagination>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return unauthorized(),
    };

    let share_uuid = match Uuid::parse_str(&share_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };

    // Only share owner or admin can view permissions
    if let Err(e) = check_share_admin(&state, &auth, share_uuid).await {
        return e;
    }

    match state
        .share_permission_repository
        .list_by_share_paginated(share_uuid, pagination)
        .await
    {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "items": result.items,
                "total": result.total,
                "page": result.page,
                "page_size": result.page_size
            })),
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

    let share_uuid = match Uuid::parse_str(&share_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };

    if let Err(e) = check_share_admin(&state, &auth, share_uuid).await {
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
        None => {
            return bad_request("Invalid permission level. Must be: read, write, execute, admin")
        }
    };

    let tenant_uuid = match Uuid::parse_str(&req.tenant_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant_id"),
    };

    // Cannot grant permission to the share owner
    let share = match state.share_repository.get_share(share_uuid).await {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };
    if share.owner_tenant_id == tenant_uuid {
        return bad_request("Cannot grant permission to the share owner");
    }

    match state
        .share_permission_repository
        .grant_permission(share_uuid, tenant_uuid, level)
        .await
    {
        Ok(perm) => {
            state.audit_service.log(
                &auth, "permission.grant", "permission", share_uuid, &share.name,
                serde_json::json!({"share_id": share_uuid, "tenant_id": req.tenant_id, "permission": req.permission}),
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

    let share_uuid = match Uuid::parse_str(&share_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };
    let tenant_uuid = match Uuid::parse_str(&tenant_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    if let Err(e) = check_share_admin(&state, &auth, share_uuid).await {
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

    // Fetch share name for audit
    let share_name = match state.share_repository.get_share(share_uuid).await {
        Ok(s) => s.name,
        Err(_) => String::new(),
    };

    match state
        .share_permission_repository
        .update_permission(share_uuid, tenant_uuid, level)
        .await
    {
        Ok(perm) => {
            state.audit_service.log(
                &auth, "permission.update", "permission", share_uuid, &share_name,
                serde_json::json!({"share_id": share_uuid, "tenant_id": tenant_id, "permission": req.permission}),
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

    let share_uuid = match Uuid::parse_str(&share_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };
    let tenant_uuid = match Uuid::parse_str(&tenant_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    if let Err(e) = check_share_admin(&state, &auth, share_uuid).await {
        return e;
    }

    // Fetch share name for audit
    let share_name = match state.share_repository.get_share(share_uuid).await {
        Ok(s) => s.name,
        Err(_) => String::new(),
    };

    match state
        .share_permission_repository
        .revoke_permission(share_uuid, tenant_uuid)
        .await
    {
        Ok(()) => {
            state.audit_service.log(
                &auth,
                "permission.revoke",
                "permission",
                share_uuid,
                &share_name,
                serde_json::json!({"share_id": share_uuid, "tenant_id": tenant_id}),
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
    Query(pagination): Query<crate::domain::tenant::Pagination>,
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

    let tenant_uuid = match Uuid::parse_str(&tenant_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    match state
        .share_permission_repository
        .list_by_tenant_paginated(tenant_uuid, pagination)
        .await
    {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "items": result.items,
                "total": result.total,
                "page": result.page,
                "page_size": result.page_size
            })),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// Check if the caller is the share owner, a system admin, or has explicit admin
/// permission on this share.
async fn check_share_admin(
    state: &AppState,
    auth: &AuthContext,
    share_id: Uuid,
) -> Result<(), axum::response::Response> {
    // System admin can always manage shares
    if auth.is_admin() {
        return Ok(());
    }

    let share = state
        .share_repository
        .get_share(share_id)
        .await
        .map_err(super::tenant_handler::error_response)?;

    if let Some(tid) = auth.tenant_id() {
        // Share owner has full admin access
        if share.owner_tenant_id == tid {
            return Ok(());
        }

        // Also allow tenants who have been explicitly granted admin permission
        if let Ok(Some(level)) = state
            .share_permission_repository
            .get_permission(share_id, tid)
            .await
        {
            if level == crate::domain::permission::PermissionLevel::Admin {
                return Ok(());
            }
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
