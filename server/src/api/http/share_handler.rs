//! Share management API handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::domain::auth::AuthContext;
use crate::domain::permission::PermissionLevel;
use crate::domain::share::{CreateShareParams, ShareFilter, UpdateShareParams};
use crate::domain::tenant::Pagination;
use crate::domain::UuidSimple;
use crate::service::path_security;
use crate::AppState;

/// POST /api/v1/shares
pub async fn create_share(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED"}})),
            )
                .into_response()
        }
    };

    // Determine owner: admin must specify, tenant uses own ID
    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "Body too large"}}),
            ),
        )
            .into_response(),
    };

    let mut params: CreateShareParams = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("Invalid JSON: {}", e)}})),
            )
                .into_response()
        }
    };

    // If tenant, force owner to self
    if let Some(tid) = auth.tenant_id() {
        params.owner_tenant_id = Some(tid);
    } else if !auth.is_admin() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": {"code": "FORBIDDEN"}})),
        )
            .into_response();
    }

    let owner_id = match params.owner_tenant_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "owner_tenant_id is required"}})),
            )
                .into_response()
        }
    };

    // Normalize source_path to eliminate `.` and leading slashes before
    // the existence check and before storing in the database.
    // `..` components are now rejected outright.
    let normalized_source = match path_security::normalize_path(&params.source_path) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => return super::tenant_handler::error_response(e),
    };
    params.source_path = normalized_source;

    // Validate source_path exists: remote tenants use StorageBackend,
    // local tenants use the namespace filesystem check.
    let tenant = match state.tenant_repository.get_tenant(owner_id).await {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": {"code": "NOT_FOUND", "message": "Tenant not found"}
                })),
            )
                .into_response()
        }
    };

    if tenant.is_remote() {
        // Remote tenant: StorageProvider must be connected
        let storage_id = owner_id.simple_string();
        if !state.workspace_service.storage_router().has_override(&storage_id) {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": {"code": "SERVICE_UNAVAILABLE", "message": "远程存储未连接，StorageProvider 尚未上线"}
                })),
            )
                .into_response();
        }

        // Check source_path exists via StorageBackend (gRPC to StorageProvider)
        match state
            .workspace_service
            .storage()
            .stat(&storage_id, &params.source_path)
            .await
        {
            Ok(stat) if stat.file_type == crate::infra::storage::FileType::Directory => {}
            Ok(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": {"code": "BAD_REQUEST", "message": "source_path is not a directory in remote storage"}
                    })),
                )
                    .into_response()
            }
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": {"code": "BAD_REQUEST", "message": "source_path directory does not exist in remote storage"}
                    })),
                )
                    .into_response()
            }
        }
    } else {
        // Local tenant: filesystem check
        let ns_dir = state.namespace_service.namespace_path(owner_id);
        let source_dir = ns_dir.join(&params.source_path);
        if !source_dir.exists() || !source_dir.is_dir() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {"code": "BAD_REQUEST", "message": "source_path directory does not exist in namespace"}
                })),
            )
                .into_response();
        }
    }

    match state.share_repository.create_share(&params).await {
        Ok(share) => {
            state.audit_service.log(
                &auth,
                "share.create",
                "share",
                share.id,
                &share.name,
                serde_json::json!({"source_path": share.source_path}),
            );
            (
                StatusCode::CREATED,
                Json(serde_json::json!({"share": share})),
            )
                .into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/shares
pub async fn list_shares(
    State(state): State<AppState>,
    Query(filter): Query<ShareFilter>,
    Query(pagination): Query<Pagination>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED"}})),
            )
                .into_response()
        }
    };

    if auth.is_admin() {
        // Admin sees all shares with pagination
        match state.share_repository.list_shares(filter, pagination).await {
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
    } else if let Some(tid) = auth.tenant_id() {
        // Tenant sees accessible shares with pagination
        match state
            .share_repository
            .list_accessible_shares_paginated(tid, pagination)
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
    } else {
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": {"code": "FORBIDDEN"}})),
        )
            .into_response()
    }
}

/// GET /api/v1/shares/:id
pub async fn get_share(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED"}})),
            )
                .into_response()
        }
    };

    let share_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, share_id, PermissionLevel::Read)
        .await
    {
        return super::tenant_handler::error_response(e);
    }

    match state.share_repository.get_share(share_id).await {
        Ok(share) => (StatusCode::OK, Json(serde_json::json!({"share": share}))).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// PUT /api/v1/shares/:id
pub async fn update_share(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED"}})),
            )
                .into_response()
        }
    };

    let share_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, share_id, PermissionLevel::Admin)
        .await
    {
        return super::tenant_handler::error_response(e);
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST"}})),
            )
                .into_response()
        }
    };
    let params: UpdateShareParams = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}}),
            ),
        )
            .into_response(),
    };

    match state.share_repository.update_share(share_id, params).await {
        Ok(share) => {
            state.audit_service.log(
                &auth,
                "share.update",
                "share",
                share.id,
                &share.name,
                serde_json::json!({}),
            );
            (StatusCode::OK, Json(serde_json::json!({"share": share}))).into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// DELETE /api/v1/shares/:id
pub async fn delete_share(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED"}})),
            )
                .into_response()
        }
    };

    let share_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, share_id, PermissionLevel::Admin)
        .await
    {
        return super::tenant_handler::error_response(e);
    }

    // Fetch the share before deletion to capture the name for audit logging
    let share_name = match state.share_repository.get_share(share_id).await {
        Ok(share) => share.name,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    match state.share_repository.delete_share(share_id).await {
        Ok(()) => {
            state.audit_service.log(
                &auth,
                "share.delete",
                "share",
                share_id,
                &share_name,
                serde_json::json!({}),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

fn bad_request(msg: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": msg}})),
    )
        .into_response()
}
