//! Share file operation handlers
//!
//! All paths are sanitized through `sanitize_share_path` to prevent directory traversal
//! attacks. User-provided paths are normalized and validated against the share's source_path
//! boundary, ensuring they cannot escape into the broader namespace.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::domain::auth::AuthContext;
use crate::domain::permission::PermissionLevel;
use crate::domain::UuidSimple;
use crate::service::path_security;
use crate::AppState;

use super::workspace::{
    FileInfoResponse, ListFilesResponse, PathQuery, ReadFileResponse, WriteFileRequest,
};

/// Resolve the storage backend ID and effective path for a share operation.
///
/// Returns `(storage_id, effective_path)` where:
/// - **Per-workspace StorageProvider**: `storage_id` is the workspace UUID (`source_path`),
///   and `effective_path` is relative to the workspace root (user-provided path only).
/// - **Legacy global StorageProvider**: `storage_id` is the tenant UUID, and
///   `effective_path` includes the `source_path` prefix.
///
/// This mirrors the resolution logic in `share_handler::create_share`.
fn resolve_storage_target(
    share: &crate::domain::share::Share,
    state: &AppState,
    user_path: &str,
    ns_root: &std::path::Path,
) -> Result<(String, String), crate::error::Error> {
    let tenant_id = share.owner_tenant_id.simple_string();

    if state.workspace_service.storage_router().has_override(&share.source_path) {
        // Per-workspace StorageProvider: source_path IS the workspace root.
        // The backend is already scoped to the workspace directory, so the
        // effective path is just the sanitized user path (no source_path prefix).
        let sanitized = path_security::sanitize_share_path(ns_root, &share.source_path, user_path)?;
        let relative = sanitized.strip_prefix(ns_root).unwrap_or(&sanitized);
        let source_prefix = format!("{}/", share.source_path);
        let effective = relative
            .to_string_lossy()
            .strip_prefix(&source_prefix)
            .unwrap_or("")
            .to_string();
        Ok((share.source_path.clone(), effective))
    } else if state.workspace_service.storage_router().has_override(&tenant_id) {
        // Legacy global StorageProvider: source_path is a subdirectory within the tenant.
        let effective = resolve_share_path(ns_root, &share.source_path, user_path)?;
        Ok((tenant_id, effective))
    } else {
        // No remote override — use local namespaced storage.
        let effective = resolve_share_path(ns_root, &share.source_path, user_path)?;
        Ok((format!("namespaces/{}", tenant_id), effective))
    }
}

