//! Server connection management

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, sleep, timeout};
use tonic::transport::Channel;
use tracing::{debug, error, info, warn};

use crate::handlers::{process, pty::PtyManager};

#[allow(clippy::all)]
#[allow(unused_imports)]
mod proto {
    include!("proto/workspace.v1.rs");
}

use proto::agent_service_client::AgentServiceClient;
use proto::{
    agent_command_response, agent_message, server_message, AgentCommandError, AgentCommandResponse,
    AgentCommandSuccess, AgentHandshake, AgentHeartbeat, AgentMessage,
};

const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Connection manager for communicating with the server
pub struct ConnectionManager {
    server_addr: String,
    sandbox_id: String,
    pty_manager: Arc<PtyManager>,
    connected: Arc<RwLock<bool>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new(server_addr: String, sandbox_id: String) -> Self {
        Self {
            server_addr,
            sandbox_id,
            pty_manager: Arc::new(PtyManager::new(16)),
            connected: Arc::new(RwLock::new(false)),
            shutdown_tx: None,
        }
    }

    /// Run the connection manager (blocking until shutdown)
    pub async fn run(&mut self) -> anyhow::Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        let mut reconnect_delay = RECONNECT_INITIAL_DELAY;

        loop {
            tokio::select! {
                result = self.connect_and_run() => {
                    match result {
                        Ok(_) => {
                            info!("Connection closed normally");
                            break;
                        }
                        Err(e) => {
                            error!("Connection error: {}", e);
                            *self.connected.write().await = false;

                            // Wait before reconnecting
                            info!("Reconnecting in {:?}...", reconnect_delay);
                            sleep(reconnect_delay).await;

                            // Exponential backoff
                            reconnect_delay = std::cmp::min(
                                reconnect_delay * 2,
                                RECONNECT_MAX_DELAY,
                            );
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Connect to server and run message loop
    async fn connect_and_run(&self) -> anyhow::Result<()> {
        info!("Connecting to server: {}", self.server_addr);

        // Create channel with timeout
        let channel = timeout(
            CONNECT_TIMEOUT,
            Channel::from_shared(self.server_addr.clone())?.connect(),
        )
        .await??;

        let mut client = AgentServiceClient::new(channel);

        // Create channels for sending messages
        let (tx, rx) = mpsc::channel::<AgentMessage>(100);
        let rx_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

        // Start bidirectional stream
        let response = client.connect(rx_stream).await?;
        let mut inbound = response.into_inner();

        // Send handshake
        info!("Sending handshake for sandbox: {}", self.sandbox_id);
        tx.send(AgentMessage {
            message: Some(agent_message::Message::Handshake(AgentHandshake {
                sandbox_id: self.sandbox_id.clone(),
                version: AGENT_VERSION.to_string(),
            })),
        })
        .await?;

        // Wait for handshake acknowledgment
        let handshake_result = timeout(Duration::from_secs(10), inbound.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("No handshake response"))??;

        match handshake_result.message {
            Some(server_message::Message::HandshakeAck(ack)) => {
                if !ack.success {
                    anyhow::bail!(
                        "Handshake failed: {}",
                        ack.error.unwrap_or_else(|| "Unknown error".to_string())
                    );
                }
                info!("Handshake successful");
            }
            _ => anyhow::bail!("Unexpected response to handshake"),
        }

        *self.connected.write().await = true;

        // Spawn heartbeat task
        let tx_heartbeat = tx.clone();
        let heartbeat_handle = tokio::spawn(async move {
            let mut interval = interval(HEARTBEAT_INTERVAL);
            loop {
                interval.tick().await;
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                if tx_heartbeat
                    .send(AgentMessage {
                        message: Some(agent_message::Message::Heartbeat(AgentHeartbeat {
                            timestamp,
                        })),
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Process incoming messages
        let tx_response = tx.clone();
        let pty_manager = self.pty_manager.clone();

        while let Some(msg_result) = inbound.next().await {
            let msg = msg_result?;

            match msg.message {
                Some(server_message::Message::RunCommand(req)) => {
                    debug!("Received run command: {}", req.correlation_id);
                    let tx = tx_response.clone();
                    let correlation_id = req.correlation_id.clone();

                    tokio::spawn(async move {
                        let result =
                            Self::handle_run_command(req.command, req.args, req.env, req.cwd).await;

                        let response = match result {
                            Ok((exit_code, stdout, stderr)) => AgentCommandResponse {
                                correlation_id,
                                result: Some(agent_command_response::Result::Success(
                                    AgentCommandSuccess {
                                        exit_code,
                                        stdout,
                                        stderr,
                                    },
                                )),
                            },
                            Err(e) => AgentCommandResponse {
                                correlation_id,
                                result: Some(agent_command_response::Result::Error(
                                    AgentCommandError {
                                        code: 1,
                                        message: e.to_string(),
                                    },
                                )),
                            },
                        };

                        let _ = tx
                            .send(AgentMessage {
                                message: Some(agent_message::Message::CommandResponse(response)),
                            })
                            .await;
                    });
                }

                Some(server_message::Message::KillProcess(req)) => {
                    debug!("Received kill process: pid={}", req.pid);
                    let tx = tx_response.clone();
                    let correlation_id = req.correlation_id.clone();

                    let result = process::kill_process(req.pid, req.signal);

                    let response = match result {
                        Ok(_) => AgentCommandResponse {
                            correlation_id,
                            result: Some(agent_command_response::Result::Success(
                                AgentCommandSuccess {
                                    exit_code: 0,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                },
                            )),
                        },
                        Err(e) => AgentCommandResponse {
                            correlation_id,
                            result: Some(agent_command_response::Result::Error(
                                AgentCommandError {
                                    code: 1,
                                    message: e.to_string(),
                                },
                            )),
                        },
                    };

                    let _ = tx
                        .send(AgentMessage {
                            message: Some(agent_message::Message::CommandResponse(response)),
                        })
                        .await;
                }

                Some(server_message::Message::CreatePty(req)) => {
                    debug!("Received create PTY: {}", req.pty_id);
                    let tx = tx_response.clone();
                    let correlation_id = req.correlation_id.clone();
                    let pty_manager = pty_manager.clone();

                    let result = pty_manager
                        .create(
                            req.pty_id,
                            req.cols as u16,
                            req.rows as u16,
                            req.shell.as_deref(),
                            &req.env,
                        )
                        .await;

                    let response = match result {
                        Ok(_) => AgentCommandResponse {
                            correlation_id,
                            result: Some(agent_command_response::Result::Success(
                                AgentCommandSuccess {
                                    exit_code: 0,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                },
                            )),
                        },
                        Err(e) => AgentCommandResponse {
                            correlation_id,
                            result: Some(agent_command_response::Result::Error(
                                AgentCommandError {
                                    code: 1,
                                    message: e.to_string(),
                                },
                            )),
                        },
                    };

                    let _ = tx
                        .send(AgentMessage {
                            message: Some(agent_message::Message::CommandResponse(response)),
                        })
                        .await;
                }

                Some(server_message::Message::ResizePty(req)) => {
                    debug!("Received resize PTY: {}", req.pty_id);
                    let tx = tx_response.clone();
                    let correlation_id = req.correlation_id.clone();
                    let pty_manager = pty_manager.clone();

                    let result = pty_manager
                        .resize(&req.pty_id, req.cols as u16, req.rows as u16)
                        .await;

                    let response = match result {
                        Ok(_) => AgentCommandResponse {
                            correlation_id,
                            result: Some(agent_command_response::Result::Success(
                                AgentCommandSuccess {
                                    exit_code: 0,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                },
                            )),
                        },
                        Err(e) => AgentCommandResponse {
                            correlation_id,
                            result: Some(agent_command_response::Result::Error(
                                AgentCommandError {
                                    code: 1,
                                    message: e.to_string(),
                                },
                            )),
                        },
                    };

                    let _ = tx
                        .send(AgentMessage {
                            message: Some(agent_message::Message::CommandResponse(response)),
                        })
                        .await;
                }

                Some(server_message::Message::KillPty(req)) => {
                    debug!("Received kill PTY: {}", req.pty_id);
                    let tx = tx_response.clone();
                    let correlation_id = req.correlation_id.clone();
                    let pty_manager = pty_manager.clone();

                    let result = pty_manager.kill(&req.pty_id).await;

                    let response = match result {
                        Ok(_) => AgentCommandResponse {
                            correlation_id,
                            result: Some(agent_command_response::Result::Success(
                                AgentCommandSuccess {
                                    exit_code: 0,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                },
                            )),
                        },
                        Err(e) => AgentCommandResponse {
                            correlation_id,
                            result: Some(agent_command_response::Result::Error(
                                AgentCommandError {
                                    code: 1,
                                    message: e.to_string(),
                                },
                            )),
                        },
                    };

                    let _ = tx
                        .send(AgentMessage {
                            message: Some(agent_message::Message::CommandResponse(response)),
                        })
                        .await;
                }

                Some(server_message::Message::PtyInput(req)) => {
                    debug!("Received PTY input: {} bytes", req.data.len());
                    let pty_manager = pty_manager.clone();

                    let _ = pty_manager.write(&req.pty_id, &req.data).await;
                }

                Some(server_message::Message::HeartbeatAck(_)) => {
                    debug!("Received heartbeat ack");
                }

                Some(server_message::Message::HandshakeAck(_)) => {
                    warn!("Unexpected handshake ack after initial handshake");
                }

                None => {
                    warn!("Received empty message");
                }
            }
        }

        // Clean up
        heartbeat_handle.abort();
        *self.connected.write().await = false;

        Ok(())
    }

    /// Handle run command request
    async fn handle_run_command(
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        cwd: Option<String>,
    ) -> anyhow::Result<(i32, String, String)> {
        let (tx, mut rx) = mpsc::channel(100);

        let cmd = command.clone();
        let args_clone = args.clone();
        let env_clone = env.clone();
        let cwd_clone = cwd.clone();

        tokio::spawn(async move {
            let _ =
                process::run_command(&cmd, &args_clone, &env_clone, cwd_clone.as_deref(), tx).await;
        });

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        while let Some(event) = rx.recv().await {
            match event {
                process::ProcessOutput::Stdout(line) => {
                    stdout.push_str(&line);
                    stdout.push('\n');
                }
                process::ProcessOutput::Stderr(line) => {
                    stderr.push_str(&line);
                    stderr.push('\n');
                }
                process::ProcessOutput::Exit(code) => {
                    exit_code = code;
                }
            }
        }

        Ok((exit_code, stdout, stderr))
    }
}
