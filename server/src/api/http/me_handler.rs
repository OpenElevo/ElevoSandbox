//! Tenant self-service /me endpoints
//!
//! These endpoints allow authenticated tenants to access their own resources.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use crate::domain::auth::AuthContext;
use crate::service::path_security;
use crate::AppState;

use super::workspace::{
    FileInfoResponse, ListFilesResponse, PathQuery, ReadFileResponse, WriteFileRequest,
};

/// GET /api/v1/me — current tenant info
pub async fn get_me(
    State(state): State<AppState>,
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

    let tenant_id = match auth.tenant_id() {
        Some(id) => id,
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

    match state.tenant_repository.get_tenant(tenant_id).await {
        Ok(tenant) => (
            StatusCode::OK,
            Json(serde_json::json!({ "tenant": tenant })),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/me/files — browse own namespace files
pub async fn list_my_files(
    State(state): State<AppState>,
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

    let tenant_id = match auth.tenant_id() {
        Some(id) => id,
        None => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "Admin does not have a namespace"}})),
            )
                .into_response();
        }
    };

    let safe_path = match path_security::normalize_path(&query.path) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => return super::tenant_handler::error_response(e),
    };

    let storage_id = format!("namespaces/{}", tenant_id);
    match state
        .workspace_service
        .storage()
        .list_dir(&storage_id, &safe_path)
        .await
    {
        Ok(entries) => {
            let responses: Vec<FileInfoResponse> = entries
                .into_iter()
                .map(|s| {
                    let f = crate::service::workspace::FileInfo::from(s);
                    FileInfoResponse {
                        name: f.name,
                        path: f.path,
                        file_type: f.file_type,
                        size: f.size,
                        modified_at: f.modified_at.map(|t| t.to_rfc3339()),
                    }
                })
                .collect();
            (StatusCode::OK, Json(ListFilesResponse { files: responses })).into_response()
        }
        Err(e) => super::namespace_handler::storage_error_response(e),
    }
}

/// GET /api/v1/me/sandboxes — list sandboxes in my namespace
pub async fn list_my_sandboxes(
    State(state): State<AppState>,
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

    let tenant_id = match auth.tenant_id() {
        Some(id) => id,
        None => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "Admin does not have a namespace"}})),
            )
                .into_response();
        }
    };

    match state
        .sandbox_service
        .list_by_namespace(tenant_id, None)
        .await
    {
        Ok(sandboxes) => {
            let total = sandboxes.len();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "sandboxes": sandboxes, "total": total })),
            )
                .into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// GET /api/v1/me/shares — list shares I own
pub async fn list_my_shares(
    State(state): State<AppState>,
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

    let tenant_id = match auth.tenant_id() {
        Some(id) => id,
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

/// Helper: resolve tenant namespace storage ID from auth, or return FORBIDDEN response
fn resolve_tenant_storage_id(auth: &AuthContext) -> Result<String, axum::response::Response> {
    match auth.tenant_id() {
        Some(id) => Ok(format!("namespaces/{}", id)),
        None => Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "权限不足"}})),
        )
            .into_response()),
    }
}

/// GET /api/v1/me/files/*path — read file content from own namespace
pub async fn read_my_file(
    State(state): State<AppState>,
    Path(path): Path<String>,
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

    let storage_id = match resolve_tenant_storage_id(auth) {
        Ok(id) => id,
        Err(r) => return r,
    };

    let safe_path = match path_security::normalize_path(&path) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => return super::tenant_handler::error_response(e),
    };

    match state.workspace_service.storage().read_file(&storage_id, &safe_path).await {
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

/// PUT /api/v1/me/files/*path — write file in own namespace
pub async fn write_my_file(
    State(state): State<AppState>,
    Path(path): Path<String>,
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

    let storage_id = match resolve_tenant_storage_id(&auth) {
        Ok(id) => id,
        Err(r) => return r,
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
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("Invalid JSON: {}", e)}})),
            )
                .into_response()
        }
    };

    let safe_path = match path_security::normalize_path(&path) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => return super::tenant_handler::error_response(e),
    };

    match state
        .workspace_service
        .storage()
        .write_file(&storage_id, &safe_path, req.content.as_bytes())
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "path": safe_path})),
        )
            .into_response(),
        Err(e) => super::namespace_handler::storage_error_response(e),
    }
}

