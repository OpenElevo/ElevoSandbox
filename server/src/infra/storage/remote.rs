//! Remote storage backend — proxies file operations to a connected Client
//! via a gRPC bidirectional stream.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot, Notify, Semaphore};
use tracing::{debug, warn};
use uuid::Uuid;

use crate::proto::{
    server_storage_message, storage_operation_request, storage_operation_response,
    storage_operation_success, CopyRequest, CreateFileRequest, DataTransferOperation,
    ExistsRequest, FileStatData, ListDirData, ListDirRequest, ReadFileRangeRequest,
    ReadLinkRequest, RemoveDirRequest, RemoveFileRequest, RenameRequest, ServerStorageMessage,
    SetFileSizeRequest, SetPermissionsRequest, SetTimesRequest, StartDataTransfer, StatFsRequest,
    StatRequest, StorageErrorCode, StorageMkdirRequest, StorageOperationError,
    StorageOperationRequest, StorageOperationResponse, StorageOperationSuccess, SymlinkRequest,
    WriteFileAtRequest,
};

use super::{FileStat, FileType, FsStats, StorageBackend, StorageError, StorageResult};

/// Connection state for a remote storage backend
const STATE_PENDING: u8 = 0;
const STATE_CONNECTED: u8 = 1;
const STATE_DISCONNECTED: u8 = 2;

/// Manages all remote storage backends.
pub struct RemoteStoragePool {
    backends: DashMap<String, Arc<RemoteStorageBackend>>,
}

impl RemoteStoragePool {
    pub fn new() -> Self {
        Self {
            backends: DashMap::new(),
        }
    }

    /// Get or create a backend for a workspace
    pub fn get_or_create(
        &self,
        workspace_id: &str,
        op_timeout_secs: u64,
        max_concurrent: usize,
        data_stream_threshold: usize,
        transfer_timeout_secs: u64,
    ) -> Arc<RemoteStorageBackend> {
        self.backends
            .entry(workspace_id.to_string())
            .or_insert_with(|| {
                Arc::new(RemoteStorageBackend::new(
                    workspace_id.to_string(),
                    op_timeout_secs,
                    max_concurrent,
                    data_stream_threshold,
                    transfer_timeout_secs,
                ))
            })
            .value()
            .clone()
    }

    /// Bind a stream to an existing backend (Client connected)
    pub async fn bind_stream(
        &self,
        workspace_id: &str,
        tx: mpsc::Sender<ServerStorageMessage>,
    ) -> Option<Arc<RemoteStorageBackend>> {
        if let Some(backend) = self.backends.get(workspace_id) {
            backend.bind(tx).await;
            Some(backend.value().clone())
        } else {
            None
        }
    }

    /// Unbind stream (Client disconnected)
    pub async fn unbind_stream(&self, workspace_id: &str) {
        if let Some(backend) = self.backends.get(workspace_id) {
            backend.unbind().await;
        }
    }

    /// Remove a backend entirely
    pub async fn remove(&self, workspace_id: &str) {
        if let Some((_, backend)) = self.backends.remove(workspace_id) {
            backend.unbind().await;
        }
    }

    /// Check how many backends exist
    pub fn count(&self) -> usize {
        self.backends.len()
    }

    /// Count how many backends are currently connected (have an active Client stream).
    pub fn connected_count(&self) -> usize {
        self.backends
            .iter()
            .filter(|entry| entry.value().is_connected())
            .count()
    }

    /// Get a backend by workspace ID.
    pub fn get_backend(&self, workspace_id: &str) -> Option<Arc<RemoteStorageBackend>> {
        self.backends.get(workspace_id).map(|e| e.value().clone())
    }
}

/// Remote storage backend that proxies operations to a Client via gRPC stream.
pub struct RemoteStorageBackend {
    workspace_id: String,
    /// Sender half of the control stream to the Client
    stream_tx: tokio::sync::RwLock<Option<mpsc::Sender<ServerStorageMessage>>>,
    /// Pending request waiters: correlation_id -> oneshot sender
    pending_requests: DashMap<String, oneshot::Sender<StorageOperationResponse>>,
    /// Accumulator for paginated ListDir responses (correlation_id -> accumulated entries)
    pending_paged: DashMap<String, Vec<FileStatData>>,
    /// Pending data transfers: transfer_id -> oneshot sender for completion/failure
    pending_transfers: DashMap<String, oneshot::Sender<DataTransferResult>>,
    /// Concurrency limiter
    concurrent_semaphore: Semaphore,
    /// Connection state
    state: AtomicU8,
    /// Notifier for state changes (wakes operations waiting for reconnection)
    state_notify: Notify,
    /// Operation timeout
    op_timeout: std::time::Duration,
    /// File size threshold above which data transfers use dedicated data stream RPCs
    data_stream_threshold: usize,
    /// Timeout for data stream transfer completion
    transfer_timeout: std::time::Duration,
    /// Data held for pending write transfers (transfer_id -> data for Client to download)
    pending_write_data: DashMap<String, Arc<Vec<u8>>>,
}

