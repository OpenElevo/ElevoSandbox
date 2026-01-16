//! MCP Tool Parameter Types
//!
//! Shared parameter types for MCP tools across different profiles.

use rmcp::schemars;
use serde::Deserialize;
use std::collections::HashMap;

// ============================================================================
// Sandbox Tool Parameters
// ============================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SandboxCreateParams {
    /// ID of the workspace to bind to (required)
    #[schemars(description = "ID of the workspace to bind to")]
    pub workspace_id: String,

    /// Docker template image to use (default: workspace-base:latest)
    #[schemars(description = "Docker template image to use")]
    pub template: Option<String>,

    /// Human-readable name for the sandbox
    #[schemars(description = "Human-readable name for the sandbox")]
    pub name: Option<String>,

    /// Environment variables to set in the sandbox
    #[schemars(description = "Environment variables to set in the sandbox")]
    pub env: Option<HashMap<String, String>>,

    /// Custom metadata for the sandbox
    #[schemars(description = "Custom metadata for the sandbox")]
    pub metadata: Option<HashMap<String, String>>,

    /// Timeout in seconds for the sandbox
    #[schemars(description = "Timeout in seconds for the sandbox")]
    pub timeout: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SandboxGetParams {
    /// ID of the sandbox to retrieve
    #[schemars(description = "ID of the sandbox to retrieve")]
    pub sandbox_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SandboxListParams {
    /// Filter by sandbox state (optional)
    #[schemars(
        description = "Filter by sandbox state: starting, running, stopping, stopped, error"
    )]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SandboxDeleteParams {
    /// ID of the sandbox to delete
    #[schemars(description = "ID of the sandbox to delete")]
    pub sandbox_id: String,

    /// Force delete even if sandbox is running
    #[schemars(description = "Force delete even if sandbox is running")]
    pub force: Option<bool>,
}

// ============================================================================
// Process Tool Parameters
// ============================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProcessRunParams {
    /// ID of the sandbox to run command in
    #[schemars(description = "ID of the sandbox to run command in")]
    pub sandbox_id: String,

    /// Command to execute
    #[schemars(description = "Command to execute")]
    pub command: String,

    /// Command arguments
    #[schemars(description = "Command arguments")]
    pub args: Option<Vec<String>>,

    /// Environment variables for the command
    #[schemars(description = "Environment variables for the command")]
    pub env: Option<HashMap<String, String>>,

    /// Working directory for the command
    #[schemars(description = "Working directory for the command")]
    pub cwd: Option<String>,

    /// Timeout in seconds for the command
    #[schemars(description = "Timeout in seconds for the command")]
    pub timeout: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProcessKillParams {
    /// ID of the sandbox containing the process
    #[schemars(description = "ID of the sandbox containing the process")]
    pub sandbox_id: String,

    /// Process ID to kill
    #[schemars(description = "Process ID to kill")]
    pub pid: u32,

    /// Signal to send (default: 15/SIGTERM)
    #[schemars(description = "Signal to send (default: 15/SIGTERM)")]
    pub signal: Option<i32>,
}

// ============================================================================
// File Tool Parameters
// ============================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileReadParams {
    /// ID of the sandbox
    #[schemars(description = "ID of the sandbox")]
    pub sandbox_id: String,

    /// Path to the file to read
    #[schemars(description = "Path to the file to read")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileWriteParams {
    /// ID of the sandbox
    #[schemars(description = "ID of the sandbox")]
    pub sandbox_id: String,

    /// Path to write the file to
    #[schemars(description = "Path to write the file to")]
    pub path: String,

    /// Content to write to the file
    #[schemars(description = "Content to write to the file")]
    pub content: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileListParams {
    /// ID of the sandbox
    #[schemars(description = "ID of the sandbox")]
    pub sandbox_id: String,

    /// Path to the directory to list
    #[schemars(description = "Path to the directory to list")]
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileMkdirParams {
    /// ID of the sandbox
    #[schemars(description = "ID of the sandbox")]
    pub sandbox_id: String,

    /// Path of the directory to create
    #[schemars(description = "Path of the directory to create")]
    pub path: String,

    /// Create parent directories if they don't exist
    #[schemars(description = "Create parent directories if they don't exist")]
    pub recursive: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileRemoveParams {
    /// ID of the sandbox
    #[schemars(description = "ID of the sandbox")]
    pub sandbox_id: String,

    /// Path to the file or directory to remove
    #[schemars(description = "Path to the file or directory to remove")]
    pub path: String,

    /// Remove directories recursively
    #[schemars(description = "Remove directories recursively")]
    pub recursive: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileMoveParams {
    /// ID of the sandbox
    #[schemars(description = "ID of the sandbox")]
    pub sandbox_id: String,

    /// Source path
    #[schemars(description = "Source path")]
    pub src: String,

    /// Destination path
    #[schemars(description = "Destination path")]
    pub dst: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileCopyParams {
    /// ID of the sandbox
    #[schemars(description = "ID of the sandbox")]
    pub sandbox_id: String,

    /// Source path
    #[schemars(description = "Source path")]
    pub src: String,

    /// Destination path
    #[schemars(description = "Destination path")]
    pub dst: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FileInfoParams {
    /// ID of the sandbox
    #[schemars(description = "ID of the sandbox")]
    pub sandbox_id: String,

    /// Path to get info for
    #[schemars(description = "Path to get info for")]
    pub path: String,
}
