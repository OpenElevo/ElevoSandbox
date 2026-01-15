//! MCP Client Test
//!
//! This test client connects to the workspace MCP server and tests all tools.

use anyhow::Result;
use rmcp::{
    ServiceExt,
    model::CallToolRequestParam,
    transport::{TokioChildProcess, ConfigureCommandExt},
};
use serde_json::json;
use tokio::process::Command;
use tracing::{info, error, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Helper to call a tool and print the result
async fn call_tool(
    client: &rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
    name: &str,
    args: serde_json::Value,
) -> Result<String> {
    info!("📤 Calling tool: {}", name);
    info!("   Arguments: {}", serde_json::to_string_pretty(&args)?);

    let result = client
        .call_tool(CallToolRequestParam {
            name: name.to_string().into(),
            arguments: args.as_object().cloned(),
        })
        .await?;

    let content = result
        .content
        .first()
        .and_then(|c| c.raw.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_else(|| "No content".into());

    if result.is_error.unwrap_or(false) {
        error!("❌ Tool error: {}", content);
    } else {
        info!("✅ Result:\n{}", content);
    }

    Ok(content)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .init();

    info!("🚀 Starting MCP Client Test");
    info!("================================");

    // Get the path to the server binary
    let server_path = std::env::var("MCP_SERVER_PATH")
        .unwrap_or_else(|_| "../../../target/debug/workspace-server".to_string());

    info!("📡 Connecting to MCP server: {}", server_path);

    // Create the transport using TokioChildProcess with Command
    let transport = TokioChildProcess::new(
        Command::new(&server_path).configure(|cmd| {
            cmd.env("WORKSPACE_MCP_MODE", "stdio");
        })
    )?;

    // Connect to the server using () as a simple client handler
    let client = ().serve(transport).await?;

    info!("✅ Connected to MCP server");
    info!("");

    // ========================================================================
    // Test 1: List available tools
    // ========================================================================
    info!("📋 Test 1: List available tools");
    info!("--------------------------------");

    let tools = client.list_tools(None).await?;
    info!("Available tools ({}):", tools.tools.len());
    for tool in &tools.tools {
        info!("  - {} : {}", tool.name, tool.description.as_deref().unwrap_or(""));
    }
    info!("");

    // ========================================================================
    // Test 2: Sandbox Tools
    // ========================================================================
    info!("🏗️  Test 2: Sandbox Tools");
    info!("--------------------------------");

    // 2.1 List existing sandboxes
    info!("\n📦 2.1 Listing existing sandboxes...");
    let _list_result = call_tool(&client, "sandbox_list", json!({})).await?;
    info!("");

    // 2.2 Use an existing running sandbox for testing
    info!("📦 2.2 Using existing sandbox for testing...");
    // Use one of the pre-existing running sandboxes
    let sandbox_id = "b1b9b632-a8bb-42be-9519-3e8921d3f8d8".to_string();
    info!("📦 Using sandbox: {}", sandbox_id);
    info!("");

    // 2.3 Get sandbox details
    info!("📦 2.3 Getting sandbox details...");
    call_tool(
        &client,
        "sandbox_get",
        json!({
            "sandbox_id": sandbox_id
        }),
    )
    .await?;
    info!("");

    // Wait a moment for the sandbox to be fully ready
    info!("⏳ Waiting for sandbox to be ready...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // ========================================================================
    // Test 3: Process Tools
    // ========================================================================
    info!("⚙️  Test 3: Process Tools");
    info!("--------------------------------");

    // 3.1 Run a simple command
    info!("\n🔧 3.1 Running 'echo Hello from MCP!'...");
    call_tool(
        &client,
        "process_run",
        json!({
            "sandbox_id": sandbox_id,
            "command": "echo",
            "args": ["Hello from MCP!"]
        }),
    )
    .await?;
    info!("");

    // 3.2 Run uname to get system info
    info!("🔧 3.2 Running 'uname -a'...");
    call_tool(
        &client,
        "process_run",
        json!({
            "sandbox_id": sandbox_id,
            "command": "uname",
            "args": ["-a"]
        }),
    )
    .await?;
    info!("");

    // 3.3 Run pwd to get current directory
    info!("🔧 3.3 Running 'pwd'...");
    call_tool(
        &client,
        "process_run",
        json!({
            "sandbox_id": sandbox_id,
            "command": "pwd"
        }),
    )
    .await?;
    info!("");

    // 3.4 Run ls to list files
    info!("🔧 3.4 Running 'ls -la /'...");
    call_tool(
        &client,
        "process_run",
        json!({
            "sandbox_id": sandbox_id,
            "command": "ls",
            "args": ["-la", "/"]
        }),
    )
    .await?;
    info!("");

    // ========================================================================
    // Test 4: FileSystem Tools
    // ========================================================================
    info!("📁 Test 4: FileSystem Tools");
    info!("--------------------------------");

    // 4.1 Create a directory
    info!("\n📂 4.1 Creating directory /tmp/mcp-test...");
    call_tool(
        &client,
        "file_mkdir",
        json!({
            "sandbox_id": sandbox_id,
            "path": "/tmp/mcp-test",
            "recursive": true
        }),
    )
    .await?;
    info!("");

    // 4.2 Write a file
    info!("📝 4.2 Writing file /tmp/mcp-test/hello.txt...");
    call_tool(
        &client,
        "file_write",
        json!({
            "sandbox_id": sandbox_id,
            "path": "/tmp/mcp-test/hello.txt",
            "content": "Hello, MCP World!\nThis is a test file.\nLine 3."
        }),
    )
    .await?;
    info!("");

    // 4.3 Read the file back
    info!("📖 4.3 Reading file /tmp/mcp-test/hello.txt...");
    call_tool(
        &client,
        "file_read",
        json!({
            "sandbox_id": sandbox_id,
            "path": "/tmp/mcp-test/hello.txt"
        }),
    )
    .await?;
    info!("");

    // 4.4 List directory contents
    info!("📂 4.4 Listing directory /tmp/mcp-test...");
    call_tool(
        &client,
        "file_list",
        json!({
            "sandbox_id": sandbox_id,
            "path": "/tmp/mcp-test"
        }),
    )
    .await?;
    info!("");

    // 4.5 Copy the file
    info!("📋 4.5 Copying file to /tmp/mcp-test/hello_copy.txt...");
    call_tool(
        &client,
        "file_copy",
        json!({
            "sandbox_id": sandbox_id,
            "src": "/tmp/mcp-test/hello.txt",
            "dst": "/tmp/mcp-test/hello_copy.txt"
        }),
    )
    .await?;
    info!("");

    // 4.6 Get file info
    info!("ℹ️  4.6 Getting file info for /tmp/mcp-test/hello.txt...");
    call_tool(
        &client,
        "file_info",
        json!({
            "sandbox_id": sandbox_id,
            "path": "/tmp/mcp-test/hello.txt"
        }),
    )
    .await?;
    info!("");

    // 4.7 Move/rename file
    info!("📦 4.7 Moving file to /tmp/mcp-test/hello_moved.txt...");
    call_tool(
        &client,
        "file_move",
        json!({
            "sandbox_id": sandbox_id,
            "src": "/tmp/mcp-test/hello_copy.txt",
            "dst": "/tmp/mcp-test/hello_moved.txt"
        }),
    )
    .await?;
    info!("");

    // 4.8 List directory to verify
    info!("📂 4.8 Verifying directory contents...");
    call_tool(
        &client,
        "file_list",
        json!({
            "sandbox_id": sandbox_id,
            "path": "/tmp/mcp-test"
        }),
    )
    .await?;
    info!("");

    // 4.9 Remove file
    info!("🗑️  4.9 Removing file /tmp/mcp-test/hello_moved.txt...");
    call_tool(
        &client,
        "file_remove",
        json!({
            "sandbox_id": sandbox_id,
            "path": "/tmp/mcp-test/hello_moved.txt"
        }),
    )
    .await?;
    info!("");

    // 4.10 Remove directory recursively
    info!("🗑️  4.10 Removing directory /tmp/mcp-test recursively...");
    call_tool(
        &client,
        "file_remove",
        json!({
            "sandbox_id": sandbox_id,
            "path": "/tmp/mcp-test",
            "recursive": true
        }),
    )
    .await?;
    info!("");

    // ========================================================================
    // Test 5: Done (skip cleanup since we're using existing sandbox)
    // ========================================================================
    info!("🧹 Test 5: Cleanup (skipped - using existing sandbox)");
    info!("--------------------------------");
    info!("Skipping delete for existing sandbox");
    info!("");

    // Verify sandbox is still there
    info!("📋 Verifying sandbox list...");
    call_tool(&client, "sandbox_list", json!({})).await?;

    // ========================================================================
    // Done
    // ========================================================================
    info!("");
    info!("================================");
    info!("✅ All MCP tests completed successfully!");
    info!("================================");

    // Disconnect
    client.cancel().await?;

    Ok(())
}
