//! PTY service

use std::sync::Arc;

use tracing::info;
use uuid::Uuid;

use crate::domain::sandbox::SandboxState;
use crate::domain::types::{PtyInfo, PtyOptions};
use crate::infra::agent_pool::AgentConnPool;
use crate::infra::sqlite::SandboxRepository;
use crate::error::{Error, Result};

/// PTY service for managing interactive terminals
pub struct PtyService {
    agent_pool: Arc<AgentConnPool>,
    repository: Arc<SandboxRepository>,
}

impl PtyService {
    /// Create a new PTY service
    pub fn new(agent_pool: Arc<AgentConnPool>, repository: Arc<SandboxRepository>) -> Self {
        Self { agent_pool, repository }
    }

    /// Create a new PTY
    pub async fn create(&self, sandbox_id: &str, opts: PtyOptions) -> Result<PtyInfo> {
        // Validate sandbox exists and is running
        let sandbox = self.repository.get(sandbox_id).await?;
        if sandbox.state != SandboxState::Running {
            return Err(Error::InvalidSandboxState {
                expected: "running".to_string(),
                actual: sandbox.state.as_str().to_string(),
            });
        }

        // Check agent connection
        if !self.agent_pool.is_connected(sandbox_id) {
            return Err(Error::AgentNotConnected(sandbox_id.to_string()));
        }

        let pty_id = Uuid::new_v4().to_string();
        let cols = opts.cols.unwrap_or(80) as u32;
        let rows = opts.rows.unwrap_or(24) as u32;

        info!(
            "Creating PTY {} in sandbox {} ({}x{})",
            pty_id, sandbox_id, cols, rows
        );

        // Send create PTY request to agent
        self.agent_pool
            .create_pty(
                sandbox_id,
                pty_id.clone(),
                cols,
                rows,
                opts.shell,
                opts.env.unwrap_or_default(),
            )
            .await?;

        Ok(PtyInfo {
            id: pty_id,
            sandbox_id: sandbox_id.to_string(),
            cols: cols as u16,
            rows: rows as u16,
        })
    }

    /// Resize a PTY
    pub async fn resize(&self, sandbox_id: &str, pty_id: &str, cols: u16, rows: u16) -> Result<()> {
        // Validate sandbox exists and is running
        let sandbox = self.repository.get(sandbox_id).await?;
        if sandbox.state != SandboxState::Running {
            return Err(Error::InvalidSandboxState {
                expected: "running".to_string(),
                actual: sandbox.state.as_str().to_string(),
            });
        }

        // Check agent connection
        if !self.agent_pool.is_connected(sandbox_id) {
            return Err(Error::AgentNotConnected(sandbox_id.to_string()));
        }

        info!(
            "Resizing PTY {} in sandbox {} to {}x{}",
            pty_id, sandbox_id, cols, rows
        );

        self.agent_pool
            .resize_pty(sandbox_id, pty_id, cols as u32, rows as u32)
            .await
    }

    /// Kill a PTY
    pub async fn kill(&self, sandbox_id: &str, pty_id: &str) -> Result<()> {
        // Validate sandbox exists and is running
        let sandbox = self.repository.get(sandbox_id).await?;
        if sandbox.state != SandboxState::Running {
            return Err(Error::InvalidSandboxState {
                expected: "running".to_string(),
                actual: sandbox.state.as_str().to_string(),
            });
        }

        // Check agent connection
        if !self.agent_pool.is_connected(sandbox_id) {
            return Err(Error::AgentNotConnected(sandbox_id.to_string()));
        }

        info!("Killing PTY {} in sandbox {}", pty_id, sandbox_id);

        self.agent_pool.kill_pty(sandbox_id, pty_id).await
    }

    /// Send input to a PTY
    pub async fn send_input(&self, sandbox_id: &str, pty_id: &str, data: Vec<u8>) -> Result<()> {
        // Check agent connection (skip sandbox validation for performance)
        if !self.agent_pool.is_connected(sandbox_id) {
            return Err(Error::AgentNotConnected(sandbox_id.to_string()));
        }

        self.agent_pool.send_pty_input(sandbox_id, pty_id, data).await
    }
}
