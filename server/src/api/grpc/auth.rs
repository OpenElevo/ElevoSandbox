//! gRPC authentication for FileSystemService and ClientStorageService
//!
//! Supports three authentication paths:
//! - Legacy `fs_api_token`: exact Bearer token match (backward compat for FUSE clients)
//! - JWT: admin authentication via `Authorization: Bearer <jwt>`
//! - API Key: tenant authentication via `Authorization: Bearer sk_<token>` (async DB lookup)
//!
//! Path-aware: only authenticates FileSystemService and ClientStorageService.
//! AgentService and other internal gRPC services pass through without auth.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tonic::{body::BoxBody, codegen::http, Request, Status};
use tower::{Layer, Service};
use tracing::warn;

use crate::api::http::auth::AuthConfig;
use crate::infra::tenant_repository::TenantRepository;

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

// ── Legacy synchronous interceptor (kept for backward compat) ──

/// Simple token-based authentication interceptor.
///
/// Validates Bearer tokens against a pre-configured secret.
/// Kept for backward compatibility with direct interceptor usage.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AuthInterceptor {
    valid_token: String,
}

#[allow(dead_code)]
impl AuthInterceptor {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            valid_token: token.into(),
        }
    }

    #[allow(clippy::result_large_err)]
    pub fn authenticate<T>(&self, request: Request<T>) -> Result<Request<T>, Status> {
        let token = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        match token {
            Some(t) if t == self.valid_token => Ok(request),
            Some(_) => Err(Status::unauthenticated("invalid token")),
            None => Err(Status::unauthenticated("missing authorization header")),
        }
    }
}

impl tonic::service::Interceptor for AuthInterceptor {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
        self.authenticate(request)
    }
}

// ── Async tower auth layer (supports JWT + API Key + legacy token) ──

/// Tower layer that adds async authentication to gRPC services.
///
/// Path-aware: only authenticates specific gRPC services, passes through
/// AgentService and other internal services without auth.
#[derive(Clone)]
pub struct GrpcAuthLayer {
    tenant_repository: TenantRepository,
    auth_config: AuthConfig,
    legacy_token: Option<String>,
}

impl GrpcAuthLayer {
    pub fn new(
        tenant_repository: TenantRepository,
        auth_config: AuthConfig,
        legacy_token: Option<String>,
    ) -> Self {
        Self {
            tenant_repository,
            auth_config,
            legacy_token,
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
            legacy_token: self.legacy_token.clone(),
        }
    }
}

/// Tower service wrapper that authenticates gRPC requests.
#[derive(Clone)]
pub struct GrpcAuthService<S> {
    inner: S,
    tenant_repository: TenantRepository,
    auth_config: AuthConfig,
    legacy_token: Option<String>,
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
    S: Service<http::Request<ReqBody>, Response = http::Response<BoxBody>>
        + Clone
        + Send
        + 'static,
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

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        // Clone inner service (standard tower pattern for async call)
        let mut inner = self.inner.clone();
        std::mem::swap(&mut self.inner, &mut inner);

        let tenant_repo = self.tenant_repository.clone();
        let auth_config = self.auth_config.clone();
        let legacy_token = self.legacy_token.clone();

        Box::pin(async move {
            // Skip auth for unauthenticated services (AgentService, health, etc.)
            if !requires_auth(req.uri().path()) {
                return inner.call(req).await;
            }

            // Dev mode: skip authentication
            if auth_config.dev_mode {
                return inner.call(req).await;
            }

            // Extract Bearer token from Authorization header
            let auth_header = req
                .headers()
                .get(http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let token = match auth_header.as_deref() {
                Some(h) if h.starts_with("Bearer ") => &h[7..],
                _ => {
                    return Ok(unauthenticated_response("missing authorization header"));
                }
            };

            // Route 1: Legacy fs_api_token (backward compat for FUSE clients)
            if let Some(ref legacy) = legacy_token {
                if constant_time_eq(token.as_bytes(), legacy.as_bytes()) {
                    return inner.call(req).await;
                }
            }

            // Route 2: API Key (sk_ prefix) — async DB lookup
            if token.starts_with("sk_") {
                match validate_api_key(&tenant_repo, token).await {
                    Ok(()) => return inner.call(req).await,
                    Err(msg) => return Ok(unauthenticated_response(&msg)),
                }
            }

            // Route 3: JWT verification (synchronous)
            match auth_config.verify_jwt_public(token) {
                Ok(_claims) => inner.call(req).await,
                Err(e) => Ok(unauthenticated_response(&format!("JWT: {}", e))),
            }
        })
    }
}

/// Validate an API Key token by hashing and looking up in the database.
async fn validate_api_key(repo: &TenantRepository, token: &str) -> Result<(), String> {
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

    // Fire-and-forget last_used update
    let repo = repo.clone();
    let key_id = key.id.clone();
    tokio::spawn(async move {
        if let Err(e) = repo.update_last_used(&key_id).await {
            warn!("Failed to update API key last_used_at: {}", e);
        }
    });

    Ok(())
}

/// Build an HTTP 401 response compatible with tonic's BoxBody.
fn unauthenticated_response(message: &str) -> http::Response<BoxBody> {
    let status = tonic::Status::unauthenticated(message);
    status.into_http()
}

/// Constant-time byte comparison to prevent timing side-channels.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    // ── AuthInterceptor tests ──

    #[test]
    fn test_valid_token() {
        let interceptor = AuthInterceptor::new("secret123");
        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from("Bearer secret123").unwrap(),
        );

        assert!(interceptor.authenticate(request).is_ok());
    }

    #[test]
    fn test_invalid_token() {
        let interceptor = AuthInterceptor::new("secret123");
        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from("Bearer wrongtoken").unwrap(),
        );

        let result = interceptor.authenticate(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_missing_token() {
        let interceptor = AuthInterceptor::new("secret123");
        let request = Request::new(());

        let result = interceptor.authenticate(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_malformed_header() {
        let interceptor = AuthInterceptor::new("secret123");
        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from("Basic secret123").unwrap(),
        );

        let result = interceptor.authenticate(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

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

    // ── Utility tests ──

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
        assert!(constant_time_eq(b"", b""));
    }
}
