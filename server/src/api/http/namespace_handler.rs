//! Namespace file operation handlers
//!
//! These endpoints operate on files within a tenant's namespace directory.
//! Access: Namespace owner (tenant) or Admin.
//!
//! Unlike sandbox file operations, namespace files bypass the legacy
//! workspace DB table — they use the storage backend directly.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::domain::auth::AuthContext;
use crate::infra::storage::{FileType, StorageError};
use crate::service::path_security;
use crate::service::workspace::FileInfo;
use crate::AppState;

use super::workspace::{
    DeleteQuery, FileInfoResponse, ListFilesResponse, MkdirRequest, MoveRequest, PathQuery,
    ReadFileResponse, WriteFileRequest,
};

/// Check namespace access (owner or admin)
#[allow(clippy::result_large_err)]
fn check_namespace_access(
    auth: &AuthContext,
    namespace_id: &str,
) -> Result<(), axum::response::Response> {
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

/// Convert a StorageError into an HTTP response
pub(super) fn storage_error_response(err: StorageError) -> axum::response::Response {
    let (status, code, message) = match &err {
        StorageError::NotFound(p) => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("Not found: {}", p),
        ),
        StorageError::AlreadyExists(p) => (
            StatusCode::CONFLICT,
            "ALREADY_EXISTS",
            format!("Already exists: {}", p),
        ),
        StorageError::PermissionDenied(p) => (
            StatusCode::FORBIDDEN,
            "PERMISSION_DENIED",
            format!("Permission denied: {}", p),
        ),
        StorageError::PathTraversalDenied(p) => (
            StatusCode::FORBIDDEN,
            "PATH_NOT_ALLOWED",
            format!("Path traversal denied: {}", p),
        ),
        StorageError::IsADirectory(p) => (
            StatusCode::BAD_REQUEST,
            "IS_A_DIRECTORY",
            format!("Is a directory: {}", p),
        ),
        StorageError::NotADirectory(p) => (
            StatusCode::BAD_REQUEST,
            "NOT_A_DIRECTORY",
            format!("Not a directory: {}", p),
        ),
        StorageError::NotAFile(p) => (
            StatusCode::BAD_REQUEST,
            "NOT_A_FILE",
            format!("Not a file: {}", p),
        ),
        StorageError::DirectoryNotEmpty(p) => (
            StatusCode::CONFLICT,
            "DIRECTORY_NOT_EMPTY",
            format!("Directory not empty: {}", p),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            err.to_string(),
        ),
    };
    (
        status,
        Json(serde_json::json!({"error": {"code": code, "message": message}})),
    )
        .into_response()
}

/// GET /api/v1/namespaces/:id/files
pub async fn read_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth =
        match request.extensions().get::<AuthContext>() {
            Some(a) => a,
            None => return (
                StatusCode::UNAUTHORIZED,
                Json(
                    serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}),
                ),
            )
                .into_response(),
        };
    if let Err(e) = check_namespace_access(auth, &namespace_id) {
        return e;
    }

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let sid = storage_id(&namespace_id);
    match state.workspace_service.storage().read_file(&sid, &safe_path).await {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(content) => (StatusCode::OK, Json(ReadFileResponse { content })).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": {"code": "INTERNAL_ERROR", "message": format!("Invalid UTF-8: {}", e)}}))).into_response(),
        },
        Err(e) => storage_error_response(e),
    }
}

/// PUT /api/v1/namespaces/:id/files
pub async fn write_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth =
        match request.extensions().get::<AuthContext>() {
            Some(a) => a.clone(),
            None => return (
                StatusCode::UNAUTHORIZED,
                Json(
                    serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}),
                ),
            )
                .into_response(),
        };
    if let Err(e) = check_namespace_access(&auth, &namespace_id) {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 1024 * 10).await {
        Ok(b) => b,
        Err(_) => return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "Body too large"}}),
            ),
        )
            .into_response(),
    };
    let req: WriteFileRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("Invalid JSON: {}", e)}}))).into_response(),
    };

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let sid = storage_id(&namespace_id);
    match state
        .workspace_service
        .storage()
        .write_file(&sid, &safe_path, req.content.as_bytes())
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "path": safe_path})),
        )
            .into_response(),
        Err(e) => storage_error_response(e),
    }
}

/// DELETE /api/v1/namespaces/:id/files
pub async fn delete_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<DeleteQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth =
        match request.extensions().get::<AuthContext>() {
            Some(a) => a,
            None => return (
                StatusCode::UNAUTHORIZED,
                Json(
                    serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}),
                ),
            )
                .into_response(),
        };
    if let Err(e) = check_namespace_access(auth, &namespace_id) {
        return e;
    }

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let recursive = query.recursive.as_deref() == Some("true");
    let sid = storage_id(&namespace_id);
    let storage = state.workspace_service.storage();

    let result = match storage.stat(&sid, &safe_path).await {
        Ok(stat) if stat.file_type == FileType::Directory => {
            storage.remove_dir(&sid, &safe_path, recursive).await
        }
        Ok(_) => storage.remove_file(&sid, &safe_path).await,
        Err(e) => Err(e),
    };

    match result {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "path": safe_path})),
        )
            .into_response(),
        Err(e) => storage_error_response(e),
    }
}