/// Result of a data transfer operation
pub enum DataTransferResult {
    /// Data successfully received from Client (for read transfers)
    ReadComplete(Vec<u8>),
    /// Write transfer acknowledged by Client
    WriteComplete,
    /// Transfer failed
    Failed(String),
}

impl RemoteStorageBackend {
    pub fn new(
        workspace_id: String,
        op_timeout_secs: u64,
        max_concurrent: usize,
        data_stream_threshold: usize,
        transfer_timeout_secs: u64,
    ) -> Self {
        Self {
            workspace_id,
            stream_tx: tokio::sync::RwLock::new(None),
            pending_requests: DashMap::new(),
            pending_paged: DashMap::new(),
            pending_transfers: DashMap::new(),
            concurrent_semaphore: Semaphore::new(max_concurrent),
            state: AtomicU8::new(STATE_PENDING),
            state_notify: Notify::new(),
            op_timeout: std::time::Duration::from_secs(op_timeout_secs),
            data_stream_threshold,
            transfer_timeout: std::time::Duration::from_secs(transfer_timeout_secs),
            pending_write_data: DashMap::new(),
        }
    }

    /// Bind a stream sender (Client connected).
    /// Must be called from an async context.
    pub async fn bind(&self, tx: mpsc::Sender<ServerStorageMessage>) {
        let mut lock = self.stream_tx.write().await;
        *lock = Some(tx);
        self.state.store(STATE_CONNECTED, Ordering::Release);
        self.state_notify.notify_waiters();
    }

    /// Unbind the stream (Client disconnected), fail all pending requests.
    /// Must be called from an async context.
    pub async fn unbind(&self) {
        {
            let mut lock = self.stream_tx.write().await;
            *lock = None;
        }
        self.state.store(STATE_DISCONNECTED, Ordering::Release);

        // Drain all pending requests with error
        let ids: Vec<String> = self
            .pending_requests
            .iter()
            .map(|e| e.key().clone())
            .collect();
        for id in ids {
            if let Some((_, tx)) = self.pending_requests.remove(&id) {
                let _ = tx.send(StorageOperationResponse {
                    correlation_id: id,
                    result: Some(storage_operation_response::Result::Error(
                        StorageOperationError {
                            code: StorageErrorCode::IoError.into(),
                            message: "client disconnected".to_string(),
                        },
                    )),
                });
            }
        }

        // Drain paginated accumulators
        self.pending_paged.clear();

        // Drain pending data transfers
        let transfer_ids: Vec<String> = self
            .pending_transfers
            .iter()
            .map(|e| e.key().clone())
            .collect();
        for id in transfer_ids {
            if let Some((_, tx)) = self.pending_transfers.remove(&id) {
                let _ = tx.send(DataTransferResult::Failed(
                    "client disconnected".to_string(),
                ));
            }
        }

        // Clean up any pending write data
        self.pending_write_data.clear();
    }

    /// Check if the backend is currently connected
    pub fn is_connected(&self) -> bool {
        self.state.load(Ordering::Acquire) == STATE_CONNECTED
    }

    /// Check if the backend was previously connected (i.e., state is DISCONNECTED, not PENDING).
    /// Used to determine if FUSE caches should be fully purged on reconnection.
    pub fn was_previously_connected(&self) -> bool {
        self.state.load(Ordering::Acquire) == STATE_DISCONNECTED
    }

