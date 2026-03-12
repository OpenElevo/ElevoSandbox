//! Sandbox HTTP handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::auth::AuthContext;
use crate::domain::sandbox::{CreateSandboxParams, SandboxState};
use crate::domain::share::MountRequest;
use crate::AppState;

/// Create sandbox request
#[derive(Debug, Deserialize)]
pub struct CreateSandboxRequest {
    pub namespace_id: Option<String>,
    pub root_path: Option<String>,
    pub template: Option<String>,
    pub name: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub timeout: Option<i32>,
    #[serde(default)]
    pub mounts: Vec<MountRequest>,
}

/// Sandbox response
#[derive(Debug, Serialize)]
pub struct SandboxResponse {
    pub id: String,
    pub namespace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace_name: Option<String>,
    pub root_path: String,
    pub name: Option<String>,
    pub template: String,
    pub state: String,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub created_at: String,
    pub updated_at: String,
    pub timeout: i32,
    pub error_message: Option<String>,
}

/// List sandboxes response (paginated)
#[derive(Debug, Serialize)]
pub struct ListSandboxesResponse {
    pub items: Vec<SandboxResponse>,
    pub total: usize,
    pub page: u32,
    pub page_size: u32,
}

/// List query parameters
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub state: Option<String>,
    pub namespace_id: Option<uuid::Uuid>,
    pub name: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Delete query parameters
#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    pub force: Option<String>,
}

fn sandbox_to_response(sandbox: crate::domain::sandbox::Sandbox) -> SandboxResponse {
    SandboxResponse {
        id: sandbox.id.to_string(),
        namespace_id: sandbox.namespace_id.to_string(),
        namespace_name: sandbox.namespace_name,
        root_path: sandbox.root_path,
        name: sandbox.name,
        template: sandbox.template,
        state: sandbox.state.as_str().to_string(),
        env: Some(sandbox.env),
        metadata: Some(sandbox.metadata),
        created_at: sandbox.created_at.to_rfc3339(),
        updated_at: sandbox.updated_at.to_rfc3339(),
        timeout: sandbox.timeout,
        error_message: sandbox.error_message,
    }
}

/// Create a new sandbox
pub async fn create_sandbox(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}})),
            )
                .into_response()
        }
    };

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

    let req: CreateSandboxRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}})),
            )
                .into_response()
        }
    };

    // Determine namespace_id: tenant uses own ID, admin must specify
    let namespace_id = if let Some(tid) = auth.tenant_id() {
        tid
    } else if auth.is_admin() {
        match &req.namespace_id {
            Some(nid) => match Uuid::parse_str(nid) {
                Ok(u) => u,
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "Invalid namespace_id"}})),
                    )
                        .into_response()
                }
            },
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "Admin must specify namespace_id"}})),
                )
                    .into_response()
            }
        }
    } else {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "权限不足"}})),
        )
            .into_response();
    };

    let params = CreateSandboxParams {
        namespace_id,
        root_path: req.root_path.unwrap_or_else(|| "/".to_string()),
        template: req.template,
        name: req.name,
        env: req.env,
        metadata: req.metadata,
        timeout: req.timeout,
        mounts: req.mounts,
    };

    match state.sandbox_service.create(params).await {
        Ok(sandbox) => {
            state.audit_service.log(
                &auth,
                "sandbox.create",
                "sandbox",
                sandbox.id,
                sandbox.name.as_deref().unwrap_or(""),
                serde_json::json!({
                    "namespace_id": sandbox.namespace_id,
                    "template": sandbox.template,
                }),
            );
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "sandbox": sandbox_to_response(sandbox)
                })),
            )
                .into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// Get a sandbox by ID
pub async fn get_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}})),
            )
                .into_response()
        }
    };

    let sandbox_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "Invalid sandbox ID"}})),
            )
                .into_response()
        }
    };

    let sandbox = match state.sandbox_service.get(sandbox_id).await {
        Ok(s) => s,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    // Tenants can only view sandboxes in their own namespace
    if let Some(tenant_id) = auth.tenant_id() {
        if sandbox.namespace_id != tenant_id {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": {"code": "NOT_FOUND", "message": "Sandbox not found"}})),
            )
                .into_response();
        }
    }

    (StatusCode::OK, Json(serde_json::json!(sandbox_to_response(sandbox)))).into_response()
}

