//! gRPC ClientStorageService implementation
//!
//! Handles Client storage provider connections. Clients connect via a bidirectional
//! stream, allowing the Server to send file operation requests and receive responses.

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::infra::fuse::mount::FuseMountManager;
use crate::infra::storage::remote::RemoteStoragePool;
use crate::infra::storage::router::StorageRouter;
use crate::infra::storage::StorageBackend;
use crate::infra::workspace_repository::WorkspaceRepository;
use crate::proto::{
    client_message, client_storage_service_server::ClientStorageService,
    read_file_stream_request, server_storage_message, write_file_stream_response, ClientMessage,
    ReadFileStreamRequest, ReadFileStreamResponse, ServerStorageMessage, StorageHandshakeAck,
    StoragePing, WriteFileStreamDone, WriteFileStreamRequest, WriteFileStreamResponse,
};

/// gRPC ClientStorageService implementation
pub struct ClientStorageServiceImpl {
    pool: Arc<RemoteStoragePool>,
    storage_router: Arc<StorageRouter>,
    workspace_repository: Arc<WorkspaceRepository>,
    config: Arc<Config>,
    fuse_manager: Arc<FuseMountManager>,
}

impl ClientStorageServiceImpl {
    pub fn new(
        pool: Arc<RemoteStoragePool>,
        storage_router: Arc<StorageRouter>,
        workspace_repository: Arc<WorkspaceRepository>,
        config: Arc<Config>,
        fuse_manager: Arc<FuseMountManager>,
    ) -> Self {
        Self {
            pool,
            storage_router,
            workspace_repository,
            config,
            fuse_manager,
        }
    }
}

/// Send a handshake failure message and return.
async fn send_handshake_error(tx: &mpsc::Sender<ServerStorageMessage>, error_msg: String) {
    let _ = tx
        .send(ServerStorageMessage {
            message: Some(server_storage_message::Message::HandshakeAck(
                StorageHandshakeAck {
                    success: false,
                    error: Some(error_msg),
                },
            )),
        })
        .await;
}

/// Verify the Client token against the configured fs_api_token.
/// Returns true if authentication passes.
///
/// Reuses the same token as FileSystemService (fs_api_token). When no token
/// is configured, all connections are accepted (open mode).
fn verify_token(config: &Config, token: &str) -> bool {
    match &config.fs_api_token {
        Some(expected_token) => {
            // Fixed-length comparison to reduce timing side-channel risk.
            // Network latency over gRPC dominates, making timing attacks impractical,
            // but we still XOR all bytes to avoid early-exit on first mismatch.
            let a = expected_token.as_bytes();
            let b = token.as_bytes();
            if a.len() != b.len() {
                return false;
            }
            let mut diff: u8 = 0;
            for (x, y) in a.iter().zip(b.iter()) {
                diff |= x ^ y;
            }
            diff == 0
        }
        None => {
            // No token configured — open mode (same as FileSystemService behavior)
            true
        }
    }
}

#[tonic::async_trait]
impl ClientStorageService for ClientStorageServiceImpl {
    type ConnectStream =
        Pin<Box<dyn Stream<Item = Result<ServerStorageMessage, Status>> + Send>>;

