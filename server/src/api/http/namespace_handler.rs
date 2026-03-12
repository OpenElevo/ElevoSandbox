//! Namespace file operation handlers
//!
//! These endpoints operate on files within a tenant's namespace directory.
//! Access: Namespace owner (tenant) or Admin.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::domain::auth::AuthContext;
use crate::service::path_security;
use crate::AppState;

use super::workspace::{
    DeleteQuery, FileInfoResponse, ListFilesResponse, MkdirRequest, MoveRequest, PathQuery,
    ReadFileResponse, WriteFileRequest,
};

/// Check namespace access (owner or admin)
#[allow(clippy::result_large_err)]
fn check_namespace_access(auth: &AuthContext, namespace_id: &str) -> Result<(), axum::response::Response> {
    // Admin can access any namespace
    if auth.is_admin() {
        return Ok(());
    }

    let uuid = Uuid::parse_str(namespace_id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": { "code": "BAD_REQUEST", "message": "Invalid namespace ID" }
            })),
        )
            .into_response()
    })?;

    if auth.is_namespace_owner(&uuid) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": { "code": "FORBIDDEN", "message": "Namespace access denied" }
            })),
        )
            .into_response())
    }
}

/// Convert a namespace_id (tenant UUID) to the storage workspace_id.
///
/// The LocalStorageBackend resolves paths as `base_dir / workspace_id / path`.
/// Namespace files are stored under `base_dir/namespaces/<tenant_id>/`, so we
/// prefix the namespace_id with "namespaces/" to get the correct storage path.
fn storage_id(namespace_id: &str) -> String {
    format!("namespaces/{}", namespace_id)
}

/// Normalize a user-provided path and convert it to an owned String, returning
/// an error response if the path contains `..` components or is otherwise invalid.
fn safe_path_string(raw: &str) -> Result<String, axum::response::Response> {
    path_security::normalize_path(raw)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| super::tenant_handler::error_response(e))
}

/// GET /api/v1/namespaces/:id/files
pub async fn read_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}))).into_response(),
    };
    if let Err(e) = check_namespace_access(auth, &namespace_id) {
        return e;
    }

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match state.workspace_service.read_file_string(&storage_id(&namespace_id), &safe_path).await {
        Ok(content) => (StatusCode::OK, Json(ReadFileResponse { content })).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// PUT /api/v1/namespaces/:id/files
pub async fn write_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}))).into_response(),
    };
    if let Err(e) = check_namespace_access(&auth, &namespace_id) {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 1024 * 10).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "Body too large"}}))).into_response(),
    };
    let req: WriteFileRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("Invalid JSON: {}", e)}}))).into_response(),
    };

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match state.workspace_service.write_file(&storage_id(&namespace_id), &safe_path, req.content.as_bytes()).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true, "path": safe_path}))).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// DELETE /api/v1/namespaces/:id/files
pub async fn delete_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<DeleteQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}))).into_response(),
    };
    if let Err(e) = check_namespace_access(auth, &namespace_id) {
        return e;
    }

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let recursive = query.recursive.as_deref() == Some("true");
    match state.workspace_service.delete_file(&storage_id(&namespace_id), &safe_path, recursive).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true, "path": safe_path}))).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/namespaces/:id/files/list
pub async fn list_files(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}))).into_response(),
    };
    if let Err(e) = check_namespace_access(auth, &namespace_id) {
        return e;
    }

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match state.workspace_service.list_files(&storage_id(&namespace_id), &safe_path).await {
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

/// POST /api/v1/namespaces/:id/files/mkdir
pub async fn mkdir(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}))).into_response(),
    };
    if let Err(e) = check_namespace_access(&auth, &namespace_id) {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": {"code": "BAD_REQUEST"}}))).into_response(),
    };
    let req: MkdirRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}}))).into_response(),
    };

    let safe_path = match safe_path_string(&req.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match state.workspace_service.mkdir(&storage_id(&namespace_id), &safe_path).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true, "path": safe_path}))).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// POST /api/v1/namespaces/:id/files/move
pub async fn move_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}))).into_response(),
    };
    if let Err(e) = check_namespace_access(&auth, &namespace_id) {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": {"code": "BAD_REQUEST"}}))).into_response(),
    };
    let req: MoveRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}}))).into_response(),
    };

    let safe_source = match safe_path_string(&req.source) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let safe_destination = match safe_path_string(&req.destination) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match state.workspace_service.move_file(&storage_id(&namespace_id), &safe_source, &safe_destination).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// POST /api/v1/namespaces/:id/files/copy
pub async fn copy_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}))).into_response(),
    };
    if let Err(e) = check_namespace_access(&auth, &namespace_id) {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": {"code": "BAD_REQUEST"}}))).into_response(),
    };
    let req: MoveRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}}))).into_response(),
    };

    let safe_source = match safe_path_string(&req.source) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let safe_destination = match safe_path_string(&req.destination) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match state.workspace_service.copy_file(&storage_id(&namespace_id), &safe_source, &safe_destination).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/namespaces/:id/files/info
pub async fn get_file_info(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a,
        None => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}))).into_response(),
    };
    if let Err(e) = check_namespace_access(auth, &namespace_id) {
        return e;
    }

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    match state.workspace_service.get_file_info(&storage_id(&namespace_id), &safe_path).await {
        Ok(info) => (StatusCode::OK, Json(FileInfoResponse {
            name: info.name,
            path: info.path,
            file_type: info.file_type,
            size: info.size,
            modified_at: info.modified_at.map(|t| t.to_rfc3339()),
        })).into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}
