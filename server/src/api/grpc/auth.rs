//! gRPC authentication for FileSystemService and ClientStorageService
//!
//! Supports two authentication paths:
//! - JWT: admin authentication via `Authorization: Bearer <jwt>`
//! - API Key: tenant authentication via `Authorization: Bearer sk_<token>` (async DB lookup)
//!
//! Path-aware: only authenticates FileSystemService and ClientStorageService.
//! AgentService and other internal gRPC services pass through without auth.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tonic::{body::BoxBody, codegen::http};
use tower::{Layer, Service};

use uuid::Uuid;

use crate::api::http::auth::AuthConfig;
use crate::infra::oidc::OidcService;
use crate::infra::tenant_repository::TenantRepository;
use crate::service::api_key_usage::ApiKeyUsageTracker;

/// Identity extracted by the gRPC auth layer and injected into request extensions.
///
/// FileSystemService and other gRPC services can extract this to perform
/// authorization checks (e.g., namespace ownership, share permissions).
#[derive(Clone, Debug)]
pub enum GrpcIdentity {
    /// Admin (JWT authenticated)
    Admin,
    /// Tenant (API Key authenticated)
    #[allow(dead_code)]
    Tenant { tenant_id: Uuid },
    /// Dev mode (no auth required)
    DevMode,
}

/// gRPC service paths that require authentication.
const AUTHENTICATED_SERVICE_PREFIXES: &[&str] = &[
    "/workspace.v1.FileSystemService/",
    "/workspace.v1.ClientStorageService/",
    "/workspace.v1.WorkspaceService/",
    "/workspace.v1.SandboxService/",
    "/workspace.v1.ProcessService/",
    "/workspace.v1.PtyService/",
];

/// gRPC service paths that are exempt from authentication (internal services).
const UNAUTHENTICATED_SERVICE_PREFIXES: &[&str] = &[
    "/workspace.v1.AgentService/",
    "/grpc.health.v1.Health/",
    "/grpc.reflection.v1alpha.ServerReflection/",
];

// ── Async tower auth layer (supports JWT + API Key) ──

/// Tower layer that adds async authentication to gRPC services.
///
/// Path-aware: only authenticates specific gRPC services, passes through
/// AgentService and other internal services without auth.
#[derive(Clone)]
pub struct GrpcAuthLayer {
    tenant_repository: TenantRepository,
    auth_config: AuthConfig,
    api_key_usage: Arc<ApiKeyUsageTracker>,
    oidc_service: Arc<tokio::sync::RwLock<Option<Arc<OidcService>>>>,
}

impl GrpcAuthLayer {
    pub fn new(
        tenant_repository: TenantRepository,
        auth_config: AuthConfig,
        api_key_usage: Arc<ApiKeyUsageTracker>,
        oidc_service: Arc<tokio::sync::RwLock<Option<Arc<OidcService>>>>,
    ) -> Self {
        Self {
            tenant_repository,
            auth_config,
            api_key_usage,
            oidc_service,
        }
    }
}

impl<S> Layer<S> for GrpcAuthLayer {
    type Service = GrpcAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcAuthService {
            inner,
            tenant_repository: self.tenant_repository.clone(),
            auth_config: self.auth_config.clone(),
            api_key_usage: self.api_key_usage.clone(),
            oidc_service: self.oidc_service.clone(),
        }
    }
}

/// Tower service wrapper that authenticates gRPC requests.
#[derive(Clone)]
pub struct GrpcAuthService<S> {
    inner: S,
    tenant_repository: TenantRepository,
    auth_config: AuthConfig,
    api_key_usage: Arc<ApiKeyUsageTracker>,
    oidc_service: Arc<tokio::sync::RwLock<Option<Arc<OidcService>>>>,
}

/// Check if a request path requires authentication.
fn requires_auth(path: &str) -> bool {
    // Explicitly unauthenticated services
    for prefix in UNAUTHENTICATED_SERVICE_PREFIXES {
        if path.starts_with(prefix) {
            return false;
        }
    }
    // Explicitly authenticated services
    for prefix in AUTHENTICATED_SERVICE_PREFIXES {
        if path.starts_with(prefix) {
            return true;
        }
    }
    // Unknown services default to requiring auth
    true
}