    async fn connect(
        &self,
        request: Request<Streaming<ClientMessage>>,
    ) -> Result<Response<Self::ConnectStream>, Status> {
        let mut inbound = request.into_inner();
        let pool = self.pool.clone();
        let storage_router = self.storage_router.clone();
        let workspace_repository = self.workspace_repository.clone();
        let config = self.config.clone();
        let fuse_manager = self.fuse_manager.clone();

        // Channel for sending messages to Client
        let (tx, rx) = mpsc::channel::<Result<ServerStorageMessage, Status>>(256);
        // Channel for outbound messages (typed, not Result-wrapped)
        let (out_tx, mut out_rx) = mpsc::channel::<ServerStorageMessage>(256);

        // Forward outbound messages into the response stream
        let tx_fwd = tx.clone();
        tokio::spawn(async move {
            while let Some(msg) = out_rx.recv().await {
                if tx_fwd.send(Ok(msg)).await.is_err() {
                    break;
                }
            }
        });

        let out_tx_clone = out_tx.clone();
        tokio::spawn(async move {
            // ── Step 1: Wait for handshake ──
            let first_msg = inbound.next().await;
            let (workspace_id, token) = match first_msg {
                Some(Ok(msg)) => match msg.message {
                    Some(client_message::Message::Handshake(hs)) => {
                        info!(
                            workspace_id = %hs.workspace_id,
                            "Client storage handshake received"
                        );
                        (hs.workspace_id, hs.token)
                    }
                    _ => {
                        send_handshake_error(
                            &out_tx_clone,
                            "expected handshake message".to_string(),
                        )
                        .await;
                        error!("Client storage: invalid first message");
                        return;
                    }
                },
                Some(Err(e)) => {
                    error!("Client storage handshake error: {}", e);
                    return;
                }
                None => {
                    error!("Client storage: stream closed before handshake");
                    return;
                }
            };

            // ── Step 2: Authenticate token ──
            if !verify_token(&config, &token) {
                send_handshake_error(
                    &out_tx_clone,
                    "authentication failed: invalid token".to_string(),
                )
                .await;
                warn!(workspace_id = %workspace_id, "Client storage authentication failed");
                return;
            }

            // ── Step 3: Verify workspace exists and is remote ──
            let workspace = match workspace_repository.get(&workspace_id).await {
                Ok(ws) => ws,
                Err(_) => {
                    send_handshake_error(
                        &out_tx_clone,
                        format!("workspace '{}' not found", workspace_id),
                    )
                    .await;
                    return;
                }
            };

            if !workspace.is_remote() {
                send_handshake_error(
                    &out_tx_clone,
                    format!("workspace '{}' is not a remote workspace", workspace_id),
                )
                .await;
                return;
            }

            // ── Step 4: Capacity check ──
            // Use count_remote() for accurate count (pool.count() is TOCTOU-prone
            // but acceptable here since we do a final check after get_or_create)
            if pool.count() >= config.max_remote_workspaces {
                // Also check the workspace repository for an authoritative count
                match workspace_repository.count_remote().await {
                    Ok(count) if count as usize >= config.max_remote_workspaces => {
                        send_handshake_error(
                            &out_tx_clone,
                            "max remote workspaces limit reached".to_string(),
                        )
                        .await;
                        return;
                    }
                    Err(e) => {
                        warn!("Failed to count remote workspaces: {}", e);
                        // Fall through — optimistic
                    }
                    _ => {}
                }
            }

            // ── Step 5: Create/get backend and bind stream ──
            let backend = pool.get_or_create(
                &workspace_id,
                config.remote_op_timeout_secs,
                config.remote_max_concurrent_requests,
                config.remote_data_stream_threshold,
                config.remote_transfer_timeout_secs,
            );

            // Create a dedicated sender for the control stream to the Client
            let (ctrl_tx, mut ctrl_rx) = mpsc::channel::<ServerStorageMessage>(256);
            backend.bind(ctrl_tx.clone()).await;

            // Register backend in StorageRouter
            storage_router.register(
                &workspace_id,
                backend.clone() as Arc<dyn StorageBackend>,
            );

            // Send handshake acknowledgment
            let _ = out_tx_clone
                .send(ServerStorageMessage {
                    message: Some(server_storage_message::Message::HandshakeAck(
                        StorageHandshakeAck {
                            success: true,
                            error: None,
                        },
                    )),
                })
                .await;

            info!(workspace_id = %workspace_id, "Client storage connected and authenticated");

            // Update connected client count metric
            crate::infra::metrics::set_remote_connected_clients(pool.connected_count());

            // ── Step 5.5: FUSE mount management ──
            // If the backend was previously connected (reconnection), purge all caches
            if backend.was_previously_connected() {
                info!(workspace_id = %workspace_id, "Client reconnected, purging FUSE caches");
                fuse_manager.purge_all_caches(&workspace_id);
            }
            // Mount FUSE if not already mounted
            if let Err(e) = fuse_manager.mount_if_not_exists(&workspace_id, backend.clone()).await {
                warn!(
                    workspace_id = %workspace_id,
                    error = %e,
                    "Failed to mount FUSE for remote workspace (non-fatal)"
                );
            }

            // ── Step 6: Forward control stream messages to the response stream ──
            let out_tx_ctrl = out_tx_clone.clone();
            let ws_id_ctrl = workspace_id.clone();
            tokio::spawn(async move {
                while let Some(msg) = ctrl_rx.recv().await {
                    if out_tx_ctrl.send(msg).await.is_err() {
                        debug!(workspace_id = %ws_id_ctrl, "Control forwarding ended");
                        break;
                    }
                }
            });

            // ── Step 7: Start heartbeat task with timeout detection ──
            let heartbeat_interval = std::time::Duration::from_secs(
                config.remote_heartbeat_interval_secs,
            );
            let heartbeat_timeout = std::time::Duration::from_secs(
                config.remote_heartbeat_timeout_secs,
            );
            // Use monotonic Instant to track last activity (immune to clock skew)
            // Store elapsed millis from a fixed reference point as AtomicU64
            let reference_instant = std::time::Instant::now();
            let last_activity = Arc::new(AtomicU64::new(0)); // 0 = reference_instant
            let last_activity_hb = last_activity.clone();
            let out_tx_hb = out_tx_clone.clone();
            let ws_id_hb = workspace_id.clone();
            let heartbeat_task = tokio::spawn(async move {
                let mut interval = tokio::time::interval(heartbeat_interval);
                loop {
                    interval.tick().await;

                    // Check if Client is still alive (heartbeat timeout detection)
                    let now_ms = reference_instant.elapsed().as_millis() as u64;
                    let last_ms = last_activity_hb.load(Ordering::Relaxed);
                    if now_ms.saturating_sub(last_ms) > heartbeat_timeout.as_millis() as u64 {
                        warn!(
                            workspace_id = %ws_id_hb,
                            elapsed_ms = now_ms.saturating_sub(last_ms),
                            timeout_ms = heartbeat_timeout.as_millis() as u64,
                            "Client heartbeat timeout, disconnecting"
                        );
                        break;
                    }

                    let ping = ServerStorageMessage {
                        message: Some(server_storage_message::Message::Ping(
                            StoragePing {
                                timestamp: now_ms,
                            },
                        )),
                    };
                    if out_tx_hb.send(ping).await.is_err() {
                        debug!(workspace_id = %ws_id_hb, "Heartbeat task ended (send failed)");
                        break;
                    }
                }
            });

            // ── Step 8: Main loop — process incoming Client messages ──
            while let Some(result) = inbound.next().await {
                // Update last activity using monotonic elapsed time
                last_activity.store(
                    reference_instant.elapsed().as_millis() as u64,
                    Ordering::Relaxed,
                );

                match result {
                    Ok(msg) => match msg.message {
                        Some(client_message::Message::OperationResponse(resp)) => {
                            debug!(
                                workspace_id = %workspace_id,
                                correlation_id = %resp.correlation_id,
                                "Received operation response"
                            );
                            backend.handle_response(resp);
                        }
                        Some(client_message::Message::FileChanged(notification)) => {
                            crate::infra::metrics::increment_file_change_notifications(
                                &workspace_id,
                            );
                            debug!(
                                workspace_id = %workspace_id,
                                event_count = notification.events.len(),
                                full_purge = notification.full_purge,
                                "Received file change notification"
                            );
                            if notification.full_purge {
                                fuse_manager.purge_all_caches(&workspace_id);
                            } else {
                                let paths: Vec<String> = notification
                                    .events
                                    .iter()
                                    .map(|e| e.path.clone())
                                    .collect();
                                if !paths.is_empty() {
                                    fuse_manager.invalidate_paths(&workspace_id, &paths);
                                }
                            }
                        }
                        Some(client_message::Message::Pong(pong)) => {
                            debug!(
                                workspace_id = %workspace_id,
                                timestamp = pong.timestamp,
                                "Received pong"
                            );
                        }
                        Some(client_message::Message::DataTransferFailed(dtf)) => {
                            warn!(
                                workspace_id = %workspace_id,
                                transfer_id = %dtf.transfer_id,
                                reason = %dtf.reason,
                                "Data transfer failed"
                            );
                            backend.handle_data_transfer_failed(
                                &dtf.transfer_id,
                                &dtf.reason,
                            );
                        }
                        Some(client_message::Message::Handshake(_)) => {
                            warn!(
                                workspace_id = %workspace_id,
                                "Unexpected duplicate handshake"
                            );
                        }
                        None => {
                            debug!(workspace_id = %workspace_id, "Empty message");
                        }
                    },
                    Err(e) => {
                        error!(
                            workspace_id = %workspace_id,
                            error = %e,
                            "Error receiving Client message"
                        );
                        break;
                    }
                }
            }

            // ── Step 9: Client disconnected — cleanup ──
            heartbeat_task.abort();
            pool.unbind_stream(&workspace_id).await;
            // Update connected client count metric after disconnect
            crate::infra::metrics::set_remote_connected_clients(pool.connected_count());
            info!(workspace_id = %workspace_id, "Client storage disconnected");
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }

    async fn read_file_stream(
        &self,
        request: Request<Streaming<ReadFileStreamRequest>>,
    ) -> Result<Response<ReadFileStreamResponse>, Status> {
        let mut stream = request.into_inner();

        // 1. Read header (first message)
        let header = match stream.next().await {
            Some(Ok(req)) => match req.payload {
                Some(read_file_stream_request::Payload::Header(h)) => h,
                _ => return Err(Status::invalid_argument("first message must be header")),
            },
            Some(Err(e)) => return Err(Status::internal(format!("stream error: {}", e))),
            None => return Err(Status::invalid_argument("missing header")),
        };

        // 2. Find the backend
        let backend = self
            .pool
            .get_backend(&header.workspace_id)
            .ok_or_else(|| Status::not_found("workspace not found"))?;

        // 3. Collect data chunks
        let mut data = Vec::new();
        while let Some(msg) = stream.next().await {
            match msg?.payload {
                Some(read_file_stream_request::Payload::Data(chunk)) => {
                    data.extend_from_slice(&chunk);
                }
                _ => break,
            }
        }

        let bytes_read = data.len() as u64;

        // 4. Record data transfer metrics
        crate::infra::metrics::record_data_transfer_bytes(
            &header.workspace_id,
            "read",
            bytes_read,
        );

        // 5. Complete the transfer
        backend.complete_read_transfer(&header.transfer_id, data);

        Ok(Response::new(ReadFileStreamResponse { bytes_read }))
    }

    type WriteFileStreamStream =
        Pin<Box<dyn Stream<Item = Result<WriteFileStreamResponse, Status>> + Send>>;

    async fn write_file_stream(
        &self,
        request: Request<WriteFileStreamRequest>,
    ) -> Result<Response<Self::WriteFileStreamStream>, Status> {
        let req = request.into_inner();

        // 1. Find the backend
        let backend = self
            .pool
            .get_backend(&req.workspace_id)
            .ok_or_else(|| Status::not_found("workspace not found"))?;

        // 2. Get the data to stream to Client
        let data = backend
            .get_write_data(&req.transfer_id)
            .ok_or_else(|| Status::not_found("transfer not found or already completed"))?;

        let total_bytes = data.len() as u64;
        let workspace_id = req.workspace_id.clone();
        let transfer_id = req.transfer_id.clone();
        let backend_clone = backend.clone();

        // 3. Stream data in 64KB chunks
        let (tx, rx) = mpsc::channel::<Result<WriteFileStreamResponse, Status>>(32);

        tokio::spawn(async move {
            const CHUNK_SIZE: usize = 64 * 1024;
            let mut offset = 0;

            while offset < data.len() {
                let end = (offset + CHUNK_SIZE).min(data.len());
                let chunk = data[offset..end].to_vec();
                offset = end;

                let resp = WriteFileStreamResponse {
                    payload: Some(write_file_stream_response::Payload::Data(chunk)),
                };

                if tx.send(Ok(resp)).await.is_err() {
                    warn!(
                        workspace_id = %workspace_id,
                        transfer_id = %transfer_id,
                        "WriteFileStream client disconnected during data transfer"
                    );
                    return;
                }
            }

            // 4. Send Done marker
            let done = WriteFileStreamResponse {
                payload: Some(write_file_stream_response::Payload::Done(
                    WriteFileStreamDone { total_bytes },
                )),
            };

            if tx.send(Ok(done)).await.is_err() {
                warn!(
                    workspace_id = %workspace_id,
                    transfer_id = %transfer_id,
                    "WriteFileStream client disconnected before Done marker"
                );
                return;
            }

            // 5. Record metrics and complete the transfer
            crate::infra::metrics::record_data_transfer_bytes(
                &workspace_id,
                "write",
                total_bytes,
            );
            backend_clone.complete_write_transfer(&transfer_id);
            backend_clone.remove_write_data(&transfer_id);
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }
}
