//! Full MCP Handler
//!
//! Complete MCP handler with all available tools.
//! Use this profile when the AI needs full control over sandbox lifecycle,
//! process management, and file operations.
//!
//! Tools: sandbox_create, sandbox_get, sandbox_list, sandbox_delete,
//!        process_run, process_kill,
//!        file_read, file_write, file_list, file_mkdir, file_remove,
//!        file_move, file_copy, file_info (14 tools)

use std::collections::HashMap;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler,
};
use tracing::{error, info};

use super::common::{format_command_result, run_command};
use super::types::*;
use crate::domain::sandbox::{CreateSandboxParams, SandboxState};
use crate::AppState;

/// Full MCP Handler - complete tool set
#[derive(Clone)]
pub struct FullMcpHandler {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

impl FullMcpHandler {
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
    ) -> Result<crate::domain::types::CommandResult, crate::Error> {
        run_command(
            &self.state,
            sandbox_id,
            command,
            args,
            HashMap::new(),
            None,
            30000,
        )
        .await
    }
}

#[tool_router]
impl FullMcpHandler {
    // ========================================================================
    // Sandbox Tools
    // ========================================================================

    #[tool(
        description = "Create a new sandbox environment bound to a workspace. Returns the sandbox ID and details."
    )]
    async fn sandbox_create(&self, Parameters(params): Parameters<SandboxCreateParams>) -> String {
        info!(
            "MCP[full]: sandbox_create called for workspace {}",
            params.workspace_id
        );

        let create_params = CreateSandboxParams {
            workspace_id: params.workspace_id,
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
                error!("MCP[full]: sandbox_create failed: {}", e);
                format!("Failed to create sandbox: {}", e)
            }
        }
    }

    #[tool(description = "Get details of a sandbox by ID.")]
    async fn sandbox_get(&self, Parameters(params): Parameters<SandboxGetParams>) -> String {
        info!("MCP[full]: sandbox_get called for {}", params.sandbox_id);

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
                error!("MCP[full]: sandbox_get failed: {}", e);
                format!("Failed to get sandbox: {}", e)
            }
        }
    }

    #[tool(description = "List all sandboxes, optionally filtered by state.")]
    async fn sandbox_list(&self, Parameters(params): Parameters<SandboxListParams>) -> String {
        info!("MCP[full]: sandbox_list called");

        let state = params
            .state
            .as_deref()
            .and_then(|s| match s.to_lowercase().as_str() {
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
                error!("MCP[full]: sandbox_list failed: {}", e);
                format!("Failed to list sandboxes: {}", e)
            }
        }
    }

    #[tool(description = "Delete a sandbox by ID.")]
    async fn sandbox_delete(&self, Parameters(params): Parameters<SandboxDeleteParams>) -> String {
        info!("MCP[full]: sandbox_delete called for {}", params.sandbox_id);

        let force = params.force.unwrap_or(false);
        match self
            .state
            .sandbox_service
            .delete(&params.sandbox_id, force)
            .await
        {
            Ok(_) => format!("Sandbox {} deleted successfully", params.sandbox_id),
            Err(e) => {
                error!("MCP[full]: sandbox_delete failed: {}", e);
                format!("Failed to delete sandbox: {}", e)
            }
        }
    }

    // ========================================================================
    // Process Tools
    // ========================================================================

    #[tool(
        description = "Run a command in a sandbox and wait for completion. Returns exit code, stdout, and stderr."
    )]
    async fn process_run(&self, Parameters(params): Parameters<ProcessRunParams>) -> String {
        info!(
            "MCP[full]: process_run in {} with command: {}",
            params.sandbox_id, params.command
        );

        let timeout_ms = params.timeout.map(|t| (t * 1000) as u64).unwrap_or(0);

        match run_command(
            &self.state,
            &params.sandbox_id,
            &params.command,
            params.args.unwrap_or_default(),
            params.env.unwrap_or_default(),
            params.cwd,
            timeout_ms,
        )
        .await
        {
            Ok(result) => format_command_result(&result),
            Err(e) => {
                error!("MCP[full]: process_run failed: {}", e);
                format!("Failed to run command: {}", e)
            }
        }
    }

    #[tool(description = "Kill a running process in a sandbox.")]
    async fn process_kill(&self, Parameters(params): Parameters<ProcessKillParams>) -> String {
        info!(
            "MCP[full]: process_kill in {} for pid {}",
            params.sandbox_id, params.pid
        );

        let signal = params.signal.unwrap_or(15);
        match self
            .state
            .process_service
            .kill(&params.sandbox_id, params.pid, Some(signal))
            .await
        {
            Ok(_) => format!("Process {} killed with signal {}", params.pid, signal),
            Err(e) => {
                error!("MCP[full]: process_kill failed: {}", e);
                format!("Failed to kill process: {}", e)
            }
        }
    }

    // ========================================================================
    // File Tools
    // ========================================================================

    #[tool(description = "Read the contents of a file in a sandbox.")]
    async fn file_read(&self, Parameters(params): Parameters<FileReadParams>) -> String {
        info!(
            "MCP[full]: file_read in {} for {}",
            params.sandbox_id, params.path
        );

        match self
            .run_cmd(&params.sandbox_id, "cat", vec![params.path.clone()])
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
                error!("MCP[full]: file_read failed: {}", e);
                format!("Failed to read file: {}", e)
            }
        }
    }

    #[tool(description = "Write content to a file in a sandbox.")]
    async fn file_write(&self, Parameters(params): Parameters<FileWriteParams>) -> String {
        info!(
            "MCP[full]: file_write in {} for {}",
            params.sandbox_id, params.path
        );

        let script = format!(
            "cat > '{}' << 'MCPEOF'\n{}\nMCPEOF",
            params.path.replace("'", "'\\''"),
            params.content
        );

        match self
            .run_cmd(&params.sandbox_id, "bash", vec!["-c".to_string(), script])
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
                error!("MCP[full]: file_write failed: {}", e);
                format!("Failed to write file: {}", e)
            }
        }
    }

    #[tool(description = "List files in a directory in a sandbox.")]
    async fn file_list(&self, Parameters(params): Parameters<FileListParams>) -> String {
        info!(
            "MCP[full]: file_list in {} for {}",
            params.sandbox_id, params.path
        );

        match self
            .run_cmd(
                &params.sandbox_id,
                "ls",
                vec!["-la".to_string(), params.path.clone()],
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
                error!("MCP[full]: file_list failed: {}", e);
                format!("Failed to list directory: {}", e)
            }
        }
    }

    #[tool(description = "Create a directory in a sandbox.")]
    async fn file_mkdir(&self, Parameters(params): Parameters<FileMkdirParams>) -> String {
        info!(
            "MCP[full]: file_mkdir in {} for {}",
            params.sandbox_id, params.path
        );

        let mut args = vec![];
        if params.recursive.unwrap_or(true) {
            args.push("-p".to_string());
        }
        args.push(params.path.clone());

        match self.run_cmd(&params.sandbox_id, "mkdir", args).await {
            Ok(result) => {
                if result.exit_code == 0 {
                    format!("Directory {} created", params.path)
                } else {
                    format!("Failed to create directory: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP[full]: file_mkdir failed: {}", e);
                format!("Failed to create directory: {}", e)
            }
        }
    }

    #[tool(description = "Remove a file or directory in a sandbox.")]
    async fn file_remove(&self, Parameters(params): Parameters<FileRemoveParams>) -> String {
        info!(
            "MCP[full]: file_remove in {} for {}",
            params.sandbox_id, params.path
        );

        let mut args = vec!["-f".to_string()];
        if params.recursive.unwrap_or(false) {
            args.push("-r".to_string());
        }
        args.push(params.path.clone());

        match self.run_cmd(&params.sandbox_id, "rm", args).await {
            Ok(result) => {
                if result.exit_code == 0 {
                    format!("Removed {}", params.path)
                } else {
                    format!("Failed to remove: {}", result.stderr)
                }
            }
            Err(e) => {
                error!("MCP[full]: file_remove failed: {}", e);
                format!("Failed to remove: {}", e)
            }
        }
    }

    #[tool(description = "Move or rename a file in a sandbox.")]
    async fn file_move(&self, Parameters(params): Parameters<FileMoveParams>) -> String {
        info!(
            "MCP[full]: file_move in {} from {} to {}",
            params.sandbox_id, params.src, params.dst
        );

        match self
            .run_cmd(
                &params.sandbox_id,
                "mv",
                vec![params.src.clone(), params.dst.clone()],
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
                error!("MCP[full]: file_move failed: {}", e);
                format!("Failed to move: {}", e)
            }
        }
    }

    #[tool(description = "Copy a file in a sandbox.")]
    async fn file_copy(&self, Parameters(params): Parameters<FileCopyParams>) -> String {
        info!(
            "MCP[full]: file_copy in {} from {} to {}",
            params.sandbox_id, params.src, params.dst
        );

        match self
            .run_cmd(
                &params.sandbox_id,
                "cp",
                vec!["-r".to_string(), params.src.clone(), params.dst.clone()],
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
                error!("MCP[full]: file_copy failed: {}", e);
                format!("Failed to copy: {}", e)
            }
        }
    }

    #[tool(description = "Get information about a file in a sandbox.")]
    async fn file_info(&self, Parameters(params): Parameters<FileInfoParams>) -> String {
        info!(
            "MCP[full]: file_info in {} for {}",
            params.sandbox_id, params.path
        );

        let script = format!(
            "stat --format='{{\"name\":\"%n\",\"size\":%s,\"type\":\"%F\",\"permissions\":\"%a\",\"modified\":\"%y\"}}' '{}'",
            params.path.replace("'", "'\\''")
        );

        match self
            .run_cmd(&params.sandbox_id, "bash", vec!["-c".to_string(), script])
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
                error!("MCP[full]: file_info failed: {}", e);
                format!("Failed to get file info: {}", e)
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for FullMcpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Workspace SDK MCP Server (Full) - Complete sandbox management, \
                process execution, and file operations. Use sandbox_* tools to manage \
                sandbox lifecycle, process_* for command execution, and file_* for \
                file system operations."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