impl<S, ReqBody> Service<http::Request<ReqBody>> for GrpcAuthService<S>
where
    S: Service<http::Request<ReqBody>, Response = http::Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    ReqBody: Send + 'static,
{
    type Response = http::Response<BoxBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: http::Request<ReqBody>) -> Self::Future {
        // Clone inner service (standard tower pattern for async call)
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        let tenant_repo = self.tenant_repository.clone();
        let auth_config = self.auth_config.clone();
        let api_key_usage = self.api_key_usage.clone();
        let oidc_service = self.oidc_service.clone();

        Box::pin(async move {
            // Skip auth for unauthenticated services (AgentService, health, etc.)
            if !requires_auth(req.uri().path()) {
                return inner.call(req).await;
            }

            // Dev mode: skip authentication, inject DevMode identity
            if auth_config.dev_mode {
                req.extensions_mut().insert(GrpcIdentity::DevMode);
                return inner.call(req).await;
            }

            // Extract Bearer token from Authorization header
            let auth_header = req
                .headers()
                .get(http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let token = match auth_header
                .as_deref()
                .and_then(|h| h.strip_prefix("Bearer "))
            {
                Some(t) => t,
                None => {
                    return Ok(unauthenticated_response("missing authorization header"));
                }
            };

            // Route 1: API Key (sk_ prefix) — async DB lookup
            if token.starts_with("sk_") {
                match validate_api_key(&tenant_repo, &api_key_usage, token).await {
                    Ok(tenant_id) => {
                        req.extensions_mut()
                            .insert(GrpcIdentity::Tenant { tenant_id });
                        return inner.call(req).await;
                    }
                    Err(msg) => return Ok(unauthenticated_response(&msg)),
                }
            }

            // Route 2: JWT path — dispatch by algorithm
            if token.starts_with("eyJ") {
                let alg = crate::infra::oidc::jwt_utils::extract_jwt_alg(token);
                match alg.as_deref() {
                    Some("RS256") => {
                        // ElevoOne RS256 token — verify via OidcService
                        let oidc_guard = oidc_service.read().await;
                        match validate_elevoone_token(&oidc_guard, &tenant_repo, token).await {
                            Ok(tenant_id) => {
                                req.extensions_mut()
                                    .insert(GrpcIdentity::Tenant { tenant_id });
                                return inner.call(req).await;
                            }
                            Err(msg) => return Ok(unauthenticated_response(&msg)),
                        }
                    }
                    _ => {
                        // Default: treat as HS256 (our own admin JWT)
                        match auth_config.verify_jwt_public(token) {
                            Ok(_claims) => {
                                req.extensions_mut().insert(GrpcIdentity::Admin);
                                inner.call(req).await
                            }
                            Err(_) => Ok(unauthenticated_response("invalid or expired token")),
                        }
                    }
                }
            } else {
                return Ok(unauthenticated_response("invalid token format"));
            }
        })
    }
}

/// Validate an API Key token by hashing and looking up in the database.
/// Returns the tenant_id on success. Usage is tracked via the batching tracker.
async fn validate_api_key(
    repo: &TenantRepository,
    tracker: &ApiKeyUsageTracker,
    token: &str,
) -> Result<Uuid, String> {
    let result = repo
        .find_by_token_hash(token)
        .await
        .map_err(|e| format!("auth error: {}", e))?;

    let (key, tenant) = match result {
        Some(pair) => pair,
        None => return Err("unknown API key".to_string()),
    };

    if !key.is_usable() {
        return Err("API key revoked or expired".to_string());
    }

    if !tenant.is_active {
        return Err("tenant is deactivated".to_string());
    }

    // Track usage via the batching tracker (coalesces writes per key)
    tracker.update(key.id);

    Ok(tenant.id)
}

/// Validate an ElevoOne RS256 token via OidcService.
/// Returns the tenant_id on success.
async fn validate_elevoone_token(
    oidc_service: &Option<Arc<OidcService>>,
    tenant_repo: &TenantRepository,
    token: &str,
) -> Result<Uuid, String> {
    let svc = oidc_service
        .as_ref()
        .ok_or_else(|| "OIDC not configured".to_string())?;

    let identity = svc
        .verify_and_resolve_tenant(token, tenant_repo)
        .await
        .map_err(|_| "invalid or expired token".to_string())?;

    match identity {
        crate::domain::auth::Identity::Tenant { id, .. } => Ok(id),
        _ => Err("unexpected identity from OIDC token".to_string()),
    }
}

/// Build an HTTP 401 response compatible with tonic's BoxBody.
fn unauthenticated_response(message: &str) -> http::Response<BoxBody> {
    let status = tonic::Status::unauthenticated(message);
    status.into_http()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Path-based auth routing tests ──

    #[test]
    fn test_requires_auth_filesystem() {
        assert!(requires_auth("/workspace.v1.FileSystemService/Stat"));
        assert!(requires_auth("/workspace.v1.FileSystemService/ReadFile"));
    }

    #[test]
    fn test_requires_auth_agent_exempt() {
        assert!(!requires_auth("/workspace.v1.AgentService/Connect"));
    }

    #[test]
    fn test_requires_auth_health_exempt() {
        assert!(!requires_auth("/grpc.health.v1.Health/Check"));
    }

    #[test]
    fn test_requires_auth_other_services() {
        assert!(requires_auth("/workspace.v1.SandboxService/Create"));
        assert!(requires_auth("/workspace.v1.WorkspaceService/List"));
    }

    #[test]
    fn test_requires_auth_unknown_defaults_to_true() {
        assert!(requires_auth("/unknown.Service/Method"));
    }
}
