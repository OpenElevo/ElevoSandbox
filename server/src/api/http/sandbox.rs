//! Sandbox HTTP handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::auth::AuthContext;
use crate::domain::sandbox::{CreateSandboxParams, SandboxState};
use crate::domain::share::MountRequest;
use crate::{AppState, Result};

/// Create sandbox request
#[derive(Debug, Deserialize)]
pub struct CreateSandboxRequest {
    pub namespace_id: Option<String>,
    pub root_path: Option<String>,
    pub template: Option<String>,
    pub name: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub timeout: Option<u64>,
    #[serde(default)]
    pub mounts: Vec<MountRequest>,
}

/// Sandbox response
#[derive(Debug, Serialize)]
pub struct SandboxResponse {
    pub id: String,
    pub namespace_id: Option<String>,
    pub root_path: String,
    pub name: Option<String>,
    pub template: String,
    pub state: String,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub created_at: String,
    pub updated_at: String,
    pub timeout: Option<u64>,
    pub error_message: Option<String>,
}

/// List sandboxes response
#[derive(Debug, Serialize)]
pub struct ListSandboxesResponse {
    pub sandboxes: Vec<SandboxResponse>,
    pub total: usize,
}

/// List query parameters
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub state: Option<String>,
}

/// Delete query parameters
#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    pub force: Option<String>,
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
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED"}})),
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
        tid.to_string()
    } else if auth.is_admin() {
        match &req.namespace_id {
            Some(nid) => nid.clone(),
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
            Json(serde_json::json!({"error": {"code": "FORBIDDEN"}})),
        )
            .into_response();
    };

    let params = CreateSandboxParams {
        workspace_id: None,
        namespace_id: Some(namespace_id),
        root_path: req.root_path.unwrap_or_else(|| "/".to_string()),
        template: req.template,
        name: req.name,
        env: req.env,
        metadata: req.metadata,
        timeout: req.timeout,
        mounts: req.mounts,
    };

    match state.sandbox_service.create(params).await {
        Ok(sandbox) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "sandbox": SandboxResponse {
                    id: sandbox.id,
                    namespace_id: sandbox.namespace_id,
                    root_path: sandbox.root_path,
                    name: sandbox.name,
                    template: sandbox.template,
                    state: sandbox.state.as_str().to_string(),
                    env: Some(sandbox.env),
                    metadata: Some(sandbox.metadata),
                    created_at: sandbox.created_at.to_rfc3339(),
                    updated_at: sandbox.updated_at.to_rfc3339(),
                    timeout: Some(sandbox.timeout),
                    error_message: sandbox.error_message,
                }
            })),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}

/// Get a sandbox by ID
pub async fn get_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SandboxResponse>> {
    let sandbox = state.sandbox_service.get(&id).await?;

    Ok(Json(SandboxResponse {
        id: sandbox.id,
        namespace_id: sandbox.namespace_id,
        root_path: sandbox.root_path,
        name: sandbox.name,
        template: sandbox.template,
        state: sandbox.state.as_str().to_string(),
        env: Some(sandbox.env),
        metadata: Some(sandbox.metadata),
        created_at: sandbox.created_at.to_rfc3339(),
        updated_at: sandbox.updated_at.to_rfc3339(),
        timeout: Some(sandbox.timeout),
        error_message: sandbox.error_message,
    }))
}

/// List all sandboxes
pub async fn list_sandboxes(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListSandboxesResponse>> {
    let state_filter = query.state.and_then(|s| match s.as_str() {
        "starting" => Some(SandboxState::Starting),
        "running" => Some(SandboxState::Running),
        "stopping" => Some(SandboxState::Stopping),
        "stopped" => Some(SandboxState::Stopped),
        "error" => Some(SandboxState::Error),
        _ => None,
    });

    let sandboxes = state.sandbox_service.list(state_filter).await?;
    let total = sandboxes.len();

    let responses: Vec<SandboxResponse> = sandboxes
        .into_iter()
        .map(|s| SandboxResponse {
            id: s.id,
            namespace_id: s.namespace_id,
            root_path: s.root_path,
            name: s.name,
            template: s.template,
            state: s.state.as_str().to_string(),
            env: Some(s.env),
            metadata: Some(s.metadata),
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
            timeout: Some(s.timeout),
            error_message: s.error_message,
        })
        .collect();

    Ok(Json(ListSandboxesResponse {
        sandboxes: responses,
        total,
    }))
}

/// Delete a sandbox
pub async fn delete_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DeleteQuery>,
) -> Result<Json<serde_json::Value>> {
    let force = query.force.map(|f| f == "true").unwrap_or(false);
    state.sandbox_service.delete(&id, force).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}
