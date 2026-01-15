//! Agent connection pool

use std::collections::HashMap;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::timeout;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::error::{Error, Result};

/// Default timeout for agent responses
const DEFAULT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Agent command response
#[derive(Debug, Clone)]
pub struct AgentCommandResponse {
    pub correlation_id: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub error_message: Option<String>,
}

/// Agent message types
#[derive(Debug, Clone)]
pub enum AgentMessageType {
    RunCommand {
        correlation_id: String,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        cwd: Option<String>,
        timeout_ms: u64,
        stream: bool,
    },
    KillProcess {
        correlation_id: String,
        pid: u32,
        signal: i32,
    },
    CreatePty {
        correlation_id: String,
        pty_id: String,
        cols: u32,
        rows: u32,
        shell: Option<String>,
        env: HashMap<String, String>,
    },
    ResizePty {
        correlation_id: String,
        pty_id: String,
        cols: u32,
        rows: u32,
    },
    KillPty {
        correlation_id: String,
        pty_id: String,
    },
    PtyInput {
        pty_id: String,
        data: Vec<u8>,
    },
    HandshakeAck {
        success: bool,
        error: Option<String>,
    },
    HeartbeatAck {
        timestamp: u64,
    },
}

/// Agent connection pool for managing connections to sandbox agents
pub struct AgentConnPool {
    /// Map of sandbox_id -> agent connection
    connections: DashMap<String, AgentConnection>,
    /// Pending requests waiting for responses
    pending_requests: DashMap<String, oneshot::Sender<AgentCommandResponse>>,
    /// Default response timeout
    response_timeout: Duration,
}

/// Represents a connection to an agent
pub struct AgentConnection {
    pub tx: mpsc::Sender<AgentMessageType>,
    pub last_heartbeat: RwLock<std::time::Instant>,
}