    /// Handle an incoming operation response from the Client.
    /// Supports paginated responses (ListDir with is_last flag).
    pub fn handle_response(&self, response: StorageOperationResponse) {
        let correlation_id = response.correlation_id.clone();

        // Check if this is a paginated ListDir response (is_last=false means more pages coming).
        // Pagination only applies to ListDir — other operations always send a single response.
        if let Some(storage_operation_response::Result::Success(ref success)) = response.result {
            if !success.is_last {
                // Only accumulate if this is actually a ListDir response
                if let Some(storage_operation_success::Data::ListDir(ref list_data)) = success.data
                {
                    self.pending_paged
                        .entry(correlation_id.clone())
                        .or_default()
                        .extend(list_data.entries.iter().cloned());
                    return; // don't send to oneshot yet, more pages coming
                }
                // Non-ListDir with is_last=false: treat as complete (is_last defaults to false
                // in protobuf, and most clients don't set it for non-paginated responses)
            }

            // is_last=true: check if we have accumulated pages to merge
            if let Some((_, mut accumulated)) = self.pending_paged.remove(&correlation_id) {
                // Merge the final page's entries with accumulated ones
                if let Some(storage_operation_success::Data::ListDir(ref list_data)) = success.data
                {
                    accumulated.extend(list_data.entries.iter().cloned());
                }
                // Build the combined response
                let final_response = StorageOperationResponse {
                    correlation_id: correlation_id.clone(),
                    result: Some(storage_operation_response::Result::Success(
                        StorageOperationSuccess {
                            data: Some(storage_operation_success::Data::ListDir(ListDirData {
                                entries: accumulated,
                            })),
                            is_last: true,
                        },
                    )),
                };
                if let Some((_, tx)) = self.pending_requests.remove(&correlation_id) {
                    let _ = tx.send(final_response);
                } else {
                    warn!(
                        workspace_id = %self.workspace_id,
                        correlation_id = %correlation_id,
                        "received paginated response for unknown correlation_id"
                    );
                }
                return;
            }
        }

        // Non-paginated or single-page response: send directly
        if let Some((_, tx)) = self.pending_requests.remove(&correlation_id) {
            debug!(
                workspace_id = %self.workspace_id,
                correlation_id = %correlation_id,
                "dispatching response to oneshot"
            );
            match tx.send(response) {
                Ok(()) => {
                    debug!(
                        workspace_id = %self.workspace_id,
                        correlation_id = %correlation_id,
                        "oneshot send succeeded"
                    );
                }
                Err(_resp) => {
                    warn!(
                        workspace_id = %self.workspace_id,
                        correlation_id = %correlation_id,
                        "oneshot receiver was dropped before send"
                    );
                }
            }
        } else {
            warn!(
                workspace_id = %self.workspace_id,
                correlation_id = %correlation_id,
                pending_count = self.pending_requests.len(),
                "received response for unknown correlation_id"
            );
        }
    }

    /// Handle a data transfer failure notification from the Client
    pub fn handle_data_transfer_failed(&self, transfer_id: &str, reason: &str) {
        if let Some((_, tx)) = self.pending_transfers.remove(transfer_id) {
            let _ = tx.send(DataTransferResult::Failed(reason.to_string()));
        } else {
            warn!(
                workspace_id = %self.workspace_id,
                transfer_id = %transfer_id,
                "received data transfer failed for unknown transfer_id"
            );
        }
    }

    /// Complete a read data transfer with the received data
    pub fn complete_read_transfer(&self, transfer_id: &str, data: Vec<u8>) {
        if let Some((_, tx)) = self.pending_transfers.remove(transfer_id) {
            let _ = tx.send(DataTransferResult::ReadComplete(data));
        }
    }

    /// Complete a write data transfer
    pub fn complete_write_transfer(&self, transfer_id: &str) {
        if let Some((_, tx)) = self.pending_transfers.remove(transfer_id) {
            let _ = tx.send(DataTransferResult::WriteComplete);
        }
    }

    /// Register a pending data transfer and return the receiver
    pub fn register_transfer(&self, transfer_id: String) -> oneshot::Receiver<DataTransferResult> {
        let (tx, rx) = oneshot::channel();
        self.pending_transfers.insert(transfer_id, tx);
        rx
    }

