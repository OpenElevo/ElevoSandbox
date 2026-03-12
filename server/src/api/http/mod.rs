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

use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, head, post, put},
    Json, Router,
};
use governor::{
    clock::DefaultClock,
    state::keyed::DashMapStateStore,
    Quota, RateLimiter,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::domain::auth::AuthContext;
use crate::AppState;

type GlobalLimiter = RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>;
type KeyedIpLimiter = RateLimiter<IpAddr, DashMapStateStore<IpAddr>, DefaultClock>;

/// State bundle for rate-limiting middlewares: pairs the limiter with the
/// trusted proxy IP list so that IP extraction uses the correct source.
struct RateLimitState<L> {
    limiter: Arc<L>,
    trusted_proxy_ips: Arc<Vec<String>>,
}

impl<L> Clone for RateLimitState<L> {
    fn clone(&self) -> Self {
        Self {
            limiter: Arc::clone(&self.limiter),
            trusted_proxy_ips: Arc::clone(&self.trusted_proxy_ips),
        }
    }
}

/// Extract client IP from request for rate-limiting purposes.
///
/// Delegates to the auth module's `extract_client_ip` which honours the
/// `TRUSTED_PROXY_IPS` allowlist.  Falls back to `UNSPECIFIED` when no IP
/// can be determined (so the rate-limiter still has a key to work with).
fn extract_client_ip_for_limiter(request: &Request, trusted_proxy_ips: &[String]) -> IpAddr {
    auth::extract_client_ip(request, trusted_proxy_ips)
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}

/// Global per-IP rate limiting middleware
async fn global_rate_limit(
    axum::extract::State(state): axum::extract::State<RateLimitState<GlobalLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    let ip = extract_client_ip_for_limiter(&request, &state.trusted_proxy_ips);
    match state.limiter.check_key(&ip) {
        Ok(_) => next.run(request).await,
        Err(_) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": {"code": "RATE_LIMITED", "message": "Too many requests"}
            })),
        )
            .into_response(),
    }
}

/// Login-specific rate limiting middleware (per-IP, stricter)
async fn login_rate_limit(
    axum::extract::State(state): axum::extract::State<RateLimitState<KeyedIpLimiter>>,
    request: Request,
    next: Next,
) -> Response {
    let ip = extract_client_ip_for_limiter(&request, &state.trusted_proxy_ips);
    match state.limiter.check_key(&ip) {
        Ok(_) => next.run(request).await,
        Err(_) => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": {"code": "RATE_LIMITED", "message": "Too many login attempts, please try again later"}
            })),
        )
            .into_response(),
    }
}

/// Admin-enforcement middleware: requires the request to carry an Admin identity.
///
/// Must be applied *after* `auth_middleware` (which populates the `AuthContext` extension).
/// Returns 403 Forbidden if the authenticated identity is not an admin.
async fn require_admin_middleware(request: Request, next: Next) -> Response {
    match request.extensions().get::<AuthContext>() {
        Some(auth) if auth.is_admin() => next.run(request).await,
        Some(_) => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": {"code": "FORBIDDEN", "message": "admin access required"}
            })),
        )
            .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {"code": "UNAUTHORIZED", "message": "authentication required"}
            })),
        )
            .into_response(),
    }
}

/// Create the HTTP router with all routes
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Shared trusted proxy IPs list, derived from config once at startup.
    let trusted_proxy_ips = Arc::new(state.config.trusted_proxy_ips.clone());

    // Login-specific rate limiter: 10 requests per minute per IP
    let login_rate_state = RateLimitState {
        limiter: Arc::new(RateLimiter::dashmap(
            Quota::per_minute(NonZeroU32::new(10).unwrap()),
        )),
        trusted_proxy_ips: trusted_proxy_ips.clone(),
    };

    // Global per-IP rate limiter: configured RPS (default 100)
    let rps = state.config.rate_limit_rps.max(1);
    let global_rate_state = RateLimitState {
        limiter: Arc::new(RateLimiter::dashmap(
            Quota::per_second(NonZeroU32::new(rps).unwrap()),
        )),
        trusted_proxy_ips: trusted_proxy_ips.clone(),
    };

    // Login route with its own stricter rate limiter
    let login_route = Router::new()
        .route("/auth/login", post(auth_handler::login))
        .route_layer(middleware::from_fn_with_state(
            login_rate_state,
            login_rate_limit,
        ));

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health::health_check))
        .merge(login_route)
        .route(
            "/downloads/workspace-fuse/{platform}/{arch}",
            get(downloads::download_workspace_fuse),
        )
        .route(
            "/downloads/workspace-fuse/{platform}/{arch}",
            head(downloads::check_workspace_fuse),
        );

    // Admin-only routes — requires Admin JWT (per-handler require_admin() checks enforce this)
    let admin_routes = Router::new()
        // Tenant management
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
        // API Key management
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
            post(namespace_handler::write_file),
        )
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
        // Tenant permissions
        .route(
            "/tenants/{id}/permissions",
            get(permission_handler::list_tenant_permissions),
        )
        // Audit logs
        .route("/audit-logs", get(audit_handler::list_audit_logs))
        // Dashboard stats
        .route("/dashboard/stats", get(dashboard_handler::get_stats))
        // Layers are applied innermost-first (bottom to top in execution order):
        // 1. auth_middleware runs first to populate AuthContext
        // 2. require_admin_middleware runs after to enforce admin-only access
        .layer(axum::middleware::from_fn(require_admin_middleware))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ));

    // Dual-auth routes — accepts Admin JWT or Tenant API Key
    // Nested under /api/v1
    let authenticated_routes = Router::new()
        // Tenant self-service /me endpoints
        .route("/me", get(me_handler::get_me))
        .route("/me/files", get(me_handler::list_my_files))
        .route("/me/files/*path", get(me_handler::read_my_file))
        .route("/me/files/*path", put(me_handler::write_my_file))
        .route("/me/files/*path", post(me_handler::create_my_file))
        .route("/me/files/*path", delete(me_handler::delete_my_file))
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
        .route(
            "/shares/{id}/files",
            post(share_file_handler::write_share_file),
        )
        .route(
            "/shares/{id}/files",
            delete(share_file_handler::delete_share_file),
        )
        .route(
            "/shares/{id}/files/list",
            get(share_file_handler::list_share_files),
        )
        // Share permission management
        .route(
            "/shares/{id}/permissions",
            get(permission_handler::list_permissions),
        )
        .route(
            "/shares/{id}/permissions",
            post(permission_handler::grant_permission),
        )
        .route(
            "/shares/{id}/permissions/{tenant_id}",
            put(permission_handler::update_permission),
        )
        .route(
            "/shares/{id}/permissions/{tenant_id}",
            delete(permission_handler::revoke_permission),
        )
        // Sandbox routes
        .route("/sandboxes", post(sandbox::create_sandbox))
        .route("/sandboxes", get(sandbox::list_sandboxes))
        .route("/sandboxes/batch-delete", post(sandbox::batch_delete_sandboxes))
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
        .nest("/api/v1", public_routes)
        .nest("/api/v1", admin_routes)
        .nest("/api/v1", authenticated_routes)
        .nest_service("/admin", spa)
        .layer(middleware::from_fn_with_state(
            global_rate_state,
            global_rate_limit,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
