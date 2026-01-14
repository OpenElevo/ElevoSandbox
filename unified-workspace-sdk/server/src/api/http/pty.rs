//! PTY HTTP handlers

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::domain::types::PtyOptions;
use crate::{AppState, Result};

/// Create PTY request
#[derive(Debug, Deserialize)]
pub struct CreatePtyRequest {
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub shell: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// PTY response
#[derive(Debug, Serialize)]
pub struct PtyResponse {
    pub id: String,
    pub cols: u16,
    pub rows: u16,
}

/// Resize PTY request
#[derive(Debug, Deserialize)]
pub struct ResizePtyRequest {
    pub cols: u16,
    pub rows: u16,
}

/// Create a new PTY
pub async fn create_pty(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
    Json(req): Json<CreatePtyRequest>,
) -> Result<Json<PtyResponse>> {
    let opts = PtyOptions {
        cols: req.cols,
        rows: req.rows,
        shell: req.shell,
        env: req.env,
    };

    let pty_info = state.pty_service.create(&sandbox_id, opts).await?;

    Ok(Json(PtyResponse {
        id: pty_info.id,
        cols: pty_info.cols,
        rows: pty_info.rows,
    }))
}

/// WebSocket handler for PTY
pub async fn pty_websocket(
    State(state): State<AppState>,
    Path((sandbox_id, pty_id)): Path<(String, String)>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_pty_socket(socket, state, sandbox_id, pty_id))
}

async fn handle_pty_socket(
    mut socket: WebSocket,
    state: AppState,
    sandbox_id: String,
    pty_id: String,
) {
    // Handle incoming messages
    while let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            match msg {
                Message::Text(text) => {
                    // Send text input to PTY
                    if let Err(e) = state.pty_service.send_input(&sandbox_id, &pty_id, text.as_bytes().to_vec()).await {
                        tracing::error!("Failed to send PTY input: {}", e);
                        break;
                    }
                }
                Message::Binary(data) => {
                    // Send binary input to PTY
                    if let Err(e) = state.pty_service.send_input(&sandbox_id, &pty_id, data.to_vec()).await {
                        tracing::error!("Failed to send PTY input: {}", e);
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        } else {
            break;
        }
    }
}

/// Resize a PTY
pub async fn resize_pty(
    State(state): State<AppState>,
    Path((sandbox_id, pty_id)): Path<(String, String)>,
    Json(req): Json<ResizePtyRequest>,
) -> Result<Json<serde_json::Value>> {
    state.pty_service.resize(&sandbox_id, &pty_id, req.cols, req.rows).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

/// Kill a PTY
pub async fn kill_pty(
    State(state): State<AppState>,
    Path((sandbox_id, pty_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    state.pty_service.kill(&sandbox_id, &pty_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}