impl AgentConnPool {
    /// Create a new agent connection pool
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            pending_requests: DashMap::new(),
            response_timeout: DEFAULT_RESPONSE_TIMEOUT,
        }
    }

    /// Create a new agent connection pool with custom timeout
    pub fn with_timeout(response_timeout: Duration) -> Self {
        Self {
            connections: DashMap::new(),
            pending_requests: DashMap::new(),
            response_timeout,
        }
    }

    /// Register an agent connection
    pub fn register(&self, sandbox_id: &str, tx: mpsc::Sender<AgentMessageType>) {
        let sandbox_id = sandbox_id.to_string();
        let now = std::time::Instant::now();
        self.connections.insert(
            sandbox_id.clone(),
            AgentConnection {
                tx,
                last_heartbeat: RwLock::new(now),
            },
        );
        info!("Agent registered for sandbox: {}", sandbox_id);
    }

    /// Unregister an agent connection
    pub fn unregister(&self, sandbox_id: &str) {
        if self.connections.remove(sandbox_id).is_some() {
            info!("Agent unregistered for sandbox: {}", sandbox_id);
        }
    }

    /// Check if an agent is connected
    pub fn is_connected(&self, sandbox_id: &str) -> bool {
        self.connections.contains_key(sandbox_id)
    }

    /// Get the number of connected agents
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// Check if the pool is empty
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Send a handshake acknowledgment
    pub async fn send_handshake_ack(&self, sandbox_id: &str, success: bool, error: Option<String>) -> Result<()> {
        let conn = self.connections.get(sandbox_id)
            .ok_or_else(|| Error::AgentNotConnected(sandbox_id.to_string()))?;

        let msg = AgentMessageType::HandshakeAck { success, error };

        conn.tx.send(msg).await
            .map_err(|_| Error::AgentCommunicationError("Failed to send message".to_string()))?;

        Ok(())
    }

    /// Send a heartbeat acknowledgment
    pub async fn send_heartbeat_ack(&self, sandbox_id: &str, timestamp: u64) -> Result<()> {
        let conn = self.connections.get(sandbox_id)
            .ok_or_else(|| Error::AgentNotConnected(sandbox_id.to_string()))?;

        // Update last heartbeat time
        *conn.last_heartbeat.write().await = std::time::Instant::now();

        let msg = AgentMessageType::HeartbeatAck { timestamp };

        conn.tx.send(msg).await
            .map_err(|_| Error::AgentCommunicationError("Failed to send message".to_string()))?;

        Ok(())
    }

    /// Run a command on an agent and wait for response
    pub async fn run_command(
        &self,
        sandbox_id: &str,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        cwd: Option<String>,
        timeout_ms: u64,
        stream: bool,
    ) -> Result<AgentCommandResponse> {
        let conn = self.connections.get(sandbox_id)
            .ok_or_else(|| Error::AgentNotConnected(sandbox_id.to_string()))?;

        let correlation_id = Uuid::new_v4().to_string();

        // Create oneshot channel for response
        let (response_tx, response_rx) = oneshot::channel();
        self.pending_requests.insert(correlation_id.clone(), response_tx);

        // Send command request
        let msg = AgentMessageType::RunCommand {
            correlation_id: correlation_id.clone(),
            command,
            args,
            env,
            cwd,
            timeout_ms,
            stream,
        };

        if conn.tx.send(msg).await.is_err() {
            self.pending_requests.remove(&correlation_id);
            return Err(Error::AgentCommunicationError("Failed to send command".to_string()));
        }

        drop(conn); // Release the reference before awaiting

        // Wait for response with timeout
        let response_timeout = if timeout_ms > 0 {
            Duration::from_millis(timeout_ms)
        } else {
            self.response_timeout
        };

        match timeout(response_timeout, response_rx).await {
            Ok(Ok(response)) => {
                self.pending_requests.remove(&correlation_id);
                Ok(response)
            }
            Ok(Err(_)) => {
                self.pending_requests.remove(&correlation_id);
                Err(Error::AgentCommunicationError("Response channel closed".to_string()))
            }
            Err(_) => {
                self.pending_requests.remove(&correlation_id);
                Err(Error::ProcessTimeout)
            }
        }
    }

    /// Kill a process on an agent
    pub async fn kill_process(&self, sandbox_id: &str, pid: u32, signal: i32) -> Result<()> {
        let conn = self.connections.get(sandbox_id)
            .ok_or_else(|| Error::AgentNotConnected(sandbox_id.to_string()))?;

        let correlation_id = Uuid::new_v4().to_string();

        let msg = AgentMessageType::KillProcess {
            correlation_id,
            pid,
            signal,
        };

        conn.tx.send(msg).await
            .map_err(|_| Error::AgentCommunicationError("Failed to send message".to_string()))?;

        Ok(())
    }

    /// Create a PTY on an agent
    pub async fn create_pty(
        &self,
        sandbox_id: &str,
        pty_id: String,
        cols: u32,
        rows: u32,
        shell: Option<String>,
        env: HashMap<String, String>,
    ) -> Result<()> {
        let conn = self.connections.get(sandbox_id)
            .ok_or_else(|| Error::AgentNotConnected(sandbox_id.to_string()))?;

        let correlation_id = Uuid::new_v4().to_string();

        let msg = AgentMessageType::CreatePty {
            correlation_id,
            pty_id,
            cols,
            rows,
            shell,
            env,
        };

        conn.tx.send(msg).await
            .map_err(|_| Error::AgentCommunicationError("Failed to send message".to_string()))?;

        Ok(())
    }

    /// Resize a PTY on an agent
    pub async fn resize_pty(&self, sandbox_id: &str, pty_id: &str, cols: u32, rows: u32) -> Result<()> {
        let conn = self.connections.get(sandbox_id)
            .ok_or_else(|| Error::AgentNotConnected(sandbox_id.to_string()))?;

        let correlation_id = Uuid::new_v4().to_string();

        let msg = AgentMessageType::ResizePty {
            correlation_id,
            pty_id: pty_id.to_string(),
            cols,
            rows,
        };

        conn.tx.send(msg).await
            .map_err(|_| Error::AgentCommunicationError("Failed to send message".to_string()))?;

        Ok(())
    }

    /// Kill a PTY on an agent
    pub async fn kill_pty(&self, sandbox_id: &str, pty_id: &str) -> Result<()> {
        let conn = self.connections.get(sandbox_id)
            .ok_or_else(|| Error::AgentNotConnected(sandbox_id.to_string()))?;

        let correlation_id = Uuid::new_v4().to_string();

        let msg = AgentMessageType::KillPty {
            correlation_id,
            pty_id: pty_id.to_string(),
        };

        conn.tx.send(msg).await
            .map_err(|_| Error::AgentCommunicationError("Failed to send message".to_string()))?;

        Ok(())
    }

    /// Send PTY input to an agent
    pub async fn send_pty_input(&self, sandbox_id: &str, pty_id: &str, data: Vec<u8>) -> Result<()> {
        let conn = self.connections.get(sandbox_id)
            .ok_or_else(|| Error::AgentNotConnected(sandbox_id.to_string()))?;

        let msg = AgentMessageType::PtyInput {
            pty_id: pty_id.to_string(),
            data,
        };

        conn.tx.send(msg).await
            .map_err(|_| Error::AgentCommunicationError("Failed to send message".to_string()))?;

        Ok(())
    }

    /// Handle a response from an agent (proto version)
    pub fn handle_response(&self, _sandbox_id: &str, response: crate::proto::AgentCommandResponse) {
        use crate::proto::agent_command_response;

        let correlation_id = &response.correlation_id;
        if let Some((_, sender)) = self.pending_requests.remove(correlation_id) {
            let converted = match response.result {
                Some(agent_command_response::Result::Success(s)) => AgentCommandResponse {
                    correlation_id: correlation_id.clone(),
                    success: true,
                    exit_code: Some(s.exit_code),
                    stdout: Some(s.stdout),
                    stderr: Some(s.stderr),
                    error_message: None,
                },
                Some(agent_command_response::Result::Error(e)) => AgentCommandResponse {
                    correlation_id: correlation_id.clone(),
                    success: false,
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    error_message: Some(e.message),
                },
                None => AgentCommandResponse {
                    correlation_id: correlation_id.clone(),
                    success: false,
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    error_message: Some("No result in response".to_string()),
                },
            };
            let _ = sender.send(converted);
        } else {
            warn!("Received response for unknown correlation_id: {}", correlation_id);
        }
    }

    /// Handle PTY output from an agent
    pub fn handle_pty_output(&self, sandbox_id: &str, pty_id: &str, data: Vec<u8>) {
        // TODO: Forward PTY output to WebSocket connections
        debug!("PTY output for {}:{}: {} bytes", sandbox_id, pty_id, data.len());
    }

    /// Handle a response from an agent (internal version)
    pub fn handle_internal_response(&self, response: AgentCommandResponse) {
        let correlation_id = &response.correlation_id;
        if let Some((_, sender)) = self.pending_requests.remove(correlation_id) {
            let _ = sender.send(response);
        } else {
            warn!("Received response for unknown correlation_id: {}", correlation_id);
        }
    }

    /// Wait for an agent to connect
    pub async fn wait_for_connection(&self, sandbox_id: &str, timeout_duration: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        let check_interval = Duration::from_millis(100);

        while start.elapsed() < timeout_duration {
            if self.is_connected(sandbox_id) {
                return Ok(());
            }
            tokio::time::sleep(check_interval).await;
        }

        Err(Error::AgentConnectionTimeout)
    }

    /// Get all connected sandbox IDs
    pub fn get_connected_sandboxes(&self) -> Vec<String> {
        self.connections.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Check for stale connections
    pub async fn check_stale_connections(&self, max_idle: Duration) -> Vec<String> {
        let mut stale = Vec::new();
        let now = std::time::Instant::now();

        for entry in self.connections.iter() {
            let last_heartbeat = *entry.last_heartbeat.read().await;
            if now.duration_since(last_heartbeat) > max_idle {
                stale.push(entry.key().clone());
            }
        }

        stale
    }
}

impl Default for AgentConnPool {
    fn default() -> Self {
        Self::new()
    }
}
