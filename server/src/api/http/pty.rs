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
    use crate::infra::agent_pool::PtyOutputEvent;

    // Subscribe to PTY output from the agent
    let mut pty_rx = state.agent_pool.subscribe_pty(&sandbox_id, &pty_id);

    // Bidirectional loop: forward input from WebSocket → PTY, output from PTY → WebSocket
    loop {
        tokio::select! {
            // Read from WebSocket (client input)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = state
                            .pty_service
                            .send_input(&sandbox_id, &pty_id, text.as_bytes().to_vec())
                            .await
                        {
                            tracing::error!("Failed to send PTY input: {}", e);
                            break;
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        if let Err(e) = state
                            .pty_service
                            .send_input(&sandbox_id, &pty_id, data.to_vec())
                            .await
                        {
                            tracing::error!("Failed to send PTY input: {}", e);
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::debug!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            // Read from PTY output (agent → client)
            event = pty_rx.recv() => {
                match event {
                    Some(PtyOutputEvent::Output(data)) => {
                        if socket.send(Message::Binary(data.into())).await.is_err() {
                            break;
                        }
                    }
                    Some(PtyOutputEvent::Exit(code)) => {
                        let msg = format!("\r\n[Process exited with code {}]\r\n", code);
                        let _ = socket.send(Message::Text(msg.into())).await;
                        break;
                    }
                    Some(PtyOutputEvent::Error(err)) => {
                        let msg = format!("\r\n[PTY error: {}]\r\n", err);
                        let _ = socket.send(Message::Text(msg.into())).await;
                        break;
                    }
                    None => break,
                }
            }
        }
    }

    // Cleanup: unsubscribe from PTY output
    state.agent_pool.unsubscribe_pty(&sandbox_id, &pty_id);
}

/// Resize a PTY
pub async fn resize_pty(
    State(state): State<AppState>,
    Path((sandbox_id, pty_id)): Path<(String, String)>,
    Json(req): Json<ResizePtyRequest>,
) -> Result<Json<serde_json::Value>> {
    state
        .pty_service
        .resize(&sandbox_id, &pty_id, req.cols, req.rows)
        .await?;
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