/// POST /api/v1/me/files/*path — create file or directory in own namespace
pub async fn create_my_file(
    State(state): State<AppState>,
    Path(path): Path<String>,
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

    let storage_id = match resolve_tenant_storage_id(&auth) {
        Ok(id) => id,
        Err(r) => return r,
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

    let safe_path = match path_security::normalize_path(&path) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => return super::tenant_handler::error_response(e),
    };

    // Determine if this is a mkdir or a file write based on body
    // If body is empty or contains a "directory": true flag → mkdir
    // Otherwise treat as file write with content
    #[derive(serde::Deserialize)]
    struct CreateRequest {
        #[serde(default)]
        directory: bool,
        content: Option<String>,
    }

    let storage = state.workspace_service.storage();

    if body.is_empty() {
        // Empty body → create directory
        match storage.mkdir(&storage_id, &safe_path, true).await {
            Ok(()) => (
                StatusCode::CREATED,
                Json(serde_json::json!({"success": true, "path": safe_path})),
            )
                .into_response(),
            Err(e) => super::namespace_handler::storage_error_response(e),
        }
    } else {
        let req: CreateRequest = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("Invalid JSON: {}", e)}})),
                )
                    .into_response()
            }
        };

        if req.directory {
            match storage.mkdir(&storage_id, &safe_path, true).await {
                Ok(()) => (
                    StatusCode::CREATED,
                    Json(serde_json::json!({"success": true, "path": safe_path})),
                )
                    .into_response(),
                Err(e) => super::namespace_handler::storage_error_response(e),
            }
        } else {
            let content = req.content.unwrap_or_default();
            match storage
                .write_file(&storage_id, &safe_path, content.as_bytes())
                .await
            {
                Ok(()) => (
                    StatusCode::CREATED,
                    Json(serde_json::json!({"success": true, "path": safe_path})),
                )
                    .into_response(),
                Err(e) => super::namespace_handler::storage_error_response(e),
            }
        }
    }
}

/// Query parameters for delete operations via path route
#[derive(Debug, serde::Deserialize)]
pub struct RecursiveQuery {
    pub recursive: Option<String>,
}

/// DELETE /api/v1/me/files/*path — delete file or directory in own namespace
pub async fn delete_my_file(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(query): Query<RecursiveQuery>,
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

    let storage_id = match resolve_tenant_storage_id(auth) {
        Ok(id) => id,
        Err(r) => return r,
    };

    let safe_path = match path_security::normalize_path(&path) {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(e) => return super::tenant_handler::error_response(e),
    };

    let recursive = query.recursive.as_deref() == Some("true");
    let storage = state.workspace_service.storage();

    let result = match storage.stat(&storage_id, &safe_path).await {
        Ok(stat) if stat.file_type == crate::infra::storage::FileType::Directory => {
            storage.remove_dir(&storage_id, &safe_path, recursive).await
        }
        Ok(_) => storage.remove_file(&storage_id, &safe_path).await,
        Err(e) => Err(e),
    };

    match result {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"success": true, "path": safe_path})),
        )
            .into_response(),
        Err(e) => super::namespace_handler::storage_error_response(e),
    }
}

/// GET /api/v1/me/accessible-shares — list shares I have access to
pub async fn list_my_accessible_shares(
    State(state): State<AppState>,
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

    let tenant_id = match auth.tenant_id() {
        Some(id) => id,
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
        .list_accessible_shares(tenant_id)
        .await
    {
        Ok(shares) => {
            let total = shares.len() as i64;
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "items": shares,
                    "total": total,
                    "page": 1,
                    "page_size": total
                })),
            )
                .into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}