    /// Get the operation name from a request operation variant (for metrics).
    fn operation_name(op: &storage_operation_request::Operation) -> &'static str {
        use storage_operation_request::Operation;
        match op {
            Operation::ReadFileRange(_) => "read_file_range",
            Operation::WriteFileAt(_) => "write_file_at",
            Operation::CreateFile(_) => "create_file",
            Operation::Stat(_) => "stat",
            Operation::ListDir(_) => "list_dir",
            Operation::Exists(_) => "exists",
            Operation::Mkdir(_) => "mkdir",
            Operation::RemoveFile(_) => "remove_file",
            Operation::RemoveDir(_) => "remove_dir",
            Operation::Rename(_) => "rename",
            Operation::Copy(_) => "copy",
            Operation::SetFileSize(_) => "set_file_size",
            Operation::SetPermissions(_) => "set_permissions",
            Operation::SetTimes(_) => "set_times",
            Operation::Symlink(_) => "symlink",
            Operation::ReadLink(_) => "readlink",
            Operation::StatFs(_) => "stat_fs",
        }
    }

    /// Send a request to the Client and wait for the response.
    async fn send_request(
        &self,
        operation: storage_operation_request::Operation,
    ) -> StorageResult<StorageOperationResponse> {
        let op_name = Self::operation_name(&operation);
        let start = std::time::Instant::now();
        // Acquire concurrency permit
        let _permit = self
            .concurrent_semaphore
            .acquire()
            .await
            .map_err(|_| StorageError::Internal("semaphore closed".to_string()))?;

        // Wait for connection (with op_timeout) instead of failing immediately
        self.ensure_connected().await?;

        let correlation_id = Uuid::now_v7().to_string();
        let (resp_tx, resp_rx) = oneshot::channel();

        // Register waiter
        self.pending_requests
            .insert(correlation_id.clone(), resp_tx);
        crate::infra::metrics::set_remote_pending_requests(
            &self.workspace_id,
            self.pending_requests.len(),
        );

        // Build and send request
        let msg = ServerStorageMessage {
            message: Some(server_storage_message::Message::OperationRequest(
                StorageOperationRequest {
                    correlation_id: correlation_id.clone(),
                    operation: Some(operation),
                },
            )),
        };

        let send_result = {
            let tx_guard = self.stream_tx.read().await;
            match tx_guard.as_ref() {
                Some(tx) => tx.send(msg).await,
                None => {
                    self.pending_requests.remove(&correlation_id);
                    return Err(StorageError::Internal(format!(
                        "remote storage stream not available for workspace '{}'",
                        self.workspace_id
                    )));
                }
            }
        };

        if let Err(e) = send_result {
            self.pending_requests.remove(&correlation_id);
            return Err(StorageError::Internal(format!(
                "failed to send request to client: {}",
                e
            )));
        }

        debug!(
            workspace_id = %self.workspace_id,
            correlation_id = %correlation_id,
            op = op_name,
            "waiting for response"
        );

        // Wait for response with timeout
        let result = match tokio::time::timeout(self.op_timeout, resp_rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                // oneshot sender dropped (client disconnected)
                crate::infra::metrics::increment_remote_op_error(op_name, "disconnected");
                Err(StorageError::Internal(
                    "client disconnected while waiting for response".to_string(),
                ))
            }
            Err(_) => {
                // Timeout - remove pending request
                self.pending_requests.remove(&correlation_id);
                crate::infra::metrics::increment_remote_op_timeout(op_name);
                Err(StorageError::Internal(format!(
                    "operation timed out after {:?}",
                    self.op_timeout
                )))
            }
        };

        let duration = start.elapsed().as_secs_f64();
        crate::infra::metrics::record_remote_op(op_name, duration);
        crate::infra::metrics::set_remote_pending_requests(
            &self.workspace_id,
            self.pending_requests.len(),
        );

        if let Ok(ref resp) = result {
            if let Some(storage_operation_response::Result::Error(ref e)) = resp.result {
                let code =
                    StorageErrorCode::try_from(e.code).unwrap_or(StorageErrorCode::Unspecified);
                crate::infra::metrics::increment_remote_op_error(op_name, &format!("{:?}", code));
            }
        }

        result
    }

    /// Wait until the backend is connected, or return error if op_timeout expires.
    async fn ensure_connected(&self) -> StorageResult<()> {
        if self.state.load(Ordering::Acquire) == STATE_CONNECTED {
            return Ok(());
        }
        tokio::time::timeout(self.op_timeout, self.wait_for_connected())
            .await
            .map_err(|_| StorageError::Io {
                path: self.workspace_id.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "client not connected within timeout",
                ),
            })
    }

    /// Spin-wait on the state notify until STATE_CONNECTED is observed.
    async fn wait_for_connected(&self) {
        loop {
            if self.state.load(Ordering::Acquire) == STATE_CONNECTED {
                return;
            }
            self.state_notify.notified().await;
        }
    }

    /// Number of pending in-flight requests
    pub fn pending_count(&self) -> usize {
        self.pending_requests.len()
    }

    /// Get write data stored for a pending write transfer.
    /// Called by the WriteFileStream RPC handler to get data to stream to Client.
    pub fn get_write_data(&self, transfer_id: &str) -> Option<Arc<Vec<u8>>> {
        self.pending_write_data.get(transfer_id).map(|r| r.clone())
    }

    /// Remove write data for a completed or failed transfer.
    pub fn remove_write_data(&self, transfer_id: &str) {
        self.pending_write_data.remove(transfer_id);
    }

    /// Write file via data stream: stores the data, sends StartDataTransfer
    /// to the Client, and waits for the Client to complete the WriteFileStream RPC.
    async fn write_file_via_data_stream(&self, path: &str, content: &[u8]) -> StorageResult<()> {
        self.ensure_connected().await?;

        let transfer_id = Uuid::now_v7().to_string();
        let data = Arc::new(content.to_vec());

        // Store write data for the Client to fetch via WriteFileStream RPC
        self.pending_write_data
            .insert(transfer_id.clone(), data.clone());

        // Register transfer waiter
        let rx = self.register_transfer(transfer_id.clone());

        // Send StartDataTransfer notification to Client via control stream
        let msg = ServerStorageMessage {
            message: Some(server_storage_message::Message::StartDataTransfer(
                StartDataTransfer {
                    transfer_id: transfer_id.clone(),
                    operation: DataTransferOperation::WriteFile.into(),
                    path: path.to_string(),
                    file_size: Some(content.len() as u64),
                    offset: None,
                    length: None,
                },
            )),
        };

        {
            let tx_guard = self.stream_tx.read().await;
            match tx_guard.as_ref() {
                Some(tx) => {
                    if let Err(e) = tx.send(msg).await {
                        self.pending_write_data.remove(&transfer_id);
                        self.pending_transfers.remove(&transfer_id);
                        return Err(StorageError::Internal(format!(
                            "failed to send StartDataTransfer to client: {}",
                            e
                        )));
                    }
                }
                None => {
                    self.pending_write_data.remove(&transfer_id);
                    self.pending_transfers.remove(&transfer_id);
                    return Err(StorageError::Internal(format!(
                        "remote storage stream not available for workspace '{}'",
                        self.workspace_id
                    )));
                }
            }
        }

        // Wait for transfer completion with transfer_timeout
        match tokio::time::timeout(self.transfer_timeout, rx).await {
            Ok(Ok(DataTransferResult::WriteComplete)) => {
                self.pending_write_data.remove(&transfer_id);
                Ok(())
            }
            Ok(Ok(DataTransferResult::Failed(reason))) => {
                self.pending_write_data.remove(&transfer_id);
                Err(StorageError::Internal(format!(
                    "write data transfer failed: {}",
                    reason
                )))
            }
            Ok(Ok(DataTransferResult::ReadComplete(_))) => {
                self.pending_write_data.remove(&transfer_id);
                Err(StorageError::Internal(
                    "unexpected ReadComplete for write transfer".to_string(),
                ))
            }
            Ok(Err(_)) => {
                self.pending_write_data.remove(&transfer_id);
                Err(StorageError::Internal(
                    "transfer channel closed unexpectedly".to_string(),
                ))
            }
            Err(_) => {
                self.pending_write_data.remove(&transfer_id);
                self.pending_transfers.remove(&transfer_id);
                Err(StorageError::Internal(format!(
                    "write data transfer timed out after {:?}",
                    self.transfer_timeout
                )))
            }
        }
    }

    /// Read file via data stream: sends StartDataTransfer to the Client,
    /// and waits for the Client to complete the ReadFileStream RPC.
    /// Currently unused — read_file uses the control stream since file size isn't
    /// known beforehand. Available for explicit data-stream reads in the future.
    #[allow(dead_code)]
    async fn read_file_via_data_stream(&self, path: &str) -> StorageResult<Vec<u8>> {
        self.ensure_connected().await?;

        let transfer_id = Uuid::now_v7().to_string();

        // Register transfer waiter
        let rx = self.register_transfer(transfer_id.clone());

        // Send StartDataTransfer notification to Client via control stream
        let msg = ServerStorageMessage {
            message: Some(server_storage_message::Message::StartDataTransfer(
                StartDataTransfer {
                    transfer_id: transfer_id.clone(),
                    operation: DataTransferOperation::ReadFile.into(),
                    path: path.to_string(),
                    file_size: None,
                    offset: None,
                    length: None,
                },
            )),
        };

        {
            let tx_guard = self.stream_tx.read().await;
            match tx_guard.as_ref() {
                Some(tx) => {
                    if let Err(e) = tx.send(msg).await {
                        self.pending_transfers.remove(&transfer_id);
                        return Err(StorageError::Internal(format!(
                            "failed to send StartDataTransfer to client: {}",
                            e
                        )));
                    }
                }
                None => {
                    self.pending_transfers.remove(&transfer_id);
                    return Err(StorageError::Internal(format!(
                        "remote storage stream not available for workspace '{}'",
                        self.workspace_id
                    )));
                }
            }
        }

        // Wait for transfer completion with transfer_timeout
        match tokio::time::timeout(self.transfer_timeout, rx).await {
            Ok(Ok(DataTransferResult::ReadComplete(data))) => Ok(data),
            Ok(Ok(DataTransferResult::Failed(reason))) => Err(StorageError::Internal(format!(
                "read data transfer failed: {}",
                reason
            ))),
            Ok(Ok(DataTransferResult::WriteComplete)) => Err(StorageError::Internal(
                "unexpected WriteComplete for read transfer".to_string(),
            )),
            Ok(Err(_)) => Err(StorageError::Internal(
                "transfer channel closed unexpectedly".to_string(),
            )),
            Err(_) => {
                self.pending_transfers.remove(&transfer_id);
                Err(StorageError::Internal(format!(
                    "read data transfer timed out after {:?}",
                    self.transfer_timeout
                )))
            }
        }
    }
}

