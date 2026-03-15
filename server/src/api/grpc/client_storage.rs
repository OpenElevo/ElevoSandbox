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
use crate::infra::tenant_repository::TenantRepository;
use crate::infra::workspace_repository::WorkspaceRepository;
use crate::proto::{
    client_message, client_storage_service_server::ClientStorageService, read_file_stream_request,
    server_storage_message, write_file_stream_response, ClientMessage, ReadFileStreamRequest,
    ReadFileStreamResponse, ServerStorageMessage, StorageHandshakeAck, StoragePing,
    WriteFileStreamDone, WriteFileStreamRequest, WriteFileStreamResponse,
};
use crate::service::api_key_usage::ApiKeyUsageTracker;

/// gRPC ClientStorageService implementation
pub struct ClientStorageServiceImpl {
    pool: Arc<RemoteStoragePool>,
    storage_router: Arc<StorageRouter>,
    workspace_repository: Arc<WorkspaceRepository>,
    tenant_repository: TenantRepository,
    config: Arc<Config>,
    fuse_manager: Arc<FuseMountManager>,
    api_key_usage: Arc<ApiKeyUsageTracker>,
}

impl ClientStorageServiceImpl {
    pub fn new(
        pool: Arc<RemoteStoragePool>,
        storage_router: Arc<StorageRouter>,
        workspace_repository: Arc<WorkspaceRepository>,
        tenant_repository: TenantRepository,
        config: Arc<Config>,
        fuse_manager: Arc<FuseMountManager>,
        api_key_usage: Arc<ApiKeyUsageTracker>,
    ) -> Self {
        Self {
            pool,
            storage_router,
            workspace_repository,
            tenant_repository,
            config,
            fuse_manager,
            api_key_usage,
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

/// Authentication result from token verification.
enum AuthResult {
    /// Authenticated via API Key — contains the tenant_id
    ApiKey { tenant_id: String },
}

/// Verify the Client token.
///
/// Validates API Keys (`sk_...`) by hashing and looking up in the database.
/// The associated tenant must be active. Returns the tenant_id for ownership checks.
/// Usage is tracked via the batching tracker.
async fn verify_token(
    tenant_repo: &TenantRepository,
    api_key_usage: &ApiKeyUsageTracker,
    token: &str,
) -> Result<AuthResult, String> {
    if !token.starts_with("sk_") {
        return Err("invalid token format: expected API key (sk_...)".to_string());
    }

    let result = tenant_repo
        .find_by_token_hash(token)
        .await
        .map_err(|e| format!("auth error: {}", e))?;

    let (key, tenant) = match result {
        Some(pair) => pair,
        None => return Err("unknown API key".to_string()),
    };

    if !key.is_usable() {
        return Err("API key revoked or expired".to_string());
    }

    if !tenant.is_active {
        return Err("tenant is deactivated".to_string());
    }

    // Track usage via the batching tracker
    api_key_usage.update(key.id);

    Ok(AuthResult::ApiKey {
        tenant_id: tenant.id.to_string(),
    })
}

#[tonic::async_trait]
impl ClientStorageService for ClientStorageServiceImpl {
    type ConnectStream = Pin<Box<dyn Stream<Item = Result<ServerStorageMessage, Status>> + Send>>;

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

        let tenant_repository = self.tenant_repository.clone();
        let api_key_usage = self.api_key_usage.clone();
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

            // ── Step 2: Authenticate token (API Key) ──
            let auth_result = match verify_token(&tenant_repository, &api_key_usage, &token).await {
                Ok(result) => result,
                Err(msg) => {
                    send_handshake_error(&out_tx_clone, format!("authentication failed: {}", msg))
                        .await;
                    warn!(workspace_id = %workspace_id, "Client storage authentication failed");
                    return;
                }
            };

            // Verify the tenant owns this workspace/namespace
            let AuthResult::ApiKey { ref tenant_id } = auth_result;
            if *tenant_id != workspace_id {
                send_handshake_error(
                    &out_tx_clone,
                    "API key does not have access to this workspace".to_string(),
                )
                .await;
                warn!(
                    workspace_id = %workspace_id,
                    tenant_id = %tenant_id,
                    "Client storage: tenant does not own workspace"
                );
                return;
            }

            // ── Step 3: Verify tenant exists and uses remote storage ──
            let tenant_uuid = match uuid::Uuid::parse_str(&workspace_id) {
                Ok(u) => u,
                Err(_) => {
                    send_handshake_error(
                        &out_tx_clone,
                        format!("invalid namespace ID: {}", workspace_id),
                    )
                    .await;
                    return;
                }
            };

            let tenant = match tenant_repository.get_tenant(tenant_uuid).await {
                Ok(t) => t,
                Err(_) => {
                    send_handshake_error(
                        &out_tx_clone,
                        format!("namespace '{}' not found", workspace_id),
                    )
                    .await;
                    return;
                }
            };

            if tenant.storage_type != crate::domain::workspace::StorageType::Remote {
                send_handshake_error(
                    &out_tx_clone,
                    format!(
                        "namespace '{}' is not configured for remote storage",
                        workspace_id
                    ),
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
            storage_router.register(&workspace_id, backend.clone() as Arc<dyn StorageBackend>);

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
            if let Err(e) = fuse_manager
                .mount_if_not_exists(&workspace_id, backend.clone())
                .await
            {
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
            let heartbeat_interval =
                std::time::Duration::from_secs(config.remote_heartbeat_interval_secs);
            let heartbeat_timeout =
                std::time::Duration::from_secs(config.remote_heartbeat_timeout_secs);
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
                        message: Some(server_storage_message::Message::Ping(StoragePing {
                            timestamp: now_ms,
                        })),
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
                                let paths: Vec<String> =
                                    notification.events.iter().map(|e| e.path.clone()).collect();
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
                            backend.handle_data_transfer_failed(&dtf.transfer_id, &dtf.reason);
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
        crate::infra::metrics::record_data_transfer_bytes(&header.workspace_id, "read", bytes_read);

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
            crate::infra::metrics::record_data_transfer_bytes(&workspace_id, "write", total_bytes);
            backend_clone.complete_write_transfer(&transfer_id);
            backend_clone.remove_write_data(&transfer_id);
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }
}
