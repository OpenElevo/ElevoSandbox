//! Tenant management API handlers (Admin only)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::auth::AuthContext;
use crate::domain::tenant::{
    CreateTenantParams, Pagination, TenantFilter, UpdateTenantParams,
};
use crate::AppState;

/// Helper to extract AuthContext from request extensions
fn get_auth(extensions: &axum::http::Extensions) -> Result<&AuthContext, impl IntoResponse> {
    extensions.get::<AuthContext>().ok_or((
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error": { "code": "UNAUTHORIZED", "message": "Not authenticated" }
        })),
    ))
}

/// Helper to require admin access
fn require_admin(auth: &AuthContext) -> Result<(), impl IntoResponse> {
    if auth.is_admin() {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": { "code": "FORBIDDEN", "message": "Admin access required" }
            })),
        ))
    }
}

/// GET /api/v1/tenants
pub async fn list_tenants(
    State(state): State<AppState>,
    Query(filter): Query<TenantFilter>,
    Query(pagination): Query<Pagination>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match get_auth(request.extensions()) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = require_admin(auth) {
        return e.into_response();
    }

    match state.tenant_repository.list_tenants(filter, pagination).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => error_response(e),
    }
}

/// POST /api/v1/tenants
pub async fn create_tenant(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match get_auth(request.extensions()) {
        Ok(a) => a.clone(),
        Err(e) => return e.into_response(),
    };
    if let Err(e) = require_admin(&auth) {
        return e.into_response();
    }

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return bad_request("Invalid request body"),
    };
    let params: CreateTenantParams = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => return bad_request(&format!("Invalid JSON: {}", e)),
    };

    if params.name.trim().is_empty() {
        return bad_request("Tenant name is required");
    }

    match state.tenant_repository.create_tenant(params).await {
        Ok((tenant, api_key)) => {
            // Create namespace directory — roll back tenant on failure
            if let Err(e) = state.namespace_service.create_namespace_dir(tenant.id).await {
                tracing::error!("Failed to create namespace dir for tenant {}: {}", tenant.id, e);
                if let Err(del_err) = state.tenant_repository.delete_tenant(tenant.id, true).await {
                    tracing::error!("Failed to roll back tenant creation: {}", del_err);
                }
                return simple_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "Failed to create namespace directory",
                );
            }

            state.audit_service.log(
                &auth, "tenant.create", "tenant", tenant.id, &tenant.name,
                serde_json::json!({}),
            );

            let mut response = serde_json::json!({ "tenant": tenant });
            if let Some((key, token)) = api_key {
                response["api_key"] = serde_json::json!({
                    "id": key.id,
                    "name": key.name,
                    "token": token,
                    "token_prefix": key.token_prefix,
                    "expires_at": key.expires_at,
                });
            }
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// GET /api/v1/tenants/:id
pub async fn get_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match get_auth(request.extensions()) {
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = require_admin(auth) {
        return e.into_response();
    }

    let tenant_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    match state.tenant_repository.get_tenant(tenant_id).await {
        Ok(tenant) => (StatusCode::OK, Json(serde_json::json!({ "tenant": tenant }))).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/v1/tenants/:id
pub async fn update_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match get_auth(request.extensions()) {
        Ok(a) => a.clone(),
        Err(e) => return e.into_response(),
    };
    if let Err(e) = require_admin(&auth) {
        return e.into_response();
    }

    let tenant_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return bad_request("Invalid request body"),
    };
    let params: UpdateTenantParams = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => return bad_request(&format!("Invalid JSON: {}", e)),
    };

    match state.tenant_repository.update_tenant(tenant_id, params).await {
        Ok(tenant) => {
            state.audit_service.log(
                &auth, "tenant.update", "tenant", tenant.id, &tenant.name,
                serde_json::json!({}),
            );
            (StatusCode::OK, Json(serde_json::json!({ "tenant": tenant }))).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// POST /api/v1/tenants/:id/activate
pub async fn activate_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match get_auth(request.extensions()) {
        Ok(a) => a.clone(),
        Err(e) => return e.into_response(),
    };
    if let Err(e) = require_admin(&auth) {
        return e.into_response();
    }

    let tenant_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    match state.tenant_repository.activate_tenant(tenant_id).await {
        Ok(tenant) => {
            state.audit_service.log(
                &auth, "tenant.update", "tenant", tenant.id, &tenant.name,
                serde_json::json!({"is_active": true}),
            );
            (StatusCode::OK, Json(serde_json::json!({ "tenant": tenant }))).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// POST /api/v1/tenants/:id/deactivate
pub async fn deactivate_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match get_auth(request.extensions()) {
        Ok(a) => a.clone(),
        Err(e) => return e.into_response(),
    };
    if let Err(e) = require_admin(&auth) {
        return e.into_response();
    }

    let tenant_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    match state.tenant_repository.deactivate_tenant(tenant_id).await {
        Ok(tenant) => {
            state.audit_service.log(
                &auth, "tenant.update", "tenant", tenant.id, &tenant.name,
                serde_json::json!({"is_active": false}),
            );
            (StatusCode::OK, Json(serde_json::json!({ "tenant": tenant }))).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    #[serde(default)]
    pub force: bool,
}

/// DELETE /api/v1/tenants/:id
pub async fn delete_tenant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DeleteQuery>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match get_auth(request.extensions()) {
        Ok(a) => a.clone(),
        Err(e) => return e.into_response(),
    };
    if let Err(e) = require_admin(&auth) {
        return e.into_response();
    }

    let tenant_id = match Uuid::parse_str(&id) {
        Ok(u) => u,
        Err(_) => return bad_request("Invalid tenant ID"),
    };

    // Fetch tenant name before deleting so the audit log has a meaningful resource_name
    let tenant_name = match state.tenant_repository.get_tenant(tenant_id).await {
        Ok(t) => t.name,
        Err(e) => return error_response(e),
    };

    match state.tenant_repository.delete_tenant(tenant_id, query.force).await {
        Ok(()) => {
            // Soft-delete namespace directory
            if let Err(e) = state.namespace_service.delete_namespace_dir(tenant_id).await {
                tracing::error!("Failed to trash namespace dir for tenant {}: {}", tenant_id, e);
            }
            state.audit_service.log(
                &auth, "tenant.delete", "tenant", tenant_id, &tenant_name,
                serde_json::json!({"force": query.force}),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => error_response(e),
    }
}

fn bad_request(msg: &str) -> axum::response::Response {
    simple_error_response(StatusCode::BAD_REQUEST, "BAD_REQUEST", msg)
}

/// Build a plain error response from explicit status, code, and message strings.
fn simple_error_response(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

/// Build an error response from a domain `Error` value.
pub fn error_response(e: crate::error::Error) -> axum::response::Response {
    let (status, code) = match &e {
        crate::error::Error::WorkspaceNotFound(_)
        | crate::error::Error::SandboxNotFound(_)
        | crate::error::Error::FileNotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        crate::error::Error::InvalidParameter(_)
        | crate::error::Error::InvalidRequest(_)
        | crate::error::Error::InvalidPath(_) => (StatusCode::BAD_REQUEST, "BAD_REQUEST"),
        crate::error::Error::WorkspaceHasActiveSandboxes => (StatusCode::CONFLICT, "HAS_ACTIVE_SANDBOXES"),
        crate::error::Error::HasActiveShares => (StatusCode::CONFLICT, "HAS_ACTIVE_SHARES"),
        crate::error::Error::HasActiveApiKeys(_) => (StatusCode::CONFLICT, "HAS_ACTIVE_API_KEYS"),
        crate::error::Error::PermissionDenied(_)
        | crate::error::Error::PathNotAllowed(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),
        crate::error::Error::FileAlreadyExists(_)
        | crate::error::Error::SandboxAlreadyExists(_) => (StatusCode::CONFLICT, "CONFLICT"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
    };

    simple_error_response(status, code, &e.to_string())
}
