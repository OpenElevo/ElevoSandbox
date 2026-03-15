//! Process HTTP handlers

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;

use crate::service::process::RunCommandOptions;
use crate::{AppState, Result};

/// Run command request
#[derive(Debug, Deserialize)]
pub struct RunCommandRequest {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub cwd: Option<String>,
    pub timeout: Option<u64>,
}

/// Command result response
#[derive(Debug, Serialize)]
pub struct CommandResultResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Process event for SSE streaming
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ProcessEventResponse {
    Stdout { data: String },
    Stderr { data: String },
    Exit { code: i32 },
    Error { message: String },
}

/// Kill process request
#[derive(Debug, Deserialize)]
pub struct KillProcessRequest {
    pub signal: Option<i32>,
}

/// Run a command in a sandbox
pub async fn run_command(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
    Json(req): Json<RunCommandRequest>,
) -> Result<Json<CommandResultResponse>> {
    let opts = RunCommandOptions {
        command: req.command,
        args: req.args.unwrap_or_default(),
        env: req.env.unwrap_or_default(),
        cwd: req.cwd,
        timeout_ms: req.timeout.unwrap_or(0),
    };

    let result = state.process_service.run(&sandbox_id, opts).await?;

    Ok(Json(CommandResultResponse {
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
    }))
}

/// Run a command with streaming output (SSE)
pub async fn run_command_stream(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<StreamCommandQuery>,
) -> impl axum::response::IntoResponse {
    let command = match params.command {
        Some(cmd) => cmd,
        None => {
            return Err((
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(
                    serde_json::json!({"error": {"code": "BAD_REQUEST", "message": "command query parameter is required"}}),
                ),
            ));
        }
    };

    let opts = RunCommandOptions {
        command,
        args: params.args.unwrap_or_default(),
        env: Default::default(),
        cwd: params.cwd,
        timeout_ms: params.timeout.unwrap_or(0),
    };

    let event_stream = state
        .process_service
        .run_stream(&sandbox_id, opts)
        .await
        .map_err(|e| {
            let status = e.status_code();
            let body = serde_json::json!({"error": {"code": e.error_code_str(), "message": e.to_string()}});
            (status, axum::Json(body))
        })?;

    use futures::StreamExt;
    let sse_stream = event_stream.map(|event| {
        let (event_type, data) = match event {
            crate::domain::types::ProcessEvent::Stdout { data } => (
                "stdout",
                serde_json::to_string(&ProcessEventResponse::Stdout { data }).unwrap(),
            ),
            crate::domain::types::ProcessEvent::Stderr { data } => (
                "stderr",
                serde_json::to_string(&ProcessEventResponse::Stderr { data }).unwrap(),
            ),
            crate::domain::types::ProcessEvent::Exit { code } => (
                "exit",
                serde_json::to_string(&ProcessEventResponse::Exit { code }).unwrap(),
            ),
            crate::domain::types::ProcessEvent::Error { message } => (
                "error",
                serde_json::to_string(&ProcessEventResponse::Error { message }).unwrap(),
            ),
        };
        Ok::<_, Infallible>(Event::default().event(event_type).data(data))
    });

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}

/// Query parameters for streaming command execution
#[derive(Debug, Deserialize)]
pub struct StreamCommandQuery {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub timeout: Option<u64>,
}

/// Kill a process
pub async fn kill_process(
    State(state): State<AppState>,
    Path((sandbox_id, pid)): Path<(String, String)>,
    Json(req): Json<KillProcessRequest>,
) -> Result<Json<serde_json::Value>> {
    let pid: u32 = pid
        .parse()
        .map_err(|_| crate::Error::InvalidParameter("pid must be a number".to_string()))?;
    let signal = req.signal.unwrap_or(15);

    state
        .process_service
        .kill(&sandbox_id, pid, Some(signal))
        .await?;
    Ok(Json(serde_json::json!({ "success": true })))
}