// ── Helper: convert proto response to domain types ──

fn proto_error_to_storage(err: &StorageOperationError) -> StorageError {
    let code = StorageErrorCode::try_from(err.code).unwrap_or(StorageErrorCode::Unspecified);
    match code {
        StorageErrorCode::NotFound => StorageError::NotFound(err.message.clone()),
        StorageErrorCode::AlreadyExists => StorageError::AlreadyExists(err.message.clone()),
        StorageErrorCode::IsADirectory => StorageError::IsADirectory(err.message.clone()),
        StorageErrorCode::NotADirectory => StorageError::NotADirectory(err.message.clone()),
        StorageErrorCode::NotAFile => StorageError::NotAFile(err.message.clone()),
        StorageErrorCode::DirectoryNotEmpty => StorageError::DirectoryNotEmpty(err.message.clone()),
        StorageErrorCode::PermissionDenied => StorageError::PermissionDenied(err.message.clone()),
        StorageErrorCode::PathTraversalDenied => {
            StorageError::PathTraversalDenied(err.message.clone())
        }
        StorageErrorCode::NotSupported => StorageError::NotSupported(err.message.clone()),
        StorageErrorCode::IoError => StorageError::Io {
            path: err.message.clone(),
            source: std::io::Error::other(err.message.clone()),
        },
        StorageErrorCode::Unspecified => StorageError::Internal(err.message.clone()),
    }
}

