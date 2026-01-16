//! Workspace HTTP handlers

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{AppState, Result};

// ==================== Request/Response Types ====================

/// Create workspace request
#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: Option<String>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// Workspace response
#[derive(Debug, Serialize)]
pub struct WorkspaceResponse {
    pub id: String,
    pub name: Option<String>,
    pub nfs_url: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
    pub created_at: String,
    pub updated_at: String,
}

/// List workspaces response
#[derive(Debug, Serialize)]
pub struct ListWorkspacesResponse {
    pub workspaces: Vec<WorkspaceResponse>,
    pub total: usize,
}

/// File info response
#[derive(Debug, Serialize)]
pub struct FileInfoResponse {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub size: u64,
    pub modified_at: Option<String>,
}

/// List files response
#[derive(Debug, Serialize)]
pub struct ListFilesResponse {
    pub files: Vec<FileInfoResponse>,
}

/// Read file response
#[derive(Debug, Serialize)]
pub struct ReadFileResponse {
    pub content: String,
}

/// Write file request
#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    pub content: String,
}

/// Mkdir request
#[derive(Debug, Deserialize)]
pub struct MkdirRequest {
    pub path: String,
}

/// Move/Copy request
#[derive(Debug, Deserialize)]
pub struct MoveRequest {
    pub source: String,
    pub destination: String,
}

/// Path query parameter
#[derive(Debug, Deserialize)]
pub struct PathQuery {
    pub path: String,
}

/// Delete query parameter
#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    pub path: String,
    pub recursive: Option<String>,
}

// ==================== Workspace CRUD Handlers ====================

/// Create a new workspace
pub async fn create_workspace(
    State(state): State<AppState>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>> {
    let params = crate::domain::workspace::CreateWorkspaceParams {
        name: req.name,
        metadata: req.metadata,
    };

    let workspace = state.workspace_service.create(params).await?;

    Ok(Json(WorkspaceResponse {
        id: workspace.id,
        name: workspace.name,
        nfs_url: workspace.nfs_url,
        metadata: workspace.metadata,
        created_at: workspace.created_at.to_rfc3339(),
        updated_at: workspace.updated_at.to_rfc3339(),
    }))
}

/// Get a workspace by ID
pub async fn get_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<WorkspaceResponse>> {
    let workspace = state.workspace_service.get(&id).await?;

    Ok(Json(WorkspaceResponse {
        id: workspace.id,
        name: workspace.name,
        nfs_url: workspace.nfs_url,
        metadata: workspace.metadata,
        created_at: workspace.created_at.to_rfc3339(),
        updated_at: workspace.updated_at.to_rfc3339(),
    }))
}

/// List all workspaces
pub async fn list_workspaces(
    State(state): State<AppState>,
) -> Result<Json<ListWorkspacesResponse>> {
    let workspaces = state.workspace_service.list().await?;
    let total = workspaces.len();

    let responses: Vec<WorkspaceResponse> = workspaces
        .into_iter()
        .map(|w| WorkspaceResponse {
            id: w.id,
            name: w.name,
            nfs_url: w.nfs_url,
            metadata: w.metadata,
            created_at: w.created_at.to_rfc3339(),
            updated_at: w.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(ListWorkspacesResponse {
        workspaces: responses,
        total,
    }))
}

/// Delete a workspace
pub async fn delete_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    state.workspace_service.delete(&id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

// ==================== File Operation Handlers ====================

/// Read a file from workspace
pub async fn read_file(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Json<ReadFileResponse>> {
    let content = state
        .workspace_service
        .read_file_string(&workspace_id, &query.path)
        .await?;
    Ok(Json(ReadFileResponse { content }))
}

/// Write a file to workspace
pub async fn write_file(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Query(query): Query<PathQuery>,
    Json(req): Json<WriteFileRequest>,
) -> Result<Json<serde_json::Value>> {
    state
        .workspace_service
        .write_file(&workspace_id, &query.path, req.content.as_bytes())
        .await?;
    Ok(Json(
        serde_json::json!({ "success": true, "path": query.path }),
    ))
}

/// List directory contents in workspace
pub async fn list_files(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Json<ListFilesResponse>> {
    let files = state
        .workspace_service
        .list_files(&workspace_id, &query.path)
        .await?;

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

    Ok(Json(ListFilesResponse { files: responses }))
}

/// Create directory in workspace
pub async fn mkdir(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Json(req): Json<MkdirRequest>,
) -> Result<Json<serde_json::Value>> {
    state
        .workspace_service
        .mkdir(&workspace_id, &req.path)
        .await?;
    Ok(Json(
        serde_json::json!({ "success": true, "path": req.path }),
    ))
}

/// Delete file or directory in workspace
pub async fn delete_file(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Query(query): Query<DeleteQuery>,
) -> Result<Json<serde_json::Value>> {
    let recursive = query.recursive.map(|r| r == "true").unwrap_or(false);
    state
        .workspace_service
        .delete_file(&workspace_id, &query.path, recursive)
        .await?;
    Ok(Json(
        serde_json::json!({ "success": true, "path": query.path }),
    ))
}

/// Move/rename file or directory in workspace
pub async fn move_file(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<serde_json::Value>> {
    state
        .workspace_service
        .move_file(&workspace_id, &req.source, &req.destination)
        .await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "source": req.source,
        "destination": req.destination
    })))
}

/// Copy file or directory in workspace
pub async fn copy_file(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<serde_json::Value>> {
    state
        .workspace_service
        .copy_file(&workspace_id, &req.source, &req.destination)
        .await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "source": req.source,
        "destination": req.destination
    })))
}

/// Get file info in workspace
pub async fn get_file_info(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Json<FileInfoResponse>> {
    let info = state
        .workspace_service
        .get_file_info(&workspace_id, &query.path)
        .await?;

    Ok(Json(FileInfoResponse {
        name: info.name,
        path: info.path,
        file_type: info.file_type,
        size: info.size,
        modified_at: info.modified_at.map(|t| t.to_rfc3339()),
    }))
}
