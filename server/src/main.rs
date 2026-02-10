//! Elevo Workspace Server
//!
//! This server provides HTTP and gRPC APIs for managing sandboxes,
//! executing processes, and handling interactive terminals.

use std::net::SocketAddr;
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

use api::grpc::{AuthInterceptor, FileSystemServiceImpl};
use config::StorageConfig;
use infra::agent_pool::AgentConnPool;
use infra::docker::DockerManager;
use infra::metrics;
use infra::nfs::{NfsManager, NfsMode};
use infra::sqlite::SandboxRepository;
use infra::storage::lease::SqliteLeaseManager;
use infra::storage::local::LocalStorageBackend;
use infra::storage::s3fs_mount::{S3Credentials, S3fsMountManager, S3fsMountMonitor};
use infra::storage::StorageBackend;
use infra::workspace_repository::WorkspaceRepository;
use proto::file_system_service_server::FileSystemServiceServer;
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

/// Initialize the storage backend based on configuration.
///
/// - **Local mode**: Creates a `LocalStorageBackend` pointing to `workspace_dir`.
/// - **S3 mode**: Cleans up stale mounts, mounts the S3 bucket via s3fs-fuse,
///   runs a health check, then creates a `LocalStorageBackend` pointing at the
///   mount point. Returns the mount manager for shutdown cleanup.
async fn init_storage(
    config: &StorageConfig,
) -> anyhow::Result<(
    Arc<dyn StorageBackend>,
    Option<Arc<S3fsMountManager>>,
)> {
    match config {
        StorageConfig::Local { workspace_dir } => {
            info!("Using local storage backend at {:?}", workspace_dir);
            // Ensure workspace root directory exists at startup (fail-fast on permission issues)
            tokio::fs::create_dir_all(workspace_dir).await?;
            let backend = LocalStorageBackend::new(workspace_dir.clone());
            Ok((Arc::new(backend), None))
        }
        StorageConfig::S3 { workspace_dir, s3 } => {
            info!(
                "Using S3 storage backend: bucket={}, endpoint={}",
                s3.bucket, s3.endpoint
            );

            let credentials = match (&s3.access_key, &s3.secret_key) {
                (Some(ak), Some(sk)) => Some(S3Credentials {
                    access_key: ak.clone(),
                    secret_key: sk.clone(),
                }),
                _ => None,
            };

            let mount_manager = Arc::new(S3fsMountManager::new(
                workspace_dir.clone(),
                s3.bucket.clone(),
                s3.endpoint.clone(),
                s3.region.clone(),
                credentials,
                s3.cache_dir.clone(),
            ));

            // Clean up any stale mounts from previous crashes
            mount_manager.cleanup_stale_mount().await?;

            // Mount the S3 bucket
            mount_manager.mount().await?;

            // Verify the mount is healthy
            mount_manager.health_check().await?;

            // Start background mount health monitor (checks every 30s, auto-remounts)
            let monitor = S3fsMountMonitor::new(
                mount_manager.clone(),
                std::time::Duration::from_secs(30),
            );
            monitor.start();

            info!("S3 storage mounted at {:?}", workspace_dir);

            // In S3 mode, we still use LocalStorageBackend — the mount point
            // is a regular POSIX directory provided by s3fs-fuse.
            let backend = LocalStorageBackend::new(workspace_dir.clone());
            Ok((Arc::new(backend), Some(mount_manager)))
        }
    }
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

    // Initialize Prometheus metrics
    let metrics_handle = metrics::init_metrics();
    info!("Prometheus metrics initialized, available at /metrics");

    // Initialize infrastructure
    let pool = SandboxRepository::init(&config.database_url).await?;
    let sandbox_repository = Arc::new(SandboxRepository::new(pool.clone()));
    let workspace_repository = Arc::new(WorkspaceRepository::new(pool.clone()));
    let docker = Arc::new(DockerManager::new(None, &config.base_image)?);
    let agent_pool = Arc::new(AgentConnPool::new());

    // Initialize storage backend (local or S3 via s3fs-fuse)
    let (storage, s3_mount_manager) = init_storage(&config.storage).await?;

    // Initialize NFS manager
    let nfs_mode = match config.nfs_mode.as_str() {
        "system" => NfsMode::System,
        _ => NfsMode::Embedded,
    };
    let nfs_manager = Arc::new(NfsManager::new(
        nfs_mode,
        config.nfs_port,
        config.get_nfs_host().to_string(),
        storage.clone(),
    ));

    // Start NFS server if embedded mode
    if let Err(e) = nfs_manager.start().await {
        error!("Failed to start NFS server: {}", e);
        // Non-fatal, continue without NFS
    }

    // Initialize workspace lease manager
    let lease_manager = Arc::new(
        SqliteLeaseManager::new(pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize lease manager: {}", e))?,
    );
    let holder_id = format!("server-{}", std::process::id());
    info!("Server holder ID: {}", holder_id);

    // Initialize services
    let workspace_service = Arc::new(WorkspaceService::new(
        workspace_repository.clone(),
        nfs_manager.clone(),
        storage.clone(),
        lease_manager as Arc<dyn infra::storage::lease::WorkspaceLeaseManager>,
        holder_id,
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

    // Add Prometheus metrics endpoint
    let metrics_handle_clone = metrics_handle.clone();
    app = app.route(
        "/metrics",
        axum::routing::get(move || {
            let handle = metrics_handle_clone.clone();
            async move { handle.render() }
        }),
    );
    info!("Prometheus metrics endpoint enabled at /metrics");

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
    let agent_grpc_server = api::grpc::create_server(agent_pool.clone());

    // Build gRPC router with all services
    let grpc_router = if config.fs_api_enabled {
        let fs_service = FileSystemServiceImpl::new(storage.clone());
        if let Some(ref token) = config.fs_api_token {
            info!("FileSystemService enabled with token authentication");
            let auth_interceptor = AuthInterceptor::new(token.clone());
            let fs_grpc_server = FileSystemServiceServer::with_interceptor(fs_service, auth_interceptor);
            Server::builder()
                .add_service(agent_grpc_server)
                .add_service(fs_grpc_server)
        } else {
            info!("FileSystemService enabled without authentication");
            let fs_grpc_server = FileSystemServiceServer::new(fs_service);
            Server::builder()
                .add_service(agent_grpc_server)
                .add_service(fs_grpc_server)
        }
    } else {
        info!("FileSystemService disabled");
        Server::builder().add_service(agent_grpc_server)
    };

    // Run both servers concurrently
    tokio::select! {
        result = http_server.with_graceful_shutdown(shutdown_signal()) => {
            if let Err(e) = result {
                tracing::error!("HTTP server error: {}", e);
            }
        }
        result = grpc_router.serve_with_shutdown(grpc_addr, shutdown_signal()) => {
            if let Err(e) = result {
                tracing::error!("gRPC server error: {}", e);
            }
        }
    }

    // Cleanup: unmount s3fs on shutdown
    if let Some(mount_mgr) = s3_mount_manager {
        info!("Unmounting s3fs...");
        if let Err(e) = mount_mgr.unmount().await {
            error!("Failed to unmount s3fs: {}", e);
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
