//! Developer MCP Handler
//!
//! MCP handler with common development tools.
//! Use this profile when the AI needs to execute commands and
//! perform basic file operations, but sandbox lifecycle is managed externally.
//!
//! Tools: process_run, file_read, file_write, file_list, file_mkdir, file_remove (6 tools)

use std::collections::HashMap;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler,
};
use tracing::{error, info};

use super::common::{format_command_result, run_command};
use super::types::{
    FileListParams, FileMkdirParams, FileReadParams, FileRemoveParams, FileWriteParams,
    ProcessRunParams,
};
use crate::AppState;

/// Developer MCP Handler - common development tools
#[derive(Clone)]
pub struct DeveloperMcpHandler {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

impl DeveloperMcpHandler {
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
impl DeveloperMcpHandler {
    // ------------------------------------------------------------------------
    // Process Tools
    // ------------------------------------------------------------------------

    #[tool(
        description = "Run a command in a sandbox and wait for completion. Returns exit code, stdout, and stderr."
    )]
    async fn process_run(&self, Parameters(params): Parameters<ProcessRunParams>) -> String {
        info!(
            "MCP[developer]: process_run in {} with command: {}",
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
                error!("MCP[developer]: process_run failed: {}", e);
                format!("Failed to run command: {}", e)
            }
        }
    }

    // ------------------------------------------------------------------------
    // File Tools
    // ------------------------------------------------------------------------

    #[tool(description = "Read the contents of a file in a sandbox.")]
    async fn file_read(&self, Parameters(params): Parameters<FileReadParams>) -> String {
        info!(
            "MCP[developer]: file_read in {} for {}",
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
                error!("MCP[developer]: file_read failed: {}", e);
                format!("Failed to read file: {}", e)
            }
        }
    }

    #[tool(description = "Write content to a file in a sandbox.")]
    async fn file_write(&self, Parameters(params): Parameters<FileWriteParams>) -> String {
        info!(
            "MCP[developer]: file_write in {} for {}",
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
                error!("MCP[developer]: file_write failed: {}", e);
                format!("Failed to write file: {}", e)
            }
        }
    }

    #[tool(description = "List files in a directory in a sandbox.")]
    async fn file_list(&self, Parameters(params): Parameters<FileListParams>) -> String {
        info!(
            "MCP[developer]: file_list in {} for {}",
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
                error!("MCP[developer]: file_list failed: {}", e);
                format!("Failed to list directory: {}", e)
            }
        }
    }

    #[tool(description = "Create a directory in a sandbox.")]
    async fn file_mkdir(&self, Parameters(params): Parameters<FileMkdirParams>) -> String {
        info!(
            "MCP[developer]: file_mkdir in {} for {}",
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
                error!("MCP[developer]: file_mkdir failed: {}", e);
                format!("Failed to create directory: {}", e)
            }
        }
    }

    #[tool(description = "Remove a file or directory in a sandbox.")]
    async fn file_remove(&self, Parameters(params): Parameters<FileRemoveParams>) -> String {
        info!(
            "MCP[developer]: file_remove in {} for {}",
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
                error!("MCP[developer]: file_remove failed: {}", e);
                format!("Failed to remove: {}", e)
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for DeveloperMcpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Workspace Developer Tools - Execute commands and manage files in sandboxes. \
                Use process_run to execute commands, file_read/file_write for file content, \
                file_list to browse directories, file_mkdir to create directories, \
                and file_remove to delete files. Sandbox lifecycle is managed externally."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
