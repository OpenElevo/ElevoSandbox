//! Common domain types

use serde::{Deserialize, Serialize};

/// Command result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Process event types for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessEvent {
    Stdout { data: String },
    Stderr { data: String },
    Exit { code: i32 },
    Error { message: String },
}

/// PTY options
#[derive(Debug, Clone, Deserialize)]
pub struct PtyOptions {
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub shell: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

impl Default for PtyOptions {
    fn default() -> Self {
        Self {
            cols: Some(80),
            rows: Some(24),
            shell: None,
            env: None,
        }
    }
}

/// PTY info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyInfo {
    pub id: String,
    pub sandbox_id: String,
    pub cols: u16,
    pub rows: u16,
}
