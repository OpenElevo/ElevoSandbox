//! Executor MCP Handler
//!
//! Minimal MCP handler with only process_run tool.
//! Use this profile when sandbox lifecycle is managed externally
//! and the AI only needs to execute commands.
//!
//! Tools: process_run (1 tool)

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ServerHandler,
};
use tracing::{error, info};

use super::common::{format_command_result, run_command};
use super::types::ProcessRunParams;
use crate::AppState;

/// Executor MCP Handler - minimal tool set for command execution only
#[derive(Clone)]
pub struct ExecutorMcpHandler {
    state: AppState,
    tool_router: ToolRouter<Self>,
}

impl ExecutorMcpHandler {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl ExecutorMcpHandler {
    #[tool(
        description = "Run a command in a sandbox and wait for completion. Returns exit code, stdout, and stderr."
    )]
    async fn process_run(&self, Parameters(params): Parameters<ProcessRunParams>) -> String {
        info!(
            "MCP[executor]: process_run in {} with command: {}",
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
                error!("MCP[executor]: process_run failed: {}", e);
                format!("Failed to run command: {}", e)
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for ExecutorMcpHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Workspace Executor - Run commands in sandboxes. \
                Use process_run to execute commands and get results. \
                Sandbox lifecycle is managed externally."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