/// Resolve an effective relative path for the workspace service by sanitizing
/// the user-provided path against the share's source directory.
///
/// Uses the centralized `sanitize_share_path` to:
/// - Reject null bytes
/// - Normalize `.` and `..` so they cannot escape the share boundary
/// - Return the path relative to the namespace root (suitable for workspace_service)
fn resolve_share_path(
    namespace_root: &std::path::Path,
    source_path: &str,
    user_path: &str,
) -> Result<String, crate::error::Error> {
    let full_path = path_security::sanitize_share_path(namespace_root, source_path, user_path)?;
    // Strip the namespace_root prefix to get the path relative to the namespace,
    // which is what workspace_service expects.
    let relative = full_path.strip_prefix(namespace_root).unwrap_or(&full_path);
    Ok(relative.to_string_lossy().into_owned())
}

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

    let share_uuid = match Uuid::parse_str(&share_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };

    let share = match state
        .permission_service
        .check_share_permission(&auth, share_uuid, PermissionLevel::Read)
        .await
    {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    let ns_root = state
        .namespace_service
        .namespace_path(share.owner_tenant_id);
    let (storage_id, effective_path) = match resolve_storage_target(
        &share,
        &state,
        &query.path,
        &ns_root,
    ) {
        Ok(v) => v,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    match state
        .workspace_service
        .storage()
        .read_file(&storage_id, &effective_path)
        .await
    {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(content) => (StatusCode::OK, Json(ReadFileResponse { content })).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": {"code": "INTERNAL_ERROR", "message": format!("Invalid UTF-8: {}", e)}})),
            ).into_response(),
        },
        Err(e) => super::namespace_handler::storage_error_response(e),
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

    let share_uuid = match Uuid::parse_str(&share_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };

    let share = match state
        .permission_service
        .check_share_permission(&auth, share_uuid, PermissionLevel::Write)
        .await
    {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };

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
        Err(e) => return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}}),
            ),
        )
            .into_response(),
    };

    let ns_root = state
        .namespace_service
        .namespace_path(share.owner_tenant_id);
    let (storage_id, effective_path) = match resolve_storage_target(
        &share,
        &state,
        &query.path,
        &ns_root,
    ) {
        Ok(v) => v,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    match state
        .workspace_service
        .storage()
        .write_file(&storage_id, &effective_path, req.content.as_bytes())
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "path": query.path})),
        )
            .into_response(),
        Err(e) => super::namespace_handler::storage_error_response(e),
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

    let share_uuid = match Uuid::parse_str(&share_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };

    let share = match state
        .permission_service
        .check_share_permission(&auth, share_uuid, PermissionLevel::Write)
        .await
    {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    let ns_root = state
        .namespace_service
        .namespace_path(share.owner_tenant_id);
    let (storage_id, effective_path) = match resolve_storage_target(
        &share,
        &state,
        &query.path,
        &ns_root,
    ) {
        Ok(v) => v,
        Err(e) => return super::tenant_handler::error_response(e),
    };
    let recursive = query.recursive.as_deref() == Some("true");

    let storage = state.workspace_service.storage();

    let result = match storage.stat(&storage_id, &effective_path).await {
        Ok(stat) if stat.file_type == crate::infra::storage::FileType::Directory => {
            storage
                .remove_dir(&storage_id, &effective_path, recursive)
                .await
        }
        Ok(_) => storage.remove_file(&storage_id, &effective_path).await,
        Err(e) => Err(e),
    };

    match result {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "path": query.path})),
        )
            .into_response(),
        Err(e) => super::namespace_handler::storage_error_response(e),
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

    let share_uuid = match Uuid::parse_str(&share_id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid share ID"),
    };

    let share = match state
        .permission_service
        .check_share_permission(&auth, share_uuid, PermissionLevel::Read)
        .await
    {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    let ns_root = state
        .namespace_service
        .namespace_path(share.owner_tenant_id);
    let (storage_id, effective_path) = match resolve_storage_target(
        &share,
        &state,
        &query.path,
        &ns_root,
    ) {
        Ok(v) => v,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    match state
        .workspace_service
        .storage()
        .list_dir(&storage_id, &effective_path)
        .await
    {
        Ok(entries) => {
            let files: Vec<FileInfoResponse> = entries
                .into_iter()
                .map(|s| {
                    let info = crate::service::workspace::FileInfo::from(s);
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
        Err(e) => super::namespace_handler::storage_error_response(e),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ns() -> &'static Path {
        Path::new("/data/namespaces/tenant1")
    }

    #[test]
    fn test_resolve_share_path_basic() {
        assert_eq!(
            resolve_share_path(ns(), "shared/data", "file.txt").unwrap(),
            "shared/data/file.txt"
        );
        // Empty user path → share root directory
        let empty_result = resolve_share_path(ns(), "shared/data", "").unwrap();
        assert!(
            empty_result.starts_with("shared/data"),
            "empty user_path should resolve to share root: {empty_result}"
        );
        // Root-only user path → share root directory
        let root_result = resolve_share_path(ns(), "shared/data", "/").unwrap();
        assert!(
            root_result.starts_with("shared/data"),
            "/ user_path should resolve to share root: {root_result}"
        );
    }

    #[test]
    fn test_resolve_share_path_traversal_rejected() {
        // `..` components must now be rejected, not silently neutralised
        assert!(resolve_share_path(ns(), "shared/data", "../../etc/passwd").is_err());
        assert!(resolve_share_path(ns(), "shared/data", "../secret").is_err());
    }

    #[test]
    fn test_resolve_share_path_dot_components() {
        // `.` is still fine (current directory reference)
        assert_eq!(
            resolve_share_path(ns(), "shared/data", "./file.txt").unwrap(),
            "shared/data/file.txt"
        );
        // Paths without `..` but with `.` still work
        assert_eq!(
            resolve_share_path(ns(), "shared/data", "a/./b/c").unwrap(),
            "shared/data/a/b/c"
        );
    }

    #[test]
    fn test_resolve_share_path_null_byte_rejected() {
        assert!(resolve_share_path(ns(), "shared/data", "foo\0bar").is_err());
    }
}
