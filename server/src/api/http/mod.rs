//! HTTP API handlers

mod health;
mod workspace;
mod sandbox;
mod process;
mod pty;

use axum::{
    routing::{get, post, put, delete},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::AppState;

/// Create the HTTP router with all routes
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api_routes = Router::new()
        // Health check
        .route("/health", get(health::health_check))
        // Workspace routes
        .route("/workspaces", post(workspace::create_workspace))
        .route("/workspaces", get(workspace::list_workspaces))
        .route("/workspaces/{id}", get(workspace::get_workspace))
        .route("/workspaces/{id}", delete(workspace::delete_workspace))
        // Workspace file routes
        .route("/workspaces/{id}/files", get(workspace::read_file))
        .route("/workspaces/{id}/files", put(workspace::write_file))
        .route("/workspaces/{id}/files", delete(workspace::delete_file))
        .route("/workspaces/{id}/files/list", get(workspace::list_files))
        .route("/workspaces/{id}/files/mkdir", post(workspace::mkdir))
        .route("/workspaces/{id}/files/move", post(workspace::move_file))
        .route("/workspaces/{id}/files/copy", post(workspace::copy_file))
        .route("/workspaces/{id}/files/info", get(workspace::get_file_info))
        // Sandbox routes
        .route("/sandboxes", post(sandbox::create_sandbox))
        .route("/sandboxes", get(sandbox::list_sandboxes))
        .route("/sandboxes/{id}", get(sandbox::get_sandbox))
        .route("/sandboxes/{id}", delete(sandbox::delete_sandbox))
        // Process routes
        .route("/sandboxes/{id}/process/run", post(process::run_command))
        .route("/sandboxes/{id}/process/run/stream", get(process::run_command_stream))
        .route("/sandboxes/{id}/process/{pid}/kill", post(process::kill_process))
        // PTY routes
        .route("/sandboxes/{id}/pty", post(pty::create_pty))
        .route("/sandboxes/{id}/pty/{pty_id}", get(pty::pty_websocket))
        .route("/sandboxes/{id}/pty/{pty_id}/resize", post(pty::resize_pty))
        .route("/sandboxes/{id}/pty/{pty_id}", delete(pty::kill_pty));

    Router::new()
        .nest("/api/v1", api_routes)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