/// List all sandboxes
pub async fn list_sandboxes(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}})),
            )
                .into_response()
        }
    };

    let state_filter = query.state.as_deref().and_then(|s| match s {
        "starting" => Some(SandboxState::Starting),
        "running" => Some(SandboxState::Running),
        "stopping" => Some(SandboxState::Stopping),
        "stopped" => Some(SandboxState::Stopped),
        "error" => Some(SandboxState::Error),
        _ => None,
    });

    // Determine which namespace(s) to list:
    // - Tenants always see only their own namespace
    // - Admins see all (or optionally filter by namespace_id query param)
    let sandboxes = if let Some(tenant_id) = auth.tenant_id() {
        match state
            .sandbox_service
            .list_by_namespace(tenant_id, state_filter)
            .await
        {
            Ok(s) => s,
            Err(e) => return super::tenant_handler::error_response(e),
        }
    } else {
        // Admin path: optionally filter by namespace_id query param
        if let Some(ns_id) = query.namespace_id {
            match state
                .sandbox_service
                .list_by_namespace(ns_id, state_filter)
                .await
            {
                Ok(s) => s,
                Err(e) => return super::tenant_handler::error_response(e),
            }
        } else {
            match state.sandbox_service.list(state_filter).await {
                Ok(s) => s,
                Err(e) => return super::tenant_handler::error_response(e),
            }
        }
    };

    // Apply optional name filter (case-insensitive substring match)
    let filtered: Vec<_> = if let Some(ref name_filter) = query.name {
        let lower = name_filter.to_lowercase();
        sandboxes
            .into_iter()
            .filter(|sb| {
                sb.name
                    .as_deref()
                    .map(|n| n.to_lowercase().contains(&lower))
                    .unwrap_or(false)
            })
            .collect()
    } else {
        sandboxes
    };

    // Pagination
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let total = filtered.len();
    let offset = ((page - 1) * page_size) as usize;

    let items: Vec<SandboxResponse> = filtered
        .into_iter()
        .skip(offset)
        .take(page_size as usize)
        .map(sandbox_to_response)
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!(ListSandboxesResponse {
            items,
            total,
            page,
            page_size,
        })),
    )
        .into_response()
}

/// Batch delete sandboxes request
#[derive(Debug, Deserialize)]
pub struct BatchDeleteRequest {
    #[serde(default)]
    pub ids: Vec<String>,
    #[serde(default)]
    pub filter: Option<BatchDeleteFilter>,
}

#[derive(Debug, Deserialize)]
pub struct BatchDeleteFilter {
    pub state: Option<String>,
    pub namespace_id: Option<String>,
}

/// Batch delete sandboxes result
#[derive(Debug, Serialize)]
pub struct BatchDeleteResponse {
    pub succeeded: Vec<String>,
    pub failed: Vec<BatchDeleteError>,
}

#[derive(Debug, Serialize)]
pub struct BatchDeleteError {
    pub id: String,
    pub error: String,
}