/// Convert a proto FileStatData to domain FileStat
fn proto_stat_to_domain(s: &FileStatData) -> FileStat {
    let file_type = match s.file_type {
        1 => FileType::Directory,
        2 => FileType::Symlink,
        _ => FileType::File,
    };
    FileStat {
        name: s.name.clone(),
        path: s.path.clone(),
        file_type,
        size: s.size,
        mode: s.mode,
        uid: s.uid,
        gid: s.gid,
        modified_at: s
            .modified_at
            .as_ref()
            .map(|t| DateTime::from_timestamp(t.seconds, t.nanos as u32).unwrap_or_default()),
        accessed_at: s
            .accessed_at
            .as_ref()
            .map(|t| DateTime::from_timestamp(t.seconds, t.nanos as u32).unwrap_or_default()),
        created_at: s
            .created_at
            .as_ref()
            .map(|t| DateTime::from_timestamp(t.seconds, t.nanos as u32).unwrap_or_default()),
    }
}

/// Extract a successful result from a StorageOperationResponse
fn extract_success(
    resp: StorageOperationResponse,
) -> StorageResult<storage_operation_success::Data> {
    match resp.result {
        Some(storage_operation_response::Result::Success(s)) => match s.data {
            Some(data) => Ok(data),
            None => Err(StorageError::Internal(
                "empty success response from client".to_string(),
            )),
        },
        Some(storage_operation_response::Result::Error(e)) => Err(proto_error_to_storage(&e)),
        None => Err(StorageError::Internal(
            "empty response from client".to_string(),
        )),
    }
}

