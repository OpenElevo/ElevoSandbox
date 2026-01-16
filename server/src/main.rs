//! Elevo Workspace Server
//!
//! This server provides HTTP and gRPC APIs for managing sandboxes,
//! executing processes, and handling interactive terminals.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::signal;
use tonic::transport::Server;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod api;
mod config;
mod domain;
mod error;
mod infra;
mod proto;
mod service;

pub use config::Config;
pub use error::{Error, Result};

use infra::agent_pool::AgentConnPool;
use infra::docker::DockerManager;
use infra::nfs::{NfsManager, NfsMode};
use infra::sqlite::SandboxRepository;
use infra::workspace_repository::WorkspaceRepository;
use service::process::ProcessService;
use service::pty::PtyService;
use service::sandbox::SandboxService;
use service::workspace::WorkspaceService;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub workspace_service: Arc<WorkspaceService>,
    pub sandbox_service: Arc<SandboxService>,
    pub process_service: Arc<ProcessService>,
    pub pty_service: Arc<PtyService>,
    pub agent_pool: Arc<AgentConnPool>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration first to check MCP mode
    dotenvy::dotenv().ok();
    let config = Config::load()?;
    let config = Arc::new(config);

    // Initialize tracing - use stderr for MCP stdio mode to avoid polluting stdout
    if config.mcp_mode == "stdio" {
        tracing_subscriber::registry()
            .with(fmt::layer().with_writer(std::io::stderr))
            .with(
                EnvFilter::builder()
                    .with_default_directive(Level::INFO.into())
                    .from_env_lossy(),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(
                EnvFilter::builder()
                    .with_default_directive(Level::INFO.into())
                    .from_env_lossy(),
            )
            .init();
    }
    let http_addr: SocketAddr = format!("{}:{}", config.http_host, config.http_port).parse()?;
    let grpc_addr: SocketAddr = format!("{}:{}", config.grpc_host, config.grpc_port).parse()?;

    info!("Starting Workspace Server");
    info!("HTTP listening on {}", http_addr);
    info!("gRPC listening on {}", grpc_addr);

    // Initialize infrastructure
    let pool = SandboxRepository::init(&config.database_url).await?;
    let sandbox_repository = Arc::new(SandboxRepository::new(pool.clone()));
    let workspace_repository = Arc::new(WorkspaceRepository::new(pool));
    let docker = Arc::new(DockerManager::new(None, &config.base_image)?);
    let agent_pool = Arc::new(AgentConnPool::new());

    // Initialize NFS manager
    let nfs_mode = match config.nfs_mode.as_str() {
        "system" => NfsMode::System,
        _ => NfsMode::Embedded,
    };
    let nfs_manager = Arc::new(NfsManager::new(
        nfs_mode,
        PathBuf::from(&config.workspace_dir),
        config.nfs_port,
        config.get_nfs_host().to_string(),
    ));

    // Start NFS server if embedded mode
    if let Err(e) = nfs_manager.start().await {
        error!("Failed to start NFS server: {}", e);
        // Non-fatal, continue without NFS
    }

    // Initialize services
    let workspace_service = Arc::new(WorkspaceService::new(
        workspace_repository.clone(),
        nfs_manager.clone(),
        config.clone(),
    ));

    let sandbox_service = Arc::new(SandboxService::new(
        sandbox_repository.clone(),
        workspace_repository.clone(),
        docker.clone(),
        agent_pool.clone(),
        config.clone(),
    ));

    let process_service = Arc::new(ProcessService::new(
        agent_pool.clone(),
        sandbox_repository.clone(),
    ));

    let pty_service = Arc::new(PtyService::new(
        agent_pool.clone(),
        sandbox_repository.clone(),
    ));

    // Create application state
    let state = AppState {
        config: config.clone(),
        workspace_service,
        sandbox_service,
        process_service,
        pty_service,
        agent_pool: agent_pool.clone(),
    };

    // Check if MCP mode is enabled (stdio mode runs exclusively)
    if config.mcp_mode == "stdio" {
        let profile = api::mcp::McpProfile::from_str(&config.mcp_profile);
        info!(
            "Running in MCP stdio mode with profile: {}",
            profile.description()
        );
        return api::mcp::serve_stdio(state, profile).await;
    }

    // Build HTTP router
    let mut app = api::http::create_router(state.clone());

    // Add MCP HTTP endpoints if enabled
    if config.mcp_mode == "http" {
        let mcp_path = config.mcp_path.clone();
        info!("MCP HTTP endpoints enabled at {}/<profile>", mcp_path);
        info!("  - {}/executor  (1 tool: process_run)", mcp_path);
        info!("  - {}/developer (6 tools: process + file ops)", mcp_path);
        info!("  - {}/full      (14 tools: all operations)", mcp_path);
        let mcp_router = api::mcp::create_mcp_router(state.clone());
        app = app.nest(&mcp_path, mcp_router);
    }

    // Start HTTP server
    let http_server = axum::serve(
        tokio::net::TcpListener::bind(http_addr).await?,
        app.into_make_service(),
    );

    // Start gRPC server for agent connections
    let grpc_server = api::grpc::create_server(agent_pool.clone());

    // Run both servers concurrently
    tokio::select! {
        result = http_server.with_graceful_shutdown(shutdown_signal()) => {
            if let Err(e) = result {
                tracing::error!("HTTP server error: {}", e);
            }
        }
        result = Server::builder()
            .add_service(grpc_server)
            .serve_with_shutdown(grpc_addr, shutdown_signal()) => {
            if let Err(e) = result {
                tracing::error!("gRPC server error: {}", e);
            }
        }
    }

    info!("Server shutdown complete");
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
