//! Share file operation handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use crate::domain::auth::AuthContext;
use crate::domain::permission::PermissionLevel;
use crate::AppState;

use super::workspace::{
    FileInfoResponse, ListFilesResponse, PathQuery, ReadFileResponse, WriteFileRequest,
};

/// GET /api/v1/shares/:id/files
pub async fn read_share_file(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return unauthorized(),
    };

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, &share_id, PermissionLevel::Read)
        .await
    {
        return super::tenant_handler::error_response(e);
    }

    let share = match state.share_repository.get_share(&share_id).await {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    // Build the effective path: namespace_id as workspace key, source_path + user path
    let effective_path = build_share_path(&share.source_path, &query.path);

    match state
        .workspace_service
        .read_file_string(&share.owner_tenant_id, &effective_path)
        .await
    {
        Ok(content) => (StatusCode::OK, Json(ReadFileResponse { content })).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// PUT /api/v1/shares/:id/files
pub async fn write_share_file(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return unauthorized(),
    };

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, &share_id, PermissionLevel::Write)
        .await
    {
        return super::tenant_handler::error_response(e);
    }

    let share = match state.share_repository.get_share(&share_id).await {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 1024 * 10).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "Body too large"}})),
            )
                .into_response()
        }
    };
    let req: WriteFileRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}})),
            )
                .into_response()
        }
    };

    let effective_path = build_share_path(&share.source_path, &query.path);

    match state
        .workspace_service
        .write_file(&share.owner_tenant_id, &effective_path, req.content.as_bytes())
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "path": query.path})),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// DELETE /api/v1/shares/:id/files
pub async fn delete_share_file(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
    Query(query): Query<super::workspace::DeleteQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return unauthorized(),
    };

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, &share_id, PermissionLevel::Write)
        .await
    {
        return super::tenant_handler::error_response(e);
    }

    let share = match state.share_repository.get_share(&share_id).await {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    let effective_path = build_share_path(&share.source_path, &query.path);
    let recursive = query.recursive.as_deref() == Some("true");

    match state
        .workspace_service
        .delete_file(&share.owner_tenant_id, &effective_path, recursive)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "path": query.path})),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/shares/:id/files/list
pub async fn list_share_files(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return unauthorized(),
    };

    if let Err(e) = state
        .permission_service
        .check_share_permission(&auth, &share_id, PermissionLevel::Read)
        .await
    {
        return super::tenant_handler::error_response(e);
    }

    let share = match state.share_repository.get_share(&share_id).await {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    let effective_path = build_share_path(&share.source_path, &query.path);

    match state
        .workspace_service
        .list_files(&share.owner_tenant_id, &effective_path)
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
            (StatusCode::OK, Json(ListFilesResponse { files: responses })).into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// Build effective path by joining share source_path with user-provided relative path
fn build_share_path(source_path: &str, user_path: &str) -> String {
    let source = source_path.trim_matches('/');
    let user = user_path.trim_start_matches('/');
    if user.is_empty() {
        source.to_string()
    } else {
        format!("{}/{}", source, user)
    }
}

fn unauthorized() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": {"code": "UNAUTHORIZED"}})),
    )
        .into_response()
}
