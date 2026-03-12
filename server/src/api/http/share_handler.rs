//! Share management API handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use crate::domain::auth::AuthContext;
use crate::domain::permission::PermissionLevel;
use crate::domain::share::{CreateShareParams, ShareFilter, UpdateShareParams};
use crate::domain::tenant::Pagination;
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
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "Body too large"}})),
            )
                .into_response()
        }
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
        params.owner_tenant_id = tid.to_string();
    } else if !auth.is_admin() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": {"code": "FORBIDDEN"}})),
        )
            .into_response();
    }

    // Validate source_path exists on disk
    let ns_dir = state
        .namespace_service
        .namespace_path(&params.owner_tenant_id);
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

    match state.share_repository.create_share(&params).await {
        Ok(share) => {
            state.audit_service.log(
                &auth, "share.create", "share", &share.id, &share.name,
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
        // Admin sees all shares
        match state.share_repository.list_shares(filter, pagination).await {
            Ok((shares, total)) => (
                StatusCode::OK,
                Json(serde_json::json!({"shares": shares, "total": total})),
            )
                .into_response(),
            Err(e) => super::tenant_handler::error_response(e),
        }
    } else if let Some(tid) = auth.tenant_id() {
        // Tenant sees accessible shares
        match state
            .share_repository
            .list_accessible_shares(&tid.to_string())
            .await
        {
            Ok(shares) => (
                StatusCode::OK,
                Json(serde_json::json!({"shares": shares, "total": shares.len()})),
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

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, &id, PermissionLevel::Read)
        .await
    {
        return super::tenant_handler::error_response(e);
    }

    match state.share_repository.get_share(&id).await {
        Ok(share) => (
            StatusCode::OK,
            Json(serde_json::json!({"share": share})),
        )
            .into_response(),
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

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, &id, PermissionLevel::Admin)
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
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}})),
            )
                .into_response()
        }
    };

    match state.share_repository.update_share(&id, params).await {
        Ok(share) => {
            state.audit_service.log(
                &auth, "share.update", "share", &share.id, &share.name,
                serde_json::json!({}),
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({"share": share})),
            )
                .into_response()
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

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, &id, PermissionLevel::Admin)
        .await
    {
        return super::tenant_handler::error_response(e);
    }

    match state.share_repository.delete_share(&id).await {
        Ok(()) => {
            state.audit_service.log(
                &auth, "share.delete", "share", &id, "",
                serde_json::json!({}),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}
