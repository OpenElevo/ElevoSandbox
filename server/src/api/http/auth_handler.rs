//! Authentication API handlers

use axum::{
    extract::{Request, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

/// POST /api/v1/auth/login
pub async fn login(
    State(state): State<AppState>,
    request: Request,
) -> impl IntoResponse {
    // Extract client IP for JWT claims
    let ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            request
                .headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        });

    // Parse body
    let body = match axum::body::to_bytes(request.into_body(), 1024 * 16).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "BAD_REQUEST", "message": "Invalid request body" }
                })),
            )
                .into_response();
        }
    };

    let login_req: LoginRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": { "code": "BAD_REQUEST", "message": "Invalid JSON: expected { \"password\": \"...\" }" }
                })),
            )
                .into_response();
        }
    };

    // Dev mode: no password required
    if state.auth_config.dev_mode {
        match state.auth_config.create_admin_token(ip) {
            Ok(token) => {
                return (StatusCode::OK, Json(LoginResponse { token })).into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": { "code": "INTERNAL_ERROR", "message": e.to_string() }
                    })),
                )
                    .into_response();
            }
        }
    }

    // Verify password
    let expected = state.auth_config.admin_password.as_deref().unwrap_or("");
    if login_req.password != expected {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": { "code": "UNAUTHORIZED", "message": "Invalid password" }
            })),
        )
            .into_response();
    }

    // Generate JWT
    match state.auth_config.create_admin_token(ip) {
        Ok(token) => (StatusCode::OK, Json(LoginResponse { token })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": { "code": "INTERNAL_ERROR", "message": e.to_string() }
            })),
        )
            .into_response(),
    }
}
