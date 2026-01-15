//! MCP (Model Context Protocol) API layer
//!
//! This module provides MCP server functionality, allowing AI assistants
//! to interact with the workspace service through standardized tools.
//!
//! ## Profiles
//!
//! Different profiles are available for different use cases:
//!
//! - **executor**: Minimal tool set (1 tool) - only `process_run`
//!   - Use when sandbox lifecycle is managed externally
//!   - AI only needs to execute commands
//!
//! - **developer**: Common development tools (6 tools)
//!   - `process_run`, `file_read`, `file_write`, `file_list`, `file_mkdir`, `file_remove`
//!   - Use when AI needs to execute commands and manage files
//!   - Sandbox lifecycle is managed externally
//!
//! - **full**: Complete tool set (14 tools)
//!   - All sandbox, process, and file operations
//!   - Use when AI needs full control over the environment

mod common;
mod types;
mod executor;
mod developer;
mod full;

pub use executor::ExecutorMcpHandler;
pub use developer::DeveloperMcpHandler;
pub use full::FullMcpHandler;

// Backwards compatibility alias
pub type WorkspaceMcpHandler = FullMcpHandler;

use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing::info;

use crate::AppState;

/// MCP Profile determines which tools are available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpProfile {
    /// Minimal - only process_run (1 tool)
    Executor,
    /// Development - process + basic file ops (6 tools)
    #[default]
    Developer,
    /// Full - all tools (14 tools)
    Full,
}

impl McpProfile {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "executor" | "exec" | "minimal" => McpProfile::Executor,
            "developer" | "dev" => McpProfile::Developer,
            "full" | "all" | "complete" => McpProfile::Full,
            _ => McpProfile::Developer, // default
        }
    }

    pub fn tool_count(&self) -> usize {
        match self {
            McpProfile::Executor => 1,
            McpProfile::Developer => 6,
            McpProfile::Full => 14,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            McpProfile::Executor => "Executor (1 tool: process_run)",
            McpProfile::Developer => "Developer (6 tools: process_run, file_read/write/list/mkdir/remove)",
            McpProfile::Full => "Full (14 tools: sandbox_*, process_*, file_*)",
        }
    }
}

/// Start MCP server in stdio mode with specified profile
pub async fn serve_stdio(state: AppState, profile: McpProfile) -> anyhow::Result<()> {
    info!(
        "Starting MCP server in stdio mode with profile: {:?} ({} tools)",
        profile,
        profile.tool_count()
    );

    match profile {
        McpProfile::Executor => {
            let handler = ExecutorMcpHandler::new(state);
            let service = handler.serve(stdio()).await?;
            info!("MCP server ready (executor profile), waiting for requests...");
            service.waiting().await?;
        }
        McpProfile::Developer => {
            let handler = DeveloperMcpHandler::new(state);
            let service = handler.serve(stdio()).await?;
            info!("MCP server ready (developer profile), waiting for requests...");
            service.waiting().await?;
        }
        McpProfile::Full => {
            let handler = FullMcpHandler::new(state);
            let service = handler.serve(stdio()).await?;
            info!("MCP server ready (full profile), waiting for requests...");
            service.waiting().await?;
        }
    }

    info!("MCP server shutdown");
    Ok(())
}

/// Start MCP server in stdio mode with default profile (developer)
///
/// This is kept for backwards compatibility
pub async fn serve_stdio_default(state: AppState) -> anyhow::Result<()> {
    serve_stdio(state, McpProfile::Developer).await
}