/// Batch delete sandboxes
pub async fn batch_delete_sandboxes(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}})),
            )
                .into_response()
        }
    };

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

    let req: BatchDeleteRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": format!("{}", e)}})),
            )
                .into_response()
        }
    };

    // Validate: ids and filter are mutually exclusive
    if !req.ids.is_empty() && req.filter.is_some() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"code": "VALIDATION_ERROR", "message": "ids and filter are mutually exclusive"}})),
        )
            .into_response();
    }

    if req.ids.is_empty() && req.filter.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"code": "VALIDATION_ERROR", "message": "either ids or filter is required"}})),
        )
            .into_response();
    }

    // Collect sandbox IDs to delete
    let sandbox_ids: Vec<(Uuid, String)> = if !req.ids.is_empty() {
        if req.ids.len() > 100 {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "VALIDATION_ERROR", "message": "max 100 IDs per batch"}})),
            )
                .into_response();
        }

        let mut parsed = Vec::new();
        for id_str in &req.ids {
            match Uuid::parse_str(id_str) {
                Ok(u) => parsed.push((u, id_str.clone())),
                Err(_) => parsed.push((Uuid::nil(), id_str.clone())), // Will fail at delete
            }
        }
        parsed
    } else if let Some(filter) = &req.filter {
        // Filter-based: list matching sandboxes
        let state_filter = filter.state.as_deref().and_then(|s| match s {
            "starting" => Some(SandboxState::Starting),
            "running" => Some(SandboxState::Running),
            "stopping" => Some(SandboxState::Stopping),
            "stopped" => Some(SandboxState::Stopped),
            "error" => Some(SandboxState::Error),
            _ => None,
        });

        // For non-admin tenants, constrain the filter to their own namespace only.
        // Admins may optionally specify a namespace_id filter to narrow the scope.
        let effective_namespace: Option<Uuid> = if let Some(tenant_id) = auth.tenant_id() {
            // Tenant: always restrict to own namespace, ignore any namespace_id in filter
            Some(tenant_id)
        } else if let Some(ns_id) = &filter.namespace_id {
            Uuid::parse_str(ns_id).ok()
        } else {
            None
        };

        let candidates = if let Some(ns_uuid) = effective_namespace {
            state
                .sandbox_service
                .list_by_namespace(ns_uuid, state_filter)
                .await
        } else {
            state.sandbox_service.list(state_filter).await
        };

        match candidates {
            Ok(sandboxes) => {
                let mut filtered: Vec<_> = sandboxes.into_iter().collect();
                // Limit to 100
                filtered.truncate(100);
                filtered.iter().map(|sb| (sb.id, sb.id.to_string())).collect()
            }
            Err(e) => {
                return super::tenant_handler::error_response(e);
            }
        }
    } else {
        Vec::new()
    };

    // Delete each sandbox, collecting results
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for (uuid, id_str) in sandbox_ids {
        if uuid.is_nil() {
            failed.push(BatchDeleteError {
                id: id_str,
                error: "invalid UUID".to_string(),
            });
            continue;
        }

        // Fetch sandbox for permission check and audit logging
        let sandbox = match state.sandbox_service.get(uuid).await {
            Ok(sb) => sb,
            Err(e) => {
                failed.push(BatchDeleteError {
                    id: id_str,
                    error: e.to_string(),
                });
                continue;
            }
        };

        // Per-item permission check: admins can delete any sandbox; tenants can only
        // delete sandboxes that belong to their own namespace.
        if !auth.is_admin() {
            if let Some(tenant_id) = auth.tenant_id() {
                if sandbox.namespace_id != tenant_id {
                    failed.push(BatchDeleteError {
                        id: id_str,
                        error: "permission denied: sandbox belongs to a different namespace"
                            .to_string(),
                    });
                    continue;
                }
            }
        }

        let sandbox_name = sandbox.name.as_deref().unwrap_or("").to_string();
        let sandbox_namespace_id = sandbox.namespace_id;

        match state.sandbox_service.delete(uuid, true).await {
            Ok(()) => {
                state.audit_service.log(
                    &auth,
                    "sandbox.delete",
                    "sandbox",
                    uuid,
                    &sandbox_name,
                    serde_json::json!({
                        "namespace_id": sandbox_namespace_id,
                        "force": true,
                    }),
                );
                succeeded.push(id_str)
            }
            Err(e) => failed.push(BatchDeleteError {
                id: id_str,
                error: e.to_string(),
            }),
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!(BatchDeleteResponse { succeeded, failed })),
    )
        .into_response()
}

/// Delete a sandbox
pub async fn delete_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DeleteQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}})),
            )
                .into_response()
        }
    };

    let sandbox_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "Invalid sandbox ID"}})),
            )
                .into_response()
        }
    };

    // Fetch sandbox for permission check and audit logging
    let sandbox = match state.sandbox_service.get(sandbox_id).await {
        Ok(sb) => sb,
        Err(e) => return super::tenant_handler::error_response(e),
    };

    // Tenants can only delete sandboxes in their own namespace
    if !auth.is_admin() {
        if let Some(tenant_id) = auth.tenant_id() {
            if sandbox.namespace_id != tenant_id {
                return (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({"error": {"code": "NOT_FOUND", "message": "Sandbox not found"}})),
                )
                    .into_response();
            }
        }
    }

    let sandbox_name = sandbox.name.as_deref().unwrap_or("").to_string();
    let sandbox_namespace_id = sandbox.namespace_id;

    let force = query.force.map(|f| f == "true").unwrap_or(false);
    match state.sandbox_service.delete(sandbox_id, force).await {
        Ok(()) => {
            state.audit_service.log(
                &auth,
                "sandbox.delete",
                "sandbox",
                sandbox_id,
                &sandbox_name,
                serde_json::json!({
                    "namespace_id": sandbox_namespace_id,
                    "force": force,
                }),
            );
            (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response()
        }
        Err(e) => super::tenant_handler::error_response(e),
    }
}
