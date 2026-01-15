//! MCP Server Handler implementation
//!
//! This module implements the MCP ServerHandler trait for the workspace service,
//! providing tool definitions for sandbox, process, and filesystem operations.

use std::collections::HashMap;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use tracing::{error, info};

use crate::AppState;
use crate::domain::sandbox::{CreateSandboxParams, SandboxState};
use crate::service::process::RunCommandOptions;

/// MCP Handler for workspace operations
#[derive(Clone)]
pub struct WorkspaceMcpHandler {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

impl WorkspaceMcpHandler {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    /// Helper to run a command in a sandbox
    async fn run_cmd(
        &self,
        sandbox_id: &str,
        command: &str,
        args: Vec<String>,
        env: HashMap<String, String>,
        cwd: Option<String>,
        timeout_ms: u64,
    ) -> Result<crate::domain::types::CommandResult, crate::Error> {
        let opts = RunCommandOptions {
            command: command.to_string(),
            args,
            env,
            cwd,
            timeout_ms,
        };
        self.state.process_service.run(sandbox_id, opts).await
    }
}

// ============================================================================
// Tool Parameter Types
// ============================================================================

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SandboxCreateParams {
    /// Docker template image to use (default: workspace-test:latest)
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
    #[schemars(description = "Filter by sandbox state: starting, running, stopping, stopped, error")]
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

// ============================================================================
// Tool Router Implementation
// ============================================================================

#[tool_router]
impl WorkspaceMcpHandler {
    // ------------------------------------------------------------------------
    // Sandbox Tools
    // ------------------------------------------------------------------------

    #[tool(description = "Create a new sandbox environment. Returns the sandbox ID and details.")]
    async fn sandbox_create(&self, Parameters(params): Parameters<SandboxCreateParams>) -> String {
        info!("MCP: sandbox_create called");

        let create_params = CreateSandboxParams {
            template: params.template,
            name: params.name,
            env: params.env,
            metadata: params.metadata,
            timeout: params.timeout,
        };

        match self.state.sandbox_service.create(create_params).await {
            Ok(sandbox) => {
                let response = serde_json::json!({
                    "id": sandbox.id,
                    "name": sandbox.name,
                    "template": sandbox.template,
                    "state": sandbox.state,
                    "created_at": sandbox.created_at,
                });
                serde_json::to_string_pretty(&response).unwrap_or_default()
            }
            Err(e) => {
                error!("MCP: sandbox_create failed: {}", e);
                format!("Failed to create sandbox: {}", e)
            }
        }
    }

    #[tool(description = "Get details of a sandbox by ID.")]
    async fn sandbox_get(&self, Parameters(params): Parameters<SandboxGetParams>) -> String {
        info!("MCP: sandbox_get called for {}", params.sandbox_id);

        match self.state.sandbox_service.get(&params.sandbox_id).await {
            Ok(sandbox) => {
                let response = serde_json::json!({
                    "id": sandbox.id,
                    "name": sandbox.name,
                    "template": sandbox.template,
                    "state": sandbox.state,
                    "created_at": sandbox.created_at,
                    "updated_at": sandbox.updated_at,
                    "metadata": sandbox.metadata,
                });
                serde_json::to_string_pretty(&response).unwrap_or_default()
            }
            Err(e) => {
                error!("MCP: sandbox_get failed: {}", e);
                format!("Failed to get sandbox: {}", e)
            }
        }
    }

    #[tool(description = "List all sandboxes, optionally filtered by state.")]
    async fn sandbox_list(&self, Parameters(params): Parameters<SandboxListParams>) -> String {
        info!("MCP: sandbox_list called");

        // Parse state string to SandboxState enum
        let state = params.state.as_deref().and_then(|s| match s.to_lowercase().as_str() {
            "starting" => Some(SandboxState::Starting),
            "running" => Some(SandboxState::Running),
            "stopping" => Some(SandboxState::Stopping),
            "stopped" => Some(SandboxState::Stopped),
            "error" => Some(SandboxState::Error),
            _ => None,
        });

        match self.state.sandbox_service.list(state).await {
            Ok(sandboxes) => {
                let response: Vec<serde_json::Value> = sandboxes
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id,
                            "name": s.name,
                            "template": s.template,
                            "state": s.state,
                            "created_at": s.created_at,
                        })
                    })
                    .collect();
                serde_json::to_string_pretty(&response).unwrap_or_default()
            }
            Err(e) => {
                error!("MCP: sandbox_list failed: {}", e);
                format!("Failed to list sandboxes: {}", e)
            }
        }
    }

    #[tool(description = "Delete a sandbox by ID.")]
    async fn sandbox_delete(&self, Parameters(params): Parameters<SandboxDeleteParams>) -> String {
        info!("MCP: sandbox_delete called for {}", params.sandbox_id);

        let force = params.force.unwrap_or(false);
        match self.state.sandbox_service.delete(&params.sandbox_id, force).await {
            Ok(_) => format!("Sandbox {} deleted successfully", params.sandbox_id),
            Err(e) => {
                error!("MCP: sandbox_delete failed: {}", e);
                format!("Failed to delete sandbox: {}", e)
            }
        }
    }

    // ------------------------------------------------------------------------
    // Process Tools
    // ------------------------------------------------------------------------

    #[tool(description = "Run a command in a sandbox and wait for completion. Returns exit code, stdout, and stderr.")]
    async fn process_run(&self, Parameters(params): Parameters<ProcessRunParams>) -> String {
        info!(
            "MCP: process_run called in {} with command: {}",
            params.sandbox_id, params.command
        );

        let timeout_ms = params.timeout.map(|t| (t * 1000) as u64).unwrap_or(0);

        match self
            .run_cmd(
                &params.sandbox_id,
                &params.command,
                params.args.unwrap_or_default(),
                params.env.unwrap_or_default(),
                params.cwd,
                timeout_ms,
            )
            .await
        {
            Ok(result) => {
                let response = serde_json::json!({
                    "exit_code": result.exit_code,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                });
                serde_json::to_string_pretty(&response).unwrap_or_default()
            }
            Err(e) => {
                error!("MCP: process_run failed: {}", e);
                format!("Failed to run command: {}", e)
            }
        }
    }

    #[tool(description = "Kill a running process in a sandbox.")]
    async fn process_kill(&self, Parameters(params): Parameters<ProcessKillParams>) -> String {
        info!(
            "MCP: process_kill called in {} for pid {}",
            params.sandbox_id, params.pid
        );

        let signal = params.signal.unwrap_or(15); // SIGTERM
        match self
            .state
            .process_service
            .kill(&params.sandbox_id, params.pid, Some(signal))
            .await
        {
            Ok(_) => format!("Process {} killed with signal {}", params.pid, signal),
            Err(e) => {
                error!("MCP: process_kill failed: {}", e);
                format!("Failed to kill process: {}", e)
            }
        }
    }

    // ------------------------------------------------------------------------
    // FileSystem Tools
    // ------------------------------------------------------------------------

    #[tool(description = "Read the contents of a file in a sandbox.")]
    async fn file_read(&self, Parameters(params): Parameters<FileReadParams>) -> String {
        info!(
            "MCP: file_read called in {} for {}",
            params.sandbox_id, params.path
        );

        match self
            .run_cmd(
                &params.sandbox_id,
                "cat",
                vec![params.path.clone()],
                HashMap::new(),
                None,
                30000,
            )
            .await
        {
            Ok(result) => {
                if result.exit_code == 0 {
                    result.stdout
                } else {
                    format!("Failed to read file: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP: file_read failed: {}", e);
                format!("Failed to read file: {}", e)
            }
        }
    }

    #[tool(description = "Write content to a file in a sandbox.")]
    async fn file_write(&self, Parameters(params): Parameters<FileWriteParams>) -> String {
        info!(
            "MCP: file_write called in {} for {}",
            params.sandbox_id, params.path
        );

        let script = format!(
            "cat > '{}' << 'MCPEOF'\n{}\nMCPEOF",
            params.path.replace("'", "'\\''"),
            params.content
        );

        match self
            .run_cmd(
                &params.sandbox_id,
                "bash",
                vec!["-c".to_string(), script],
                HashMap::new(),
                None,
                30000,
            )
            .await
        {
            Ok(result) => {
                if result.exit_code == 0 {
                    format!("Successfully wrote to {}", params.path)
                } else {
                    format!("Failed to write file: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP: file_write failed: {}", e);
                format!("Failed to write file: {}", e)
            }
        }
    }

    #[tool(description = "List files in a directory in a sandbox.")]
    async fn file_list(&self, Parameters(params): Parameters<FileListParams>) -> String {
        info!(
            "MCP: file_list called in {} for {}",
            params.sandbox_id, params.path
        );

        match self
            .run_cmd(
                &params.sandbox_id,
                "ls",
                vec!["-la".to_string(), params.path.clone()],
                HashMap::new(),
                None,
                30000,
            )
            .await
        {
            Ok(result) => {
                if result.exit_code == 0 {
                    result.stdout
                } else {
                    format!("Failed to list directory: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP: file_list failed: {}", e);
                format!("Failed to list directory: {}", e)
            }
        }
    }

    #[tool(description = "Create a directory in a sandbox.")]
    async fn file_mkdir(&self, Parameters(params): Parameters<FileMkdirParams>) -> String {
        info!(
            "MCP: file_mkdir called in {} for {}",
            params.sandbox_id, params.path
        );

        let mut args = vec![];
        if params.recursive.unwrap_or(false) {
            args.push("-p".to_string());
        }
        args.push(params.path.clone());

        match self
            .run_cmd(
                &params.sandbox_id,
                "mkdir",
                args,
                HashMap::new(),
                None,
                30000,
            )
            .await
        {
            Ok(result) => {
                if result.exit_code == 0 {
                    format!("Directory {} created", params.path)
                } else {
                    format!("Failed to create directory: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP: file_mkdir failed: {}", e);
                format!("Failed to create directory: {}", e)
            }
        }
    }

    #[tool(description = "Remove a file or directory in a sandbox.")]
    async fn file_remove(&self, Parameters(params): Parameters<FileRemoveParams>) -> String {
        info!(
            "MCP: file_remove called in {} for {}",
            params.sandbox_id, params.path
        );

        let mut args = vec!["-f".to_string()];
        if params.recursive.unwrap_or(false) {
            args.push("-r".to_string());
        }
        args.push(params.path.clone());

        match self
            .run_cmd(
                &params.sandbox_id,
                "rm",
                args,
                HashMap::new(),
                None,
                30000,
            )
            .await
        {
            Ok(result) => {
                if result.exit_code == 0 {
                    format!("Removed {}", params.path)
                } else {
                    format!("Failed to remove: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP: file_remove failed: {}", e);
                format!("Failed to remove: {}", e)
            }
        }
    }

    #[tool(description = "Move or rename a file in a sandbox.")]
    async fn file_move(&self, Parameters(params): Parameters<FileMoveParams>) -> String {
        info!(
            "MCP: file_move called in {} from {} to {}",
            params.sandbox_id, params.src, params.dst
        );

        match self
            .run_cmd(
                &params.sandbox_id,
                "mv",
                vec![params.src.clone(), params.dst.clone()],
                HashMap::new(),
                None,
                30000,
            )
            .await
        {
            Ok(result) => {
                if result.exit_code == 0 {
                    format!("Moved {} to {}", params.src, params.dst)
                } else {
                    format!("Failed to move: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP: file_move failed: {}", e);
                format!("Failed to move: {}", e)
            }
        }
    }

    #[tool(description = "Copy a file in a sandbox.")]
    async fn file_copy(&self, Parameters(params): Parameters<FileCopyParams>) -> String {
        info!(
            "MCP: file_copy called in {} from {} to {}",
            params.sandbox_id, params.src, params.dst
        );

        match self
            .run_cmd(
                &params.sandbox_id,
                "cp",
                vec!["-r".to_string(), params.src.clone(), params.dst.clone()],
                HashMap::new(),
                None,
                30000,
            )
            .await
        {
            Ok(result) => {
                if result.exit_code == 0 {
                    format!("Copied {} to {}", params.src, params.dst)
                } else {
                    format!("Failed to copy: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP: file_copy failed: {}", e);
                format!("Failed to copy: {}", e)
            }
        }
    }

    #[tool(description = "Get information about a file in a sandbox.")]
    async fn file_info(&self, Parameters(params): Parameters<FileInfoParams>) -> String {
        info!(
            "MCP: file_info called in {} for {}",
            params.sandbox_id, params.path
        );

        let script = format!(
            "stat --format='{{\"name\":\"%n\",\"size\":%s,\"type\":\"%F\",\"permissions\":\"%a\",\"modified\":\"%y\"}}' '{}'",
            params.path.replace("'", "'\\''")
        );

        match self
            .run_cmd(
                &params.sandbox_id,
                "bash",
                vec!["-c".to_string(), script],
                HashMap::new(),
                None,
                30000,
            )
            .await
        {
            Ok(result) => {
                if result.exit_code == 0 {
                    result.stdout
                } else {
                    format!("Failed to get file info: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP: file_info failed: {}", e);
                format!("Failed to get file info: {}", e)
            }
        }
    }
}

// ============================================================================
// ServerHandler Implementation
// ============================================================================

#[tool_handler]
impl ServerHandler for WorkspaceMcpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Workspace SDK MCP Server - Manage sandboxed development environments, \
                execute commands, and access files. Use sandbox_create to start, \
                process_run to execute commands, and file_* tools for file operations."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
