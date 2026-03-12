//! Tenant self-service /me endpoints
//!
//! These endpoints allow authenticated tenants to access their own resources.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use crate::domain::auth::AuthContext;
use crate::AppState;

use super::workspace::{FileInfoResponse, ListFilesResponse, PathQuery};

/// GET /api/v1/me — current tenant info
pub async fn get_me(
    State(state): State<AppState>,
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

    let tenant_id = match auth.tenant_id() {
        Some(id) => id.to_string(),
        None => {
            // Admin calling /me — return admin info
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "type": "admin",
                    "identity": auth.identity
                })),
            )
                .into_response();
        }
    };

    match state.tenant_repository.get_tenant(&tenant_id).await {
        Ok(tenant) => (StatusCode::OK, Json(serde_json::json!({ "tenant": tenant }))).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/me/files — browse own namespace files
pub async fn list_my_files(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
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

    let tenant_id = match auth.tenant_id() {
        Some(id) => id.to_string(),
        None => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "Admin does not have a namespace"}})),
            )
                .into_response();
        }
    };

    match state
        .workspace_service
        .list_files(&tenant_id, &query.path)
        .await
    {
        Ok(files) => {
            let responses: Vec<FileInfoResponse> = files
                .into_iter()
                .map(|f| FileInfoResponse {
                    name: f.name,
                    path: f.path,
                    file_type: f.file_type,
                    size: f.size,
                    modified_at: f.modified_at.map(|t| t.to_rfc3339()),
                })
                .collect();
            (
                StatusCode::OK,
                Json(ListFilesResponse { files: responses }),
            )
                .into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/me/sandboxes — list sandboxes in my namespace
pub async fn list_my_sandboxes(
    State(state): State<AppState>,
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

    let _tenant_id = match auth.tenant_id() {
        Some(id) => id.to_string(),
        None => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "Admin does not have a namespace"}})),
            )
                .into_response();
        }
    };

    // For now, list all sandboxes (will be filtered by namespace_id in Phase 2b)
    match state.sandbox_service.list(None).await {
        Ok(sandboxes) => (StatusCode::OK, Json(serde_json::json!({ "sandboxes": sandboxes }))).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/me/shares — list shares I own
pub async fn list_my_shares(
    State(state): State<AppState>,
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

    let tenant_id = match auth.tenant_id() {
        Some(id) => id.to_string(),
        None => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "Admin does not have a namespace"}})),
            )
                .into_response();
        }
    };

    use crate::domain::share::ShareFilter;
    use crate::domain::tenant::Pagination;

    let filter = ShareFilter {
        owner_tenant_id: Some(tenant_id),
        ..Default::default()
    };
    let pagination = Pagination::default();

    match state.share_repository.list_shares(filter, pagination).await {
        Ok((shares, total)) => (
            StatusCode::OK,
            Json(serde_json::json!({"shares": shares, "total": total})),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/me/accessible-shares — list shares I have access to
pub async fn list_my_accessible_shares(
    State(state): State<AppState>,
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

    let tenant_id = match auth.tenant_id() {
        Some(id) => id.to_string(),
        None => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "Admin does not have a namespace"}})),
            )
                .into_response();
        }
    };

    match state
        .share_repository
        .list_accessible_shares(&tenant_id)
        .await
    {
        Ok(shares) => (
            StatusCode::OK,
            Json(serde_json::json!({"shares": shares, "total": shares.len()})),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}
