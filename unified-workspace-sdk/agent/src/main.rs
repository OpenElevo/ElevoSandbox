//! Unified Workspace SDK Agent
//!
//! This agent runs inside sandbox containers and communicates with the server
//! via gRPC bidirectional streaming.

use std::env;

use tokio::signal;
use tracing::{info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod connection;
mod handlers;

use connection::ConnectionManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .init();

    // Get configuration from environment
    let server_addr = env::var("WORKSPACE_SERVER_ADDR")
        .unwrap_or_else(|_| "http://host.docker.internal:9090".to_string());
    let sandbox_id = env::var("WORKSPACE_SANDBOX_ID")
        .expect("WORKSPACE_SANDBOX_ID environment variable is required");

    info!("Starting Workspace Agent for sandbox: {}", sandbox_id);
    info!("Connecting to server: {}", server_addr);

    // Create connection manager
    let mut conn_manager = ConnectionManager::new(server_addr, sandbox_id);

    // Run connection manager in background
    let conn_handle = tokio::spawn(async move {
        if let Err(e) = conn_manager.run().await {
            tracing::error!("Connection manager error: {}", e);
        }
    });

    // Wait for shutdown signal
    shutdown_signal().await;

    // Stop the connection manager
    conn_handle.abort();

    info!("Agent shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Received shutdown signal");
}
