//! HTTP API handlers

mod health;
mod sandbox;
mod process;
mod pty;
mod filesystem;

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
        .route("/sandboxes/{id}/pty/{pty_id}", delete(pty::kill_pty))
        // FileSystem routes
        .route("/sandboxes/{id}/files", get(filesystem::read_file))
        .route("/sandboxes/{id}/files", put(filesystem::write_file))
        .route("/sandboxes/{id}/files", delete(filesystem::delete_file))
        .route("/sandboxes/{id}/files/list", get(filesystem::list_files))
        .route("/sandboxes/{id}/files/mkdir", post(filesystem::mkdir))
        .route("/sandboxes/{id}/files/move", post(filesystem::move_file))
        .route("/sandboxes/{id}/files/copy", post(filesystem::copy_file))
        .route("/sandboxes/{id}/files/info", get(filesystem::get_file_info));

    Router::new()
        .nest("/api/v1", api_routes)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
