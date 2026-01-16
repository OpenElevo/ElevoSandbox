//! Sandbox HTTP handlers

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::sandbox::{CreateSandboxParams, SandboxState};
use crate::{AppState, Result};

/// Create sandbox request
#[derive(Debug, Deserialize)]
pub struct CreateSandboxRequest {
    pub workspace_id: String,
    pub template: Option<String>,
    pub name: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub timeout: Option<u64>,
}

/// Sandbox response
#[derive(Debug, Serialize)]
pub struct SandboxResponse {
    pub id: String,
    pub workspace_id: String,
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
    Json(req): Json<CreateSandboxRequest>,
) -> Result<Json<SandboxResponse>> {
    let params = CreateSandboxParams {
        workspace_id: req.workspace_id,
        template: req.template,
        name: req.name,
        env: req.env,
        metadata: req.metadata,
        timeout: req.timeout,
    };

    let sandbox = state.sandbox_service.create(params).await?;

    Ok(Json(SandboxResponse {
        id: sandbox.id,
        workspace_id: sandbox.workspace_id,
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

/// Get a sandbox by ID
pub async fn get_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SandboxResponse>> {
    let sandbox = state.sandbox_service.get(&id).await?;

    Ok(Json(SandboxResponse {
        id: sandbox.id,
        workspace_id: sandbox.workspace_id,
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
            workspace_id: s.workspace_id,
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
