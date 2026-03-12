//! Tenant management API handlers (Admin only)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

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
            // Create namespace directory
            if let Err(e) = state.namespace_service.create_namespace_dir(&tenant.id).await {
                tracing::error!("Failed to create namespace dir for tenant {}: {}", tenant.id, e);
            }

            state.audit_service.log(
                &auth, "tenant.create", "tenant", &tenant.id, &tenant.name,
                serde_json::json!({}),
            );

            let mut response = serde_json::json!({ "tenant": tenant });
            if let Some((key, token)) = api_key {
                response["api_key"] = serde_json::json!({
                    "key": key,
                    "token": token,
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

    match state.tenant_repository.get_tenant(&id).await {
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

    let body = match axum::body::to_bytes(request.into_body(), 1024 * 64).await {
        Ok(b) => b,
        Err(_) => return bad_request("Invalid request body"),
    };
    let params: UpdateTenantParams = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => return bad_request(&format!("Invalid JSON: {}", e)),
    };

    match state.tenant_repository.update_tenant(&id, params).await {
        Ok(tenant) => {
            state.audit_service.log(
                &auth, "tenant.update", "tenant", &tenant.id, &tenant.name,
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
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = require_admin(auth) {
        return e.into_response();
    }

    match state.tenant_repository.activate_tenant(&id).await {
        Ok(tenant) => (StatusCode::OK, Json(serde_json::json!({ "tenant": tenant }))).into_response(),
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
        Ok(a) => a,
        Err(e) => return e.into_response(),
    };
    if let Err(e) = require_admin(auth) {
        return e.into_response();
    }

    match state.tenant_repository.deactivate_tenant(&id).await {
        Ok(tenant) => (StatusCode::OK, Json(serde_json::json!({ "tenant": tenant }))).into_response(),
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

    match state.tenant_repository.delete_tenant(&id, query.force).await {
        Ok(()) => {
            // Soft-delete namespace directory
            if let Err(e) = state.namespace_service.delete_namespace_dir(&id).await {
                tracing::error!("Failed to trash namespace dir for tenant {}: {}", id, e);
            }
            state.audit_service.log(
                &auth, "tenant.delete", "tenant", &id, "",
                serde_json::json!({"force": query.force}),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => error_response(e),
    }
}

fn bad_request(msg: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": { "code": "BAD_REQUEST", "message": msg }
        })),
    )
        .into_response()
}

pub fn error_response(e: crate::error::Error) -> axum::response::Response {
    let (status, code) = match &e {
        crate::error::Error::WorkspaceNotFound(_)
        | crate::error::Error::SandboxNotFound(_)
        | crate::error::Error::FileNotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        crate::error::Error::InvalidParameter(_)
        | crate::error::Error::InvalidRequest(_)
        | crate::error::Error::InvalidPath(_) => (StatusCode::BAD_REQUEST, "BAD_REQUEST"),
        crate::error::Error::WorkspaceHasActiveSandboxes => (StatusCode::CONFLICT, "HAS_ACTIVE_SANDBOXES"),
        crate::error::Error::PermissionDenied(_)
        | crate::error::Error::PathNotAllowed(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),
        crate::error::Error::FileAlreadyExists(_)
        | crate::error::Error::SandboxAlreadyExists(_) => (StatusCode::CONFLICT, "CONFLICT"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
    };

    (
        status,
        Json(serde_json::json!({
            "error": { "code": code, "message": e.to_string() }
        })),
    )
        .into_response()
}
