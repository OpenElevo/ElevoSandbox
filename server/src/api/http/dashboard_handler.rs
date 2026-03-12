//! Dashboard statistics API handler

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use crate::domain::auth::AuthContext;
use crate::AppState;

/// GET /api/v1/dashboard/stats — dashboard statistics (Admin only)
pub async fn get_stats(
    State(state): State<AppState>,
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

    // Single query with multiple COUNT subqueries
    let stats: Result<(i64, i64, i64, i64, i64), _> = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*) FROM tenants) AS total_tenants,
            (SELECT COUNT(*) FROM tenants WHERE is_active = true) AS active_tenants,
            (SELECT COUNT(*) FROM shares) AS total_shares,
            (SELECT COUNT(*) FROM sandboxes WHERE state = 'running') AS running_sandboxes,
            (SELECT COUNT(*) FROM api_keys WHERE is_active = true) AS active_api_keys
        "#,
    )
    .fetch_one(&state.pool)
    .await;

    match stats {
        Ok((total_tenants, active_tenants, total_shares, running_sandboxes, active_api_keys)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "tenants": { "total": total_tenants, "active": active_tenants },
                "shares": { "total": total_shares },
                "sandboxes": { "running": running_sandboxes },
                "api_keys": { "active": active_api_keys }
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"code": "INTERNAL", "message": format!("{}", e)}})),
        )
            .into_response(),
    }
}
