//! Shared tool implementations
//!
//! Common functions used by different MCP handlers.

use crate::service::process::RunCommandOptions;
use crate::AppState;
use std::collections::HashMap;

/// Run a command in a sandbox
pub async fn run_command(
    state: &AppState,
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
    state.process_service.run(sandbox_id, opts).await
}

/// Format command result as JSON string
pub fn format_command_result(result: &crate::domain::types::CommandResult) -> String {
    let response = serde_json::json!({
        "exit_code": result.exit_code,
        "stdout": result.stdout,
        "stderr": result.stderr,
    });
    serde_json::to_string_pretty(&response).unwrap_or_default()
}
