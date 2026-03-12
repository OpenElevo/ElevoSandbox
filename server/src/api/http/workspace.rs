//! Shared HTTP request/response types for file operations
//!
//! These types are shared across namespace, share, and me handlers.

use serde::{Deserialize, Serialize};

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
