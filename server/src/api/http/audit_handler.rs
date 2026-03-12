//! Audit log API handler

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use crate::domain::audit::AuditLogFilter;
use crate::domain::auth::AuthContext;
use crate::domain::tenant::Pagination;
use crate::AppState;

/// GET /api/v1/audit-logs — list audit logs (Admin only)
pub async fn list_audit_logs(
    State(state): State<AppState>,
    Query(filter): Query<AuditLogFilter>,
    Query(pagination): Query<Pagination>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let auth = match request.extensions().get::<AuthContext>() {
        Some(a) => a,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED"}})),
            )
                .into_response()
        }
    };

    if !auth.is_admin() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "Admin only"}})),
        )
            .into_response();
    }

    match state.audit_repository.list(filter, pagination).await {
        Ok((logs, total)) => (
            StatusCode::OK,
            Json(serde_json::json!({"logs": logs, "total": total})),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}
