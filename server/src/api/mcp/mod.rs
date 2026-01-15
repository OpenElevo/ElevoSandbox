//! MCP (Model Context Protocol) API layer
//!
//! This module provides MCP server functionality, allowing AI assistants
//! to interact with the workspace service through standardized tools.

mod handler;
mod tools;

pub use handler::WorkspaceMcpHandler;

use rmcp::transport::stdio;
use rmcp::ServiceExt;
use tracing::info;

use crate::AppState;

/// Start MCP server in stdio mode
///
/// This runs the MCP server using stdin/stdout for communication,
/// suitable for local CLI usage with AI assistants.
pub async fn serve_stdio(state: AppState) -> anyhow::Result<()> {
    info!("Starting MCP server in stdio mode");

    let handler = WorkspaceMcpHandler::new(state);
    let service = handler.serve(stdio()).await?;

    info!("MCP server ready, waiting for requests...");
    service.waiting().await?;

    info!("MCP server shutdown");
    Ok(())
}
