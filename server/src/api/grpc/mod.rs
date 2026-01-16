//! gRPC API handlers for Agent communication

use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

use crate::infra::agent_pool::{AgentConnPool, AgentMessageType};
use crate::proto::{
    agent_message,
    agent_service_server::{AgentService, AgentServiceServer},
    server_message, AgentMessage, ServerHandshakeAck, ServerHeartbeatAck, ServerMessage,
};

/// gRPC service implementation for Agent connections
pub struct AgentServiceImpl {
    agent_pool: Arc<AgentConnPool>,
}

impl AgentServiceImpl {
    pub fn new(agent_pool: Arc<AgentConnPool>) -> Self {
        Self { agent_pool }
    }
}

#[tonic::async_trait]
impl AgentService for AgentServiceImpl {
    type ConnectStream = Pin<Box<dyn Stream<Item = Result<ServerMessage, Status>> + Send>>;

    async fn connect(
        &self,
        request: Request<Streaming<AgentMessage>>,
    ) -> Result<Response<Self::ConnectStream>, Status> {
        let mut inbound = request.into_inner();
        let agent_pool = self.agent_pool.clone();

        // Create channel for sending messages to agent
        let (tx, rx) = mpsc::channel::<Result<ServerMessage, Status>>(100);

        // Spawn task to handle the connection asynchronously
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            // Wait for handshake message first
            let first_msg = inbound.next().await;
            let sandbox_id = match first_msg {
                Some(Ok(msg)) => {
                    match msg.message {
                        Some(agent_message::Message::Handshake(handshake)) => {
                            info!(
                                "Agent handshake received for sandbox: {}",
                                handshake.sandbox_id
                            );

                            // Register agent connection
                            let (msg_tx, mut msg_rx) = mpsc::channel::<AgentMessageType>(100);
                            agent_pool.register(&handshake.sandbox_id, msg_tx);

                            // Send handshake acknowledgment
                            let _ = tx_clone
                                .send(Ok(ServerMessage {
                                    message: Some(server_message::Message::HandshakeAck(
                                        ServerHandshakeAck {
                                            success: true,
                                            error: None,
                                        },
                                    )),
                                }))
                                .await;

                            // Spawn task to forward messages from pool to agent
                            let tx_forward = tx_clone.clone();
                            let sandbox_id_clone = handshake.sandbox_id.clone();
                            tokio::spawn(async move {
                                while let Some(msg) = msg_rx.recv().await {
                                    let server_msg = convert_pool_message_to_proto(msg);
                                    if tx_forward.send(Ok(server_msg)).await.is_err() {
                                        break;
                                    }
                                }
                                debug!("Message forwarding task ended for {}", sandbox_id_clone);
                            });

                            handshake.sandbox_id
                        }
                        _ => {
                            let _ = tx_clone
                                .send(Ok(ServerMessage {
                                    message: Some(server_message::Message::HandshakeAck(
                                        ServerHandshakeAck {
                                            success: false,
                                            error: Some("Expected handshake message".to_string()),
                                        },
                                    )),
                                }))
                                .await;
                            error!("Invalid first message - expected handshake");
                            return;
                        }
                    }
                }
                Some(Err(e)) => {
                    error!("Error receiving handshake: {}", e);
                    return;
                }
                None => {
                    error!("No handshake message received");
                    return;
                }
            };

            // Handle incoming messages from agent
            while let Some(result) = inbound.next().await {
                match result {
                    Ok(msg) => match msg.message {
                        Some(agent_message::Message::Heartbeat(hb)) => {
                            debug!("Heartbeat from {}: {}", sandbox_id, hb.timestamp);
                            let _ = tx_clone
                                .send(Ok(ServerMessage {
                                    message: Some(server_message::Message::HeartbeatAck(
                                        ServerHeartbeatAck {
                                            timestamp: hb.timestamp,
                                        },
                                    )),
                                }))
                                .await;
                        }
                        Some(agent_message::Message::CommandResponse(resp)) => {
                            debug!(
                                "Command response from {}: {}",
                                sandbox_id, resp.correlation_id
                            );
                            agent_pool.handle_response(&sandbox_id, resp);
                        }
                        Some(agent_message::Message::PtyOutput(output)) => {
                            debug!(
                                "PTY output from {}: {} bytes",
                                sandbox_id,
                                output.data.len()
                            );
                            agent_pool.handle_pty_output(&sandbox_id, &output.pty_id, output.data);
                        }
                        _ => {
                            warn!("Unexpected message type from {}", sandbox_id);
                        }
                    },
                    Err(e) => {
                        error!("Error receiving message from {}: {}", sandbox_id, e);
                        break;
                    }
                }
            }

            // Agent disconnected
            info!("Agent disconnected: {}", sandbox_id);
            agent_pool.unregister(&sandbox_id);
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }
}

/// Convert internal message type to proto message
fn convert_pool_message_to_proto(msg: AgentMessageType) -> ServerMessage {
    use crate::proto::{
        AgentCreatePtyRequest, AgentKillProcessRequest, AgentKillPtyRequest, AgentPtyInput,
        AgentResizePtyRequest, AgentRunCommandRequest,
    };

    let message = match msg {
        AgentMessageType::RunCommand {
            correlation_id,
            command,
            args,
            env,
            cwd,
            timeout_ms,
            stream,
        } => server_message::Message::RunCommand(AgentRunCommandRequest {
            correlation_id,
            command,
            args,
            env,
            cwd,
            timeout_ms,
            stream,
        }),
        AgentMessageType::KillProcess {
            correlation_id,
            pid,
            signal,
        } => server_message::Message::KillProcess(AgentKillProcessRequest {
            correlation_id,
            pid,
            signal,
        }),
        AgentMessageType::CreatePty {
            correlation_id,
            pty_id,
            cols,
            rows,
            shell,
            env,
        } => server_message::Message::CreatePty(AgentCreatePtyRequest {
            correlation_id,
            pty_id,
            cols,
            rows,
            shell,
            env,
        }),
        AgentMessageType::ResizePty {
            correlation_id,
            pty_id,
            cols,
            rows,
        } => server_message::Message::ResizePty(AgentResizePtyRequest {
            correlation_id,
            pty_id,
            cols,
            rows,
        }),
        AgentMessageType::KillPty {
            correlation_id,
            pty_id,
        } => server_message::Message::KillPty(AgentKillPtyRequest {
            correlation_id,
            pty_id,
        }),
        AgentMessageType::PtyInput { pty_id, data } => {
            server_message::Message::PtyInput(AgentPtyInput { pty_id, data })
        }
        AgentMessageType::HandshakeAck { success, error } => {
            server_message::Message::HandshakeAck(ServerHandshakeAck { success, error })
        }
        AgentMessageType::HeartbeatAck { timestamp } => {
            server_message::Message::HeartbeatAck(ServerHeartbeatAck { timestamp })
        }
    };

    ServerMessage {
        message: Some(message),
    }
}

/// Create gRPC server
pub fn create_server(agent_pool: Arc<AgentConnPool>) -> AgentServiceServer<AgentServiceImpl> {
    AgentServiceServer::new(AgentServiceImpl::new(agent_pool))
}
