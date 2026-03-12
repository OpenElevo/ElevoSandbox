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
                Json(serde_json::json!({"error": {"code": "UNAUTHORIZED", "message": "未授权访问"}})),
            )
                .into_response()
        }
    };

    if !auth.is_admin() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": {"code": "FORBIDDEN", "message": "权限不足"}})),
        )
            .into_response();
    }

    let page = pagination.page.max(1);
    let page_size = pagination.page_size.clamp(1, 100);
    let clamped = crate::domain::tenant::Pagination { page, page_size };

    match state.audit_repository.list(filter, clamped).await {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "items": result.items,
                "total": result.total,
                "page": result.page,
                "page_size": result.page_size
            })),
        )
            .into_response(),
        Err(e) => super::tenant_handler::error_response(e),
    }
}