/// Extract a successful result that has no data (Empty)
fn extract_empty(resp: StorageOperationResponse) -> StorageResult<()> {
    match resp.result {
        Some(storage_operation_response::Result::Success(_)) => Ok(()),
        Some(storage_operation_response::Result::Error(e)) => Err(proto_error_to_storage(&e)),
        None => Err(StorageError::Internal(
            "empty response from client".to_string(),
        )),
    }
}

#[async_trait]
impl StorageBackend for RemoteStorageBackend {
    async fn read_file(&self, _workspace_id: &str, path: &str) -> StorageResult<Vec<u8>> {
        let resp = self
            .send_request(storage_operation_request::Operation::ReadFileRange(
                ReadFileRangeRequest {
                    path: path.to_string(),
                    offset: 0,
                    length: 0, // 0 = read entire file
                },
            ))
            .await?;

        match extract_success(resp)? {
            storage_operation_success::Data::ReadData(r) => Ok(r.data),
            other => Err(StorageError::Internal(format!(
                "unexpected response type for read_file: {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    async fn read_file_range(
        &self,
        _workspace_id: &str,
        path: &str,
        offset: u64,
        length: u32,
    ) -> StorageResult<Vec<u8>> {
        let resp = self
            .send_request(storage_operation_request::Operation::ReadFileRange(
                ReadFileRangeRequest {
                    path: path.to_string(),
                    offset,
                    length,
                },
            ))
            .await?;

        match extract_success(resp)? {
            storage_operation_success::Data::ReadData(r) => Ok(r.data),
            other => Err(StorageError::Internal(format!(
                "unexpected response type for read_file_range: {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    async fn write_file(
        &self,
        _workspace_id: &str,
        path: &str,
        content: &[u8],
    ) -> StorageResult<()> {
        // Use data stream for large files to avoid bloating the control stream
        if content.len() > self.data_stream_threshold {
            return self.write_file_via_data_stream(path, content).await;
        }

        let resp = self
            .send_request(storage_operation_request::Operation::WriteFileAt(
                WriteFileAtRequest {
                    path: path.to_string(),
                    offset: 0,
                    data: content.to_vec(),
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn write_file_at(
        &self,
        _workspace_id: &str,
        path: &str,
        offset: u64,
        data: &[u8],
    ) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::WriteFileAt(
                WriteFileAtRequest {
                    path: path.to_string(),
                    offset,
                    data: data.to_vec(),
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn create_file(
        &self,
        _workspace_id: &str,
        path: &str,
        exclusive: bool,
    ) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::CreateFile(
                CreateFileRequest {
                    path: path.to_string(),
                    exclusive,
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn stat(&self, _workspace_id: &str, path: &str) -> StorageResult<FileStat> {
        let resp = self
            .send_request(storage_operation_request::Operation::Stat(StatRequest {
                path: path.to_string(),
            }))
            .await?;

        match extract_success(resp)? {
            storage_operation_success::Data::Stat(s) => Ok(proto_stat_to_domain(&s)),
            other => Err(StorageError::Internal(format!(
                "unexpected response type for stat: {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    async fn list_dir(&self, _workspace_id: &str, path: &str) -> StorageResult<Vec<FileStat>> {
        let resp = self
            .send_request(storage_operation_request::Operation::ListDir(
                ListDirRequest {
                    path: path.to_string(),
                },
            ))
            .await?;

        match extract_success(resp)? {
            storage_operation_success::Data::ListDir(d) => {
                Ok(d.entries.iter().map(proto_stat_to_domain).collect())
            }
            other => Err(StorageError::Internal(format!(
                "unexpected response type for list_dir: {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    async fn exists(&self, _workspace_id: &str, path: &str) -> StorageResult<bool> {
        let resp = self
            .send_request(storage_operation_request::Operation::Exists(
                ExistsRequest {
                    path: path.to_string(),
                },
            ))
            .await?;

        match extract_success(resp)? {
            storage_operation_success::Data::Exists(e) => Ok(e.exists),
            other => Err(StorageError::Internal(format!(
                "unexpected response type for exists: {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    async fn mkdir(&self, _workspace_id: &str, path: &str, recursive: bool) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::Mkdir(
                StorageMkdirRequest {
                    path: path.to_string(),
                    recursive,
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn remove_file(&self, _workspace_id: &str, path: &str) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::RemoveFile(
                RemoveFileRequest {
                    path: path.to_string(),
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn remove_dir(
        &self,
        _workspace_id: &str,
        path: &str,
        recursive: bool,
    ) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::RemoveDir(
                RemoveDirRequest {
                    path: path.to_string(),
                    recursive,
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn rename(&self, _workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::Rename(
                RenameRequest {
                    src: src.to_string(),
                    dst: dst.to_string(),
                    flags: 0, // normal rename
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn rename_noreplace(
        &self,
        _workspace_id: &str,
        src: &str,
        dst: &str,
    ) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::Rename(
                RenameRequest {
                    src: src.to_string(),
                    dst: dst.to_string(),
                    flags: 1, // noreplace
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn rename_exchange(
        &self,
        _workspace_id: &str,
        src: &str,
        dst: &str,
    ) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::Rename(
                RenameRequest {
                    src: src.to_string(),
                    dst: dst.to_string(),
                    flags: 2, // exchange
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn copy(&self, _workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::Copy(CopyRequest {
                src: src.to_string(),
                dst: dst.to_string(),
            }))
            .await?;

        extract_empty(resp)
    }

    async fn create_workspace_root(&self, _workspace_id: &str) -> StorageResult<()> {
        // Remote workspace root is managed by the Client — no server-side directory needed.
        // The FUSE mount point directory is managed by FuseMountManager.
        Ok(())
    }

    async fn delete_workspace_root(&self, _workspace_id: &str) -> StorageResult<()> {
        // Remote workspace cleanup is managed by the Client — server only needs
        // to umount FUSE and unregister from StorageRouter.
        Ok(())
    }

    async fn set_file_size(&self, _workspace_id: &str, path: &str, size: u64) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::SetFileSize(
                SetFileSizeRequest {
                    path: path.to_string(),
                    size,
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn set_permissions(
        &self,
        _workspace_id: &str,
        path: &str,
        mode: u32,
    ) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::SetPermissions(
                SetPermissionsRequest {
                    path: path.to_string(),
                    mode,
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn set_times(
        &self,
        _workspace_id: &str,
        path: &str,
        atime: Option<DateTime<Utc>>,
        mtime: Option<DateTime<Utc>>,
    ) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::SetTimes(
                SetTimesRequest {
                    path: path.to_string(),
                    atime: atime.map(|t| prost_types::Timestamp {
                        seconds: t.timestamp(),
                        nanos: t.timestamp_subsec_nanos() as i32,
                    }),
                    mtime: mtime.map(|t| prost_types::Timestamp {
                        seconds: t.timestamp(),
                        nanos: t.timestamp_subsec_nanos() as i32,
                    }),
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn symlink(
        &self,
        _workspace_id: &str,
        link_path: &str,
        target: &str,
    ) -> StorageResult<()> {
        let resp = self
            .send_request(storage_operation_request::Operation::Symlink(
                SymlinkRequest {
                    link_path: link_path.to_string(),
                    target: target.to_string(),
                },
            ))
            .await?;

        extract_empty(resp)
    }

    async fn readlink(&self, _workspace_id: &str, path: &str) -> StorageResult<String> {
        let resp = self
            .send_request(storage_operation_request::Operation::ReadLink(
                ReadLinkRequest {
                    path: path.to_string(),
                },
            ))
            .await?;

        match extract_success(resp)? {
            storage_operation_success::Data::ReadLink(r) => Ok(r.target),
            other => Err(StorageError::Internal(format!(
                "unexpected response type for readlink: {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }

    async fn stat_fs(&self, _workspace_id: &str) -> StorageResult<FsStats> {
        let resp = self
            .send_request(storage_operation_request::Operation::StatFs(
                StatFsRequest {},
            ))
            .await?;

        match extract_success(resp)? {
            storage_operation_success::Data::StatFs(s) => Ok(FsStats {
                blocks: s.blocks,
                bfree: s.bfree,
                bavail: s.bavail,
                files: s.files,
                ffree: s.ffree,
                bsize: s.bsize,
                namelen: s.namelen,
                frsize: s.frsize,
            }),
            other => Err(StorageError::Internal(format!(
                "unexpected response type for stat_fs: {:?}",
                std::mem::discriminant(&other)
            ))),
        }
    }
}