/// GET /api/v1/namespaces/:id/files/list
pub async fn list_files(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth =
        match request.extensions().get::<AuthContext>() {
            Some(a) => a,
            None => return (
                StatusCode::UNAUTHORIZED,
                Json(
                    serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}),
                ),
            )
                .into_response(),
        };
    if let Err(e) = check_namespace_access(auth, &namespace_id) {
        return e;
    }

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let sid = storage_id(&namespace_id);
    match state
        .workspace_service
        .storage()
        .list_dir(&sid, &safe_path)
        .await
    {
        Ok(entries) => {
            let files: Vec<FileInfoResponse> = entries
                .into_iter()
                .map(|s| {
                    let info = FileInfo::from(s);
                    FileInfoResponse {
                        name: info.name,
                        path: info.path,
                        file_type: info.file_type,
                        size: info.size,
                        modified_at: info.modified_at.map(|t| t.to_rfc3339()),
                    }
                })
                .collect();
            (StatusCode::OK, Json(ListFilesResponse { files })).into_response()
        }
        Err(e) => storage_error_response(e),
    }
}

/// POST /api/v1/namespaces/:id/files/mkdir
pub async fn mkdir(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth =
        match request.extensions().get::<AuthContext>() {
            Some(a) => a.clone(),
            None => return (
                StatusCode::UNAUTHORIZED,
                Json(
                    serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}),
                ),
            )
                .into_response(),
        };
    if let Err(e) = check_namespace_access(&auth, &namespace_id) {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST"}})),
            )
                .into_response()
        }
    };
    let req: MkdirRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}}),
            ),
        )
            .into_response(),
    };

    let safe_path = match safe_path_string(&req.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let sid = storage_id(&namespace_id);
    match state
        .workspace_service
        .storage()
        .mkdir(&sid, &safe_path, true)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "path": safe_path})),
        )
            .into_response(),
        Err(e) => storage_error_response(e),
    }
}

/// POST /api/v1/namespaces/:id/files/move
pub async fn move_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth =
        match request.extensions().get::<AuthContext>() {
            Some(a) => a.clone(),
            None => return (
                StatusCode::UNAUTHORIZED,
                Json(
                    serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}),
                ),
            )
                .into_response(),
        };
    if let Err(e) = check_namespace_access(&auth, &namespace_id) {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST"}})),
            )
                .into_response()
        }
    };
    let req: MoveRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}}),
            ),
        )
            .into_response(),
    };

    let safe_source = match safe_path_string(&req.source) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let safe_dest = match safe_path_string(&req.destination) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let sid = storage_id(&namespace_id);
    let storage = state.workspace_service.storage();

    // Ensure parent directory exists (convenience for users)
    if let Some(parent) = std::path::Path::new(&safe_dest).parent() {
        let ps = parent.to_string_lossy();
        if !ps.is_empty() && ps != "." {
            let _ = storage.mkdir(&sid, &ps, true).await;
        }
    }

    match storage.rename(&sid, &safe_source, &safe_dest).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response(),
        Err(e) => storage_error_response(e),
    }
}

/// POST /api/v1/namespaces/:id/files/copy
pub async fn copy_file(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth =
        match request.extensions().get::<AuthContext>() {
            Some(a) => a.clone(),
            None => return (
                StatusCode::UNAUTHORIZED,
                Json(
                    serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}),
                ),
            )
                .into_response(),
        };
    if let Err(e) = check_namespace_access(&auth, &namespace_id) {
        return e;
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST"}})),
            )
                .into_response()
        }
    };
    let req: MoveRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}}),
            ),
        )
            .into_response(),
    };

    let safe_source = match safe_path_string(&req.source) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let safe_dest = match safe_path_string(&req.destination) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let sid = storage_id(&namespace_id);
    match state
        .workspace_service
        .storage()
        .copy(&sid, &safe_source, &safe_dest)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response(),
        Err(e) => storage_error_response(e),
    }
}

/// GET /api/v1/namespaces/:id/files/info
pub async fn get_file_info(
    State(state): State<AppState>,
    Path(namespace_id): Path<String>,
    Query(query): Query<PathQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth =
        match request.extensions().get::<AuthContext>() {
            Some(a) => a,
            None => return (
                StatusCode::UNAUTHORIZED,
                Json(
                    serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}}),
                ),
            )
                .into_response(),
        };
    if let Err(e) = check_namespace_access(auth, &namespace_id) {
        return e;
    }

    let safe_path = match safe_path_string(&query.path) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let sid = storage_id(&namespace_id);
    match state
        .workspace_service
        .storage()
        .stat(&sid, &safe_path)
        .await
    {
        Ok(stat) => {
            let info = FileInfo::from(stat);
            (
                StatusCode::OK,
                Json(FileInfoResponse {
                    name: info.name,
                    path: info.path,
                    file_type: info.file_type,
                    size: info.size,
                    modified_at: info.modified_at.map(|t| t.to_rfc3339()),
                }),
            )
                .into_response()
        }
        Err(e) => storage_error_response(e),
    }
}
