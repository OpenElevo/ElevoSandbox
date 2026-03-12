//! HTTP API handlers

pub mod api_key_handler;
pub mod audit_handler;
pub mod auth;
pub mod auth_handler;
pub mod dashboard_handler;
mod downloads;
mod health;
pub mod me_handler;
pub mod namespace_handler;
pub mod permission_handler;
mod process;
mod pty;
mod sandbox;
pub mod share_file_handler;
pub mod share_handler;
pub mod tenant_handler;
mod workspace;

use axum::{
    routing::{delete, get, head, post, put},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::AppState;

/// Create the HTTP router with all routes
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health::health_check))
        .route("/auth/login", post(auth_handler::login))
        .route(
            "/downloads/workspace-fuse/{platform}/{arch}",
            get(downloads::download_workspace_fuse),
        )
        .route(
            "/downloads/workspace-fuse/{platform}/{arch}",
            head(downloads::check_workspace_fuse),
        );

    // Authenticated routes (auth middleware applied)
    let authenticated_routes = Router::new()
        // Tenant management (Admin only)
        .route("/tenants", get(tenant_handler::list_tenants))
        .route("/tenants", post(tenant_handler::create_tenant))
        .route("/tenants/{id}", get(tenant_handler::get_tenant))
        .route("/tenants/{id}", put(tenant_handler::update_tenant))
        .route(
            "/tenants/{id}/activate",
            post(tenant_handler::activate_tenant),
        )
        .route(
            "/tenants/{id}/deactivate",
            post(tenant_handler::deactivate_tenant),
        )
        .route("/tenants/{id}", delete(tenant_handler::delete_tenant))
        // API Key management (Admin only)
        .route(
            "/tenants/{id}/keys",
            get(api_key_handler::list_api_keys),
        )
        .route(
            "/tenants/{id}/keys",
            post(api_key_handler::create_api_key),
        )
        .route(
            "/tenants/{id}/keys/{key_id}",
            delete(api_key_handler::revoke_api_key),
        )
        // Namespace file operations
        .route("/namespaces/{id}/files", get(namespace_handler::read_file))
        .route("/namespaces/{id}/files", put(namespace_handler::write_file))
        .route(
            "/namespaces/{id}/files",
            delete(namespace_handler::delete_file),
        )
        .route(
            "/namespaces/{id}/files/list",
            get(namespace_handler::list_files),
        )
        .route(
            "/namespaces/{id}/files/mkdir",
            post(namespace_handler::mkdir),
        )
        .route(
            "/namespaces/{id}/files/move",
            post(namespace_handler::move_file),
        )
        .route(
            "/namespaces/{id}/files/copy",
            post(namespace_handler::copy_file),
        )
        .route(
            "/namespaces/{id}/files/info",
            get(namespace_handler::get_file_info),
        )
        // Tenant self-service /me endpoints
        .route("/me", get(me_handler::get_me))
        .route("/me/files", get(me_handler::list_my_files))
        .route("/me/sandboxes", get(me_handler::list_my_sandboxes))
        .route("/me/shares", get(me_handler::list_my_shares))
        .route("/me/accessible-shares", get(me_handler::list_my_accessible_shares))
        // Share management
        .route("/shares", post(share_handler::create_share))
        .route("/shares", get(share_handler::list_shares))
        .route("/shares/{id}", get(share_handler::get_share))
        .route("/shares/{id}", put(share_handler::update_share))
        .route("/shares/{id}", delete(share_handler::delete_share))
        // Share file operations
        .route("/shares/{id}/files", get(share_file_handler::read_share_file))
        .route("/shares/{id}/files", put(share_file_handler::write_share_file))
        .route("/shares/{id}/files", delete(share_file_handler::delete_share_file))
        .route("/shares/{id}/files/list", get(share_file_handler::list_share_files))
        // Share permission management
        .route("/shares/{id}/permissions", get(permission_handler::list_permissions))
        .route("/shares/{id}/permissions", post(permission_handler::grant_permission))
        .route(
            "/shares/{id}/permissions/{tenant_id}",
            put(permission_handler::update_permission),
        )
        .route(
            "/shares/{id}/permissions/{tenant_id}",
            delete(permission_handler::revoke_permission),
        )
        // Tenant permissions (Admin only)
        .route(
            "/tenants/{id}/permissions",
            get(permission_handler::list_tenant_permissions),
        )
        // Audit logs (Admin only)
        .route("/audit-logs", get(audit_handler::list_audit_logs))
        // Dashboard stats (Admin only)
        .route("/dashboard/stats", get(dashboard_handler::get_stats))
        // Legacy workspace routes (kept for backward compatibility)
        .route("/workspaces", post(workspace::create_workspace))
        .route("/workspaces", get(workspace::list_workspaces))
        .route("/workspaces/{id}", get(workspace::get_workspace))
        .route("/workspaces/{id}", delete(workspace::delete_workspace))
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
        .route(
            "/sandboxes/{id}/process/run/stream",
            get(process::run_command_stream),
        )
        .route(
            "/sandboxes/{id}/process/{pid}/kill",
            post(process::kill_process),
        )
        // PTY routes
        .route("/sandboxes/{id}/pty", post(pty::create_pty))
        .route("/sandboxes/{id}/pty/{pty_id}", get(pty::pty_websocket))
        .route("/sandboxes/{id}/pty/{pty_id}/resize", post(pty::resize_pty))
        .route("/sandboxes/{id}/pty/{pty_id}", delete(pty::kill_pty))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    // SPA fallback: serve admin frontend static files
    let spa = ServeDir::new("web/dist").fallback(ServeFile::new("web/dist/index.html"));

    Router::new()
        .nest("/api/v1", public_routes.merge(authenticated_routes))
        .nest_service("/admin", spa)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
