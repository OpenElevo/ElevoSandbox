//! Error types for the workspace server

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

/// Result type alias using our Error type
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for the workspace server
#[derive(Debug, Error)]
pub enum Error {
    // Sandbox errors (2000-2999)
    #[error("Sandbox not found: {0}")]
    SandboxNotFound(String),

    #[error("Sandbox already exists: {0}")]
    SandboxAlreadyExists(String),

    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Sandbox in invalid state: expected {expected}, got {actual}")]
    InvalidSandboxState { expected: String, actual: String },

    #[error("Sandbox limit exceeded")]
    SandboxLimitExceeded,

    // Workspace errors (7000-7999)
    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("Workspace has active sandboxes")]
    WorkspaceHasActiveSandboxes,

    #[error("Path not allowed: {0}")]
    PathNotAllowed(String),

    // FileSystem errors (3000-3999)
    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("File already exists: {0}")]
    FileAlreadyExists(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Not a directory: {0}")]
    NotADirectory(String),

    #[error("Directory not empty: {0}")]
    DirectoryNotEmpty(String),

    // Process errors (4000-4099)
    #[error("Process not found: {0}")]
    ProcessNotFound(String),

    #[error("Process timeout")]
    ProcessTimeout,

    #[error("Process execution failed: {0}")]
    ProcessExecutionFailed(String),

    // PTY errors (4100-4199)
    #[error("PTY not found: {0}")]
    PtyNotFound(String),

    #[error("PTY limit exceeded")]
    PtyLimitExceeded,

    #[error("PTY already closed")]
    PtyAlreadyClosed,

    // Agent errors (5000-5999)
    #[error("Agent not connected for sandbox: {0}")]
    AgentNotConnected(String),

    #[error("Agent connection timeout")]
    AgentConnectionTimeout,

    #[error("Agent communication error: {0}")]
    AgentCommunicationError(String),

    // Infrastructure errors (6000-6999)
    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Docker error: {0}")]
    DockerError(String),

    #[error("NFS error: {0}")]
    NfsError(String),

    // General errors (1000-1999)
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}

impl Error {
    /// Get the error code
    pub fn code(&self) -> u32 {
        match self {
            // Sandbox errors (2000-2999)
            Error::SandboxNotFound(_) => 2001,
            Error::SandboxAlreadyExists(_) => 2002,
            Error::TemplateNotFound(_) => 2003,
            Error::InvalidSandboxState { .. } => 2004,
            Error::SandboxLimitExceeded => 2005,

            // Workspace errors (7000-7999)
            Error::WorkspaceNotFound(_) => 7001,
            Error::WorkspaceHasActiveSandboxes => 7002,
            Error::PathNotAllowed(_) => 7003,

            // FileSystem errors (3000-3999)
            Error::FileNotFound(_) => 3001,
            Error::FileAlreadyExists(_) => 3002,
            Error::PermissionDenied(_) => 3003,
            Error::InvalidPath(_) => 3004,
            Error::NotADirectory(_) => 3005,
            Error::DirectoryNotEmpty(_) => 3006,

            // Process errors (4000-4099)
            Error::ProcessNotFound(_) => 4001,
            Error::ProcessTimeout => 4002,
            Error::ProcessExecutionFailed(_) => 4003,

            // PTY errors (4100-4199)
            Error::PtyNotFound(_) => 4101,
            Error::PtyLimitExceeded => 4102,
            Error::PtyAlreadyClosed => 4103,

            // Agent errors (5000-5999)
            Error::AgentNotConnected(_) => 5001,
            Error::AgentConnectionTimeout => 5002,
            Error::AgentCommunicationError(_) => 5003,

            // Infrastructure errors (6000-6999)
            Error::DatabaseError(_) => 6001,
            Error::DockerError(_) => 6002,
            Error::NfsError(_) => 6003,

            // General errors (1000-1999)
            Error::InvalidRequest(_) => 1001,
            Error::InvalidParameter(_) => 1002,
            Error::Internal(_) => 1003,
            Error::NotImplemented(_) => 1004,
        }
    }

    /// Get the HTTP status code
    pub fn status_code(&self) -> StatusCode {
        match self {
            Error::SandboxNotFound(_)
            | Error::WorkspaceNotFound(_)
            | Error::FileNotFound(_)
            | Error::ProcessNotFound(_)
            | Error::PtyNotFound(_)
            | Error::TemplateNotFound(_) => StatusCode::NOT_FOUND,

            Error::SandboxAlreadyExists(_) | Error::FileAlreadyExists(_) => StatusCode::CONFLICT,

            Error::WorkspaceHasActiveSandboxes => StatusCode::CONFLICT,

            Error::PermissionDenied(_) | Error::PathNotAllowed(_) => StatusCode::FORBIDDEN,

            Error::InvalidRequest(_)
            | Error::InvalidParameter(_)
            | Error::InvalidPath(_)
            | Error::InvalidSandboxState { .. }
            | Error::NotADirectory(_)
            | Error::DirectoryNotEmpty(_) => StatusCode::BAD_REQUEST,

            Error::SandboxLimitExceeded | Error::PtyLimitExceeded => {
                StatusCode::TOO_MANY_REQUESTS
            }

            Error::ProcessTimeout | Error::AgentConnectionTimeout => StatusCode::GATEWAY_TIMEOUT,

            Error::AgentNotConnected(_) | Error::AgentCommunicationError(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }

            Error::NotImplemented(_) => StatusCode::NOT_IMPLEMENTED,

            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// API error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: u32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorResponse {
            code: self.code(),
            message: self.to_string(),
            details: None,
        };

        (status, Json(body)).into_response()
    }
}

// Implement From for common error types
impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::DatabaseError(err.to_string())
    }
}

impl From<bollard::errors::Error> for Error {
    fn from(err: bollard::errors::Error) -> Self {
        Error::DockerError(err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Internal(err.to_string())
    }
}
