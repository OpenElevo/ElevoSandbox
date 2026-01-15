//! FileSystem HTTP handlers

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{AppState, Result};

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
    #[allow(dead_code)]
    pub content: String,
}

/// Mkdir request
#[derive(Debug, Deserialize)]
pub struct MkdirRequest {
    pub path: String,
    #[allow(dead_code)]
    pub recursive: Option<bool>,
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
    #[allow(dead_code)]
    pub recursive: Option<String>,
}

/// Read a file
pub async fn read_file(
    State(_state): State<AppState>,
    Path(_sandbox_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Json<ReadFileResponse>> {
    // TODO: Implement file reading through NFS or agent
    // For now, return a placeholder
    Ok(Json(ReadFileResponse {
        content: format!("Content of {}", query.path),
    }))
}

/// Write a file
pub async fn write_file(
    State(_state): State<AppState>,
    Path(_sandbox_id): Path<String>,
    Query(query): Query<PathQuery>,
    Json(_req): Json<WriteFileRequest>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Implement file writing through NFS or agent
    Ok(Json(serde_json::json!({ "success": true, "path": query.path })))
}

/// List directory contents
pub async fn list_files(
    State(_state): State<AppState>,
    Path(_sandbox_id): Path<String>,
    Query(_query): Query<PathQuery>,
) -> Result<Json<ListFilesResponse>> {
    // TODO: Implement directory listing through NFS or agent
    Ok(Json(ListFilesResponse {
        files: vec![],
    }))
}

/// Create directory
pub async fn mkdir(
    State(_state): State<AppState>,
    Path(_sandbox_id): Path<String>,
    Json(req): Json<MkdirRequest>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Implement mkdir through NFS or agent
    Ok(Json(serde_json::json!({ "success": true, "path": req.path })))
}

/// Delete file or directory
pub async fn delete_file(
    State(_state): State<AppState>,
    Path(_sandbox_id): Path<String>,
    Query(query): Query<DeleteQuery>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Implement delete through NFS or agent
    Ok(Json(serde_json::json!({ "success": true, "path": query.path })))
}

/// Move/rename file or directory
pub async fn move_file(
    State(_state): State<AppState>,
    Path(_sandbox_id): Path<String>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Implement move through NFS or agent
    Ok(Json(serde_json::json!({
        "success": true,
        "source": req.source,
        "destination": req.destination
    })))
}

/// Copy file or directory
pub async fn copy_file(
    State(_state): State<AppState>,
    Path(_sandbox_id): Path<String>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Implement copy through NFS or agent
    Ok(Json(serde_json::json!({
        "success": true,
        "source": req.source,
        "destination": req.destination
    })))
}

/// Get file info
pub async fn get_file_info(
    State(_state): State<AppState>,
    Path(_sandbox_id): Path<String>,
    Query(query): Query<PathQuery>,
) -> Result<Json<FileInfoResponse>> {
    // TODO: Implement file info through NFS or agent
    Ok(Json(FileInfoResponse {
        name: query.path.split('/').last().unwrap_or("").to_string(),
        path: query.path.clone(),
        file_type: "file".to_string(),
        size: 0,
        modified_at: None,
    }))
}
