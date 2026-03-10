# Client 本地目录远程共享 — 技术实现文档

> 本文档基于 [remote-storage.md](remote-storage.md) 设计文档，给出完整的实现规格。所有文件路径、结构体定义、函数签名均基于当前代码库实际状态。

---

## 目录

1. [实现阶段划分](#1-实现阶段划分)
2. [Phase 1: 基础设施 — StorageRouter + DB 迁移 + Proto](#2-phase-1-基础设施)
3. [Phase 2: RemoteStorageBackend + ClientStorageService（gRPC 反向流）](#3-phase-2-grpc-反向流)
4. [Phase 3: Server 端 FUSE 挂载](#4-phase-3-server-端-fuse-挂载)
5. [Phase 4: Go SDK — StorageProvider](#5-phase-4-go-sdk)
6. [Phase 5: NFS 通道支持](#6-phase-5-nfs-通道)
7. [Phase 6: 可靠性 — 断线恢复 / Server 重启 / 健康监控](#7-phase-6-可靠性)
8. [Phase 7: 可观测性](#8-phase-7-可观测性)
9. [配置变更汇总](#9-配置变更汇总)
10. [测试计划](#10-测试计划)

---

## 1. 实现阶段划分

```
Phase 1 ─── 基础设施（StorageRouter / DB / Proto）
  │
Phase 2 ─── RemoteStorageBackend + ClientStorageService（核心反向流）
  │
Phase 3 ─── Server 端 FUSE 挂载（复用 fuse-client 逻辑）
  │
Phase 4 ─── Go SDK StorageProvider（Client 端实现）
  │           → Phase 1-4 完成后可端到端跑通 gRPC 通道
  │
Phase 5 ─── NFS 通道（可选通道 + 通道切换）
  │
Phase 6 ─── 可靠性（断线 / 重启 / 健康监控 / Sandbox 自动恢复）
  │
Phase 7 ─── 可观测性（Metrics / Logging / Alerting）
```

每个 Phase 可独立提交 PR，Phase 4 完成后即可端到端验证 gRPC 通道。

---

## 2. Phase 1: 基础设施

### 2.1 数据库迁移

**新文件**: `server/migrations/20260310000000_add_remote_storage.sql`

```sql
-- Add storage_type and storage_config to workspaces table
ALTER TABLE workspaces ADD COLUMN storage_type TEXT NOT NULL DEFAULT 'managed';
ALTER TABLE workspaces ADD COLUMN storage_config TEXT NOT NULL DEFAULT '{}';
```

### 2.2 Domain 模型变更

**修改文件**: `server/src/domain/workspace.rs`

```rust
// 新增存储类型枚举
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
    Managed,
    Remote,
}

impl StorageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageType::Managed => "managed",
            StorageType::Remote => "remote",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "managed" => Ok(StorageType::Managed),
            "remote" => Ok(StorageType::Remote),
            _ => Err(format!("unknown storage type: {}", s)),
        }
    }
}

// 存储配置（对应 DB 中的 storage_config JSON）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStorageConfig {
    /// Schema 版本号
    pub v: u32,
    /// 当前传输通道
    pub transport: RemoteTransport,
    /// NFS URL（transport=nfs 时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfs_url: Option<String>,
    /// 通道切换目标（切换进行中才有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switching_to: Option<RemoteTransport>,
    /// 切换阶段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_phase: Option<SwitchPhase>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteTransport {
    Grpc,
    Nfs,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchPhase {
    Pending,
    Mounted,
}

impl Default for RemoteStorageConfig {
    fn default() -> Self {
        Self {
            v: 1,
            transport: RemoteTransport::Grpc,
            nfs_url: None,
            switching_to: None,
            switch_phase: None,
        }
    }
}

// Workspace 结构体新增字段
pub struct Workspace {
    pub id: String,
    pub name: Option<String>,
    pub nfs_url: Option<String>,
    pub storage_type: StorageType,           // 新增
    pub storage_config: RemoteStorageConfig,  // 新增
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// CreateWorkspaceParams 新增字段
pub struct CreateWorkspaceParams {
    pub name: Option<String>,
    pub storage_type: Option<StorageType>,    // 新增，默认 Managed
    pub metadata: Option<HashMap<String, String>>,
}
```

### 2.3 WorkspaceRepository 变更

**修改文件**: `server/src/infra/workspace_repository.rs`

```rust
// WorkspaceRow 新增字段
struct WorkspaceRow {
    id: String,
    name: Option<String>,
    nfs_url: Option<String>,
    storage_type: String,       // 新增
    storage_config: String,     // 新增
    metadata: String,
    created_at: String,
    updated_at: String,
}

// TryFrom 实现中增加字段解析
impl TryFrom<WorkspaceRow> for Workspace {
    fn try_from(row: WorkspaceRow) -> Result<Self> {
        // ... 现有字段解析 ...
        let storage_type = StorageType::from_str(&row.storage_type)
            .map_err(|e| Error::Internal(e))?;
        let storage_config: RemoteStorageConfig = serde_json::from_str(&row.storage_config)
            .unwrap_or_default();
        // ...
    }
}

// create 方法增加 storage_type 和 storage_config
pub async fn create(&self, params: CreateWorkspaceParams) -> Result<Workspace> {
    let storage_type = params.storage_type.unwrap_or(StorageType::Managed);
    let storage_config = if storage_type == StorageType::Remote {
        serde_json::to_string(&RemoteStorageConfig::default()).unwrap()
    } else {
        "{}".to_string()
    };

    sqlx::query(
        r#"INSERT INTO workspaces
           (id, name, storage_type, storage_config, metadata, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(&params.name)
    .bind(storage_type.as_str())
    .bind(&storage_config)
    .bind(&metadata)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&self.pool)
    .await?;
    // ...
}

// 新增：更新 storage_config
pub async fn update_storage_config(
    &self,
    id: &str,
    config: &RemoteStorageConfig,
) -> Result<()> {
    let config_json = serde_json::to_string(config)
        .map_err(|e| Error::Internal(e.to_string()))?;
    let now = Utc::now();
    let result = sqlx::query(
        "UPDATE workspaces SET storage_config = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&config_json)
    .bind(now.to_rfc3339())
    .bind(id)
    .execute(&self.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(Error::WorkspaceNotFound(id.to_string()));
    }
    Ok(())
}

// 新增：列出所有 remote workspace
pub async fn list_remote(&self) -> Result<Vec<Workspace>> {
    let rows: Vec<WorkspaceRow> = sqlx::query_as(
        r#"SELECT id, name, nfs_url, storage_type, storage_config, metadata, created_at, updated_at
           FROM workspaces WHERE storage_type = 'remote'"#,
    )
    .fetch_all(&self.pool)
    .await?;
    rows.into_iter().map(|r| r.try_into()).collect()
}

// 所有 SELECT 查询增加 storage_type, storage_config 字段
```

### 2.4 StorageRouter

**新文件**: `server/src/infra/storage/router.rs`

`StorageRouter` 自身实现 `StorageBackend` trait，作为上层的统一入口。

```rust
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::RwLock;
use crate::infra::storage::{StorageBackend, StorageResult, StorageError};

/// Remote workspace 的连接信息
pub struct RemoteConnectionInfo {
    pub is_remote: bool,
    pub transport: RemoteTransport,
    pub connected: bool,
}

/// Per-workspace 存储路由器
pub struct StorageRouter {
    /// 全局默认后端（managed workspace 使用）
    default_backend: Arc<dyn StorageBackend>,
    /// Per-workspace 覆盖后端
    overrides: DashMap<String, Arc<dyn StorageBackend>>,
    /// Per-workspace 读写锁（用于通道切换排空）
    locks: DashMap<String, Arc<RwLock<()>>>,
    /// Remote workspace 连接信息（用于直连优化接口预留）
    remote_info: DashMap<String, RemoteConnectionInfo>,
}

impl StorageRouter {
    pub fn new(default_backend: Arc<dyn StorageBackend>) -> Self {
        Self {
            default_backend,
            overrides: DashMap::new(),
            locks: DashMap::new(),
            remote_info: DashMap::new(),
        }
    }

    /// 注册 per-workspace 后端
    pub fn register(&self, workspace_id: &str, backend: Arc<dyn StorageBackend>) {
        self.overrides.insert(workspace_id.to_string(), backend);
        self.locks.insert(workspace_id.to_string(), Arc::new(RwLock::new(())));
    }

    /// 注销 per-workspace 后端
    pub fn unregister(&self, workspace_id: &str) {
        self.overrides.remove(workspace_id);
        self.locks.remove(workspace_id);
        self.remote_info.remove(workspace_id);
    }

    /// 获取 workspace 的读锁（正常文件操作使用）
    async fn read_lock(&self, workspace_id: &str) -> Option<tokio::sync::OwnedRwLockReadGuard<()>> {
        self.locks.get(workspace_id)
            .map(|lock| lock.value().clone())
            .map(|lock| lock.read_owned())
            // 注意：实际实现需要 await，此处为示意
    }

    /// 获取 workspace 的写锁（通道切换使用）
    pub async fn write_lock(
        &self,
        workspace_id: &str,
        timeout: std::time::Duration,
    ) -> Result<tokio::sync::OwnedRwLockWriteGuard<()>, StorageError> {
        let lock = self.locks.get(workspace_id)
            .ok_or_else(|| StorageError::NotFound(workspace_id.to_string()))?
            .value().clone();
        tokio::time::timeout(timeout, lock.write_owned())
            .await
            .map_err(|_| StorageError::Internal("channel switch timeout".to_string()))
    }

    /// 替换 per-workspace 后端（持有写锁时调用）
    pub fn replace_backend(&self, workspace_id: &str, backend: Arc<dyn StorageBackend>) {
        self.overrides.insert(workspace_id.to_string(), backend);
    }

    /// 直连优化接口预留
    pub fn get_remote_connection_info(&self, workspace_id: &str) -> Option<RemoteConnectionInfo> {
        self.remote_info.get(workspace_id).map(|r| RemoteConnectionInfo {
            is_remote: r.is_remote,
            transport: r.transport.clone(),
            connected: r.connected,
        })
    }

    /// 路由：获取 workspace_id 对应的 StorageBackend
    fn resolve(&self, workspace_id: &str) -> Arc<dyn StorageBackend> {
        self.overrides
            .get(workspace_id)
            .map(|r| r.value().clone())
            .unwrap_or_else(|| self.default_backend.clone())
    }
}

#[async_trait]
impl StorageBackend for StorageRouter {
    // 每个 trait 方法的实现模式一致：
    // 1. resolve(workspace_id) 获取后端
    // 2. 如果是 remote workspace，获取读锁
    // 3. 委托调用后端方法
    // 4. 释放读锁

    async fn stat(&self, workspace_id: &str, path: &str) -> StorageResult<FileStat> {
        let backend = self.resolve(workspace_id);
        // 对于 remote workspace 需要获取读锁
        if let Some(lock) = self.locks.get(workspace_id) {
            let _guard = lock.value().read().await;
            backend.stat(workspace_id, path).await
        } else {
            backend.stat(workspace_id, path).await
        }
    }

    // ... 其他所有 StorageBackend 方法同理委托 ...

    // create_workspace_root: remote workspace 不创建本地目录
    async fn create_workspace_root(&self, workspace_id: &str) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        backend.create_workspace_root(workspace_id).await
    }

    // delete_workspace_root: remote workspace 需要先 umount
    async fn delete_workspace_root(&self, workspace_id: &str) -> StorageResult<()> {
        let backend = self.resolve(workspace_id);
        backend.delete_workspace_root(workspace_id).await
    }
}
```

**注册到 `server/src/infra/storage/mod.rs`**:

```rust
pub mod lease;
pub mod local;
pub mod router;     // 新增
pub mod s3fs_mount;
```

### 2.5 主入口接线变更

**修改文件**: `server/src/main.rs`

核心变更：将全局 `Arc<dyn StorageBackend>` 替换为 `Arc<StorageRouter>`。

```rust
use infra::storage::router::StorageRouter;

async fn main() -> anyhow::Result<()> {
    // ... 现有初始化 ...

    let (storage, s3_mount_manager) = init_storage(&config.storage).await?;

    // 用 StorageRouter 包装默认后端
    let storage_router = Arc::new(StorageRouter::new(storage.clone()));

    // NfsManager 使用 StorageRouter（它实现了 StorageBackend trait）
    let nfs_manager = Arc::new(NfsManager::new(
        nfs_mode,
        config.nfs_port,
        config.get_nfs_host().to_string(),
        storage_router.clone() as Arc<dyn StorageBackend>,  // 关键变更
    ));

    // WorkspaceService 使用 StorageRouter
    let workspace_service = Arc::new(WorkspaceService::new(
        workspace_repository.clone(),
        nfs_manager.clone(),
        storage_router.clone() as Arc<dyn StorageBackend>,  // 关键变更
        lease_manager as Arc<dyn WorkspaceLeaseManager>,
        holder_id,
    ));

    // FileSystemServiceImpl 使用 StorageRouter
    let fs_service = FileSystemServiceImpl::new(
        storage_router.clone() as Arc<dyn StorageBackend>,  // 关键变更
    );

    // ... 其余不变 ...
}
```

### 2.6 Proto 定义 — ClientStorageService

**新文件**: `proto/workspace/v1/client_storage.proto`

```protobuf
syntax = "proto3";

package workspace.v1;

option go_package = "github.com/OpenElevo/ElevoSandbox/proto/workspace/v1";

import "google/protobuf/timestamp.proto";

// ClientStorageService — Client 反向流连接服务
// Client 主动连接 Server，Server 通过流向 Client 发送文件操作请求
service ClientStorageService {
  // 控制流：双向流，承载元数据操作和心跳
  rpc Connect(stream ClientMessage) returns (stream ServerStorageMessage);

  // 数据流：Client 读取本地文件并流式发送给 Server（client-streaming）
  // 数据方向：Client → Server（文件内容从 Client 本地磁盘传输到 Server）
  rpc ReadFileStream(stream ReadFileStreamRequest) returns (ReadFileStreamResponse);

  // 数据流：Server 流式发送待写入数据给 Client（server-streaming）
  // 数据方向：Server → Client（文件内容从 Server 传输到 Client 本地磁盘写入）
  rpc WriteFileStream(WriteFileStreamRequest) returns (stream WriteFileStreamResponse);
}

// ============================================================
// 控制流消息
// ============================================================

// Client → Server 消息
message ClientMessage {
  oneof message {
    // 握手（首条消息）
    StorageHandshake handshake = 1;
    // 操作响应
    StorageOperationResponse operation_response = 2;
    // 文件变更通知（Client 主动推送）
    FileChangedNotification file_changed = 3;
    // 心跳响应
    StoragePong pong = 4;
    // 数据流建立失败通知
    DataTransferFailed data_transfer_failed = 5;
  }
}

// Server → Client 消息
message ServerStorageMessage {
  oneof message {
    // 握手确认
    StorageHandshakeAck handshake_ack = 1;
    // 文件操作请求
    StorageOperationRequest operation_request = 2;
    // 心跳
    StoragePing ping = 3;
    // 通知 Client 发起数据流 RPC
    StartDataTransfer start_data_transfer = 4;
  }
}

// ============================================================
// 握手
// ============================================================

message StorageHandshake {
  string workspace_id = 1;
  // Client 认证 token（复用现有 token 机制）
  string token = 2;
}

message StorageHandshakeAck {
  bool success = 1;
  optional string error = 2;
}

// ============================================================
// 心跳
// ============================================================

message StoragePing {
  uint64 timestamp = 1;
}

message StoragePong {
  uint64 timestamp = 1;
}

// ============================================================
// 文件操作请求（Server → Client）
// ============================================================

message StorageOperationRequest {
  // 请求关联 ID
  string correlation_id = 1;
  // 操作类型
  oneof operation {
    StatRequest stat = 10;
    ListDirRequest list_dir = 11;
    ExistsRequest exists = 12;
    ReadFileRangeRequest read_file_range = 13;
    WriteFileAtRequest write_file_at = 14;
    CreateFileRequest create_file = 15;
    MkdirRequest mkdir = 16;
    RemoveFileRequest remove_file = 17;
    RemoveDirRequest remove_dir = 18;
    RenameRequest rename = 19;
    CopyRequest copy = 20;
    SetFileSizeRequest set_file_size = 21;
    SetPermissionsRequest set_permissions = 22;
    SetTimesRequest set_times = 23;
    SymlinkRequest symlink = 24;
    ReadLinkRequest read_link = 25;
    StatFsRequest stat_fs = 26;
  }
}

// --- 各操作的请求消息 ---
// 注意：所有 path 都是 workspace 内的相对路径

message StatRequest {
  string path = 1;
}

message ListDirRequest {
  string path = 1;
}

message ExistsRequest {
  string path = 1;
}

message ReadFileRangeRequest {
  string path = 1;
  uint64 offset = 2;
  uint32 length = 3;
}

message WriteFileAtRequest {
  string path = 1;
  uint64 offset = 2;
  bytes data = 3;
}

message CreateFileRequest {
  string path = 1;
  bool exclusive = 2;
}

message MkdirRequest {
  string path = 1;
  bool recursive = 2;
}

message RemoveFileRequest {
  string path = 1;
}

message RemoveDirRequest {
  string path = 1;
  bool recursive = 2;
}

message RenameRequest {
  string src = 1;
  string dst = 2;
  // 0=normal, 1=noreplace, 2=exchange
  uint32 flags = 3;
}

message CopyRequest {
  string src = 1;
  string dst = 2;
}

message SetFileSizeRequest {
  string path = 1;
  uint64 size = 2;
}

message SetPermissionsRequest {
  string path = 1;
  uint32 mode = 2;
}

message SetTimesRequest {
  string path = 1;
  optional google.protobuf.Timestamp atime = 2;
  optional google.protobuf.Timestamp mtime = 3;
}

message SymlinkRequest {
  string link_path = 1;
  string target = 2;
}

message ReadLinkRequest {
  string path = 1;
}

message StatFsRequest {}

// ============================================================
// 文件操作响应（Client → Server）
// ============================================================

message StorageOperationResponse {
  // 请求关联 ID（与请求对应）
  string correlation_id = 1;
  oneof result {
    // 成功结果
    StorageOperationSuccess success = 2;
    // 错误
    StorageOperationError error = 3;
  }
}

message StorageOperationSuccess {
  oneof data {
    FileStatData stat = 1;
    ListDirData list_dir = 2;  // 分页模式时单个批次
    ExistsData exists = 3;
    ReadData read_data = 4;
    WriteData write_data = 5;
    ReadLinkData read_link = 6;
    StatFsData stat_fs = 7;
    Empty empty = 8;  // 无返回值的操作（mkdir, remove, rename, symlink 等）
  }
  // ListDir 分页标记
  bool is_last = 10;
}

// 与 server/src/infra/storage/mod.rs 中的 FileStat 完全对齐。
// file_type 枚举值映射：0=File, 1=Directory, 2=Symlink，
// 与 Rust 端 FileType enum 的 discriminant 一致。
// 注意：外部 HTTP API 的 FileInfo.type 是 string ("file"/"directory")，
// 不支持 symlink，但这里是内部 gRPC 协议，需要完整的 POSIX 元数据。
message FileStatData {
  string name = 1;
  string path = 2;
  // 0=File, 1=Directory, 2=Symlink — 与 Rust FileType enum discriminant 对齐
  uint32 file_type = 3;
  uint64 size = 4;
  uint32 mode = 5;       // Unix permission mode (e.g., 0o644)
  uint32 uid = 6;
  uint32 gid = 7;
  optional google.protobuf.Timestamp modified_at = 8;
  optional google.protobuf.Timestamp accessed_at = 9;
  optional google.protobuf.Timestamp created_at = 10;
}

message ListDirData {
  repeated FileStatData entries = 1;
}

message ExistsData {
  bool exists = 1;
}

message ReadData {
  bytes data = 1;
}

message WriteData {
  uint64 bytes_written = 1;
}

message ReadLinkData {
  string target = 1;
}

// 与 server/src/infra/storage/mod.rs 中的 FsStats 完全对齐，
// 字段名称和类型一一对应，可直接映射转换。
message StatFsData {
  uint64 blocks = 1;     // statvfs.f_blocks
  uint64 bfree = 2;      // statvfs.f_bfree
  uint64 bavail = 3;     // statvfs.f_bavail
  uint64 files = 4;      // statvfs.f_files
  uint64 ffree = 5;      // statvfs.f_ffree
  uint32 bsize = 6;      // statvfs.f_bsize
  uint32 namelen = 7;    // statvfs.f_namemax
  uint32 frsize = 8;     // statvfs.f_frsize
}

message Empty {}

// 错误码枚举
enum StorageErrorCode {
  STORAGE_ERROR_CODE_UNSPECIFIED = 0;
  STORAGE_ERROR_CODE_NOT_FOUND = 1;
  STORAGE_ERROR_CODE_ALREADY_EXISTS = 2;
  STORAGE_ERROR_CODE_IS_A_DIRECTORY = 3;
  STORAGE_ERROR_CODE_NOT_A_DIRECTORY = 4;
  STORAGE_ERROR_CODE_NOT_A_FILE = 5;
  STORAGE_ERROR_CODE_DIRECTORY_NOT_EMPTY = 6;
  STORAGE_ERROR_CODE_PERMISSION_DENIED = 7;
  STORAGE_ERROR_CODE_PATH_TRAVERSAL_DENIED = 8;
  STORAGE_ERROR_CODE_NOT_SUPPORTED = 9;
  STORAGE_ERROR_CODE_IO_ERROR = 10;
}

message StorageOperationError {
  StorageErrorCode code = 1;
  string message = 2;
}

// ============================================================
// 数据流关联
// ============================================================

// Server 通知 Client 发起独立的数据流 RPC
message StartDataTransfer {
  string transfer_id = 1;
  DataTransferOperation operation = 2;
  string path = 3;
  // WRITE_FILE 时携带文件大小（如已知）
  optional uint64 file_size = 4;
  // READ_FILE range 模式：指定读取的偏移和长度（均不为 None 时为 range 读）
  optional uint64 offset = 5;
  optional uint64 length = 6;
}

enum DataTransferOperation {
  DATA_TRANSFER_OPERATION_UNSPECIFIED = 0;
  DATA_TRANSFER_OPERATION_READ_FILE = 1;
  DATA_TRANSFER_OPERATION_WRITE_FILE = 2;
}

// Client 通知 Server 数据流建立失败
message DataTransferFailed {
  string transfer_id = 1;
  string reason = 2;
}

// ============================================================
// 数据流 RPC 消息
// ============================================================

// ReadFileStream: Client 读取本地文件并流式发送给 Server（client-streaming）
// Client 发起此 RPC 后，将文件内容分块发送，Server 收集完整数据。
message ReadFileStreamRequest {
  oneof payload {
    // 首条消息：header，携带 transfer_id
    ReadFileStreamHeader header = 1;
    // 后续消息：文件数据分块
    bytes data = 2;
  }
}

message ReadFileStreamHeader {
  string transfer_id = 1;
  string workspace_id = 2;  // 用于 Server 端直接定位 backend，避免遍历
}

// Server 收集完所有 chunks 后返回确认
message ReadFileStreamResponse {
  uint64 bytes_read = 1;
}

// WriteFileStream: Server 流式发送待写入数据给 Client（server-streaming）
// Client 发起此 RPC 携带 transfer_id 和 workspace_id，Server 流式返回待写入的文件内容。
// Client 在本地执行写入。
message WriteFileStreamRequest {
  string transfer_id = 1;
  string workspace_id = 2;  // 用于 Server 端直接定位 backend，避免遍历
}

message WriteFileStreamResponse {
  oneof payload {
    // 文件数据分块
    bytes data = 1;
    // 传输完成标记
    WriteFileStreamDone done = 2;
  }
}

message WriteFileStreamDone {
  uint64 total_bytes = 1;
}

// ============================================================
// 文件变更通知（Client → Server）
// ============================================================

message FileChangedNotification {
  repeated FileChangeEvent events = 1;
  // inotify 降级模式下设为 true，Server 端执行全量缓存清除
  bool full_purge = 2;
}

message FileChangeEvent {
  string path = 1;
  FileChangeType event_type = 2;
  // RENAMED 时的新路径
  optional string new_path = 3;
}

enum FileChangeType {
  FILE_CHANGE_TYPE_UNSPECIFIED = 0;
  FILE_CHANGE_TYPE_CREATED = 1;
  FILE_CHANGE_TYPE_MODIFIED = 2;
  FILE_CHANGE_TYPE_DELETED = 3;
  FILE_CHANGE_TYPE_RENAMED = 4;
  FILE_CHANGE_TYPE_ATTR_CHANGED = 5;
}
```

### 2.7 Proto 编译配置

**修改文件**: `workspace-proto/build.rs`

Proto 编译在 `workspace-proto` crate 中统一管理（而非 `server/build.rs`）。当前 build.rs 分两步编译：第一步编译除 agent.proto 外的所有 proto（同时生成 server 和 client 代码），第二步单独编译 agent.proto（仅 server，因 Connect RPC 命名冲突）。

`client_storage.proto` 与 agent.proto 类似，包含 `Connect` RPC，因此也需要单独编译（仅 server 端，不生成 client——Go SDK 自行通过 protoc-gen-go 编译）。

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = "src/gen";
    std::fs::create_dir_all(out_dir)?;

    // 第一步：编译除 agent.proto 和 client_storage.proto 外的所有 proto
    // （同时生成 server 和 client 代码）
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(out_dir)
        .file_descriptor_set_path(format!("{out_dir}/main_descriptor.bin"))
        .compile_protos(
            &[
                "../proto/workspace/v1/sandbox.proto",
                "../proto/workspace/v1/process.proto",
                "../proto/workspace/v1/pty.proto",
                "../proto/workspace/v1/workspace.proto",
                "../proto/workspace/v1/filesystem.proto",
            ],
            &["../proto"],
        )?;
    std::fs::rename(
        format!("{out_dir}/workspace.v1.rs"),
        format!("{out_dir}/main_services.rs"),
    )?;

    // 第二步：编译 agent.proto（仅 server，因 Connect RPC 命名冲突）
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .out_dir(out_dir)
        .file_descriptor_set_path(format!("{out_dir}/agent_descriptor.bin"))
        .compile_protos(&["../proto/workspace/v1/agent.proto"], &["../proto"])?;
    std::fs::rename(
        format!("{out_dir}/workspace.v1.rs"),
        format!("{out_dir}/agent_services.rs"),
    )?;

    // 第三步（新增）：编译 client_storage.proto（仅 server，同样因 Connect RPC 冲突）
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .out_dir(out_dir)
        .file_descriptor_set_path(format!("{out_dir}/client_storage_descriptor.bin"))
        .compile_protos(
            &["../proto/workspace/v1/client_storage.proto"],
            &["../proto"],
        )?;
    std::fs::rename(
        format!("{out_dir}/workspace.v1.rs"),
        format!("{out_dir}/client_storage_services.rs"),
    )?;

    // 生成合并模块文件
    let mut combined = std::fs::File::create(format!("{out_dir}/workspace.v1.rs"))?;
    writeln!(combined, "// This file is @generated by build.rs")?;
    writeln!(combined)?;
    writeln!(combined, "// Main services (with client support)")?;
    writeln!(combined, "include!(\"main_services.rs\");")?;
    writeln!(combined)?;
    writeln!(combined, "// Agent service (server only)")?;
    writeln!(combined, "include!(\"agent_services.rs\");")?;
    writeln!(combined)?;
    writeln!(combined, "// Client storage service (server only)")?;
    writeln!(combined, "include!(\"client_storage_services.rs\");")?;

    println!("cargo:rerun-if-changed=../proto");
    Ok(())
}
```

Server 通过 `workspace-proto` crate 依赖自动获得生成的类型和 service trait。无需修改 `server/` 下的 build.rs 或 proto 引用文件。

### 2.8 Workspace API Proto 扩展

**修改文件**: `proto/workspace/v1/workspace.proto`

```protobuf
// CreateWorkspaceRequest 新增字段
message CreateWorkspaceRequest {
  optional string name = 1;
  map<string, string> metadata = 2;
  // 存储类型：默认 managed，remote 表示 Client 提供存储
  optional string storage_type = 3;  // 新增
}

// Workspace 消息新增字段
message Workspace {
  string id = 1;
  optional string name = 2;
  optional string nfs_url = 3;
  map<string, string> metadata = 4;
  google.protobuf.Timestamp created_at = 5;
  google.protobuf.Timestamp updated_at = 6;
  string storage_type = 7;      // 新增
  string storage_config = 8;    // 新增 (JSON 字符串)
}

// 新增 RPC
service WorkspaceService {
  // ... 现有 RPC ...

  // 注册 NFS 通道（将 remote workspace 从 gRPC 切换到 NFS）
  rpc RegisterNfsTransport(RegisterNfsTransportRequest) returns (RegisterNfsTransportResponse);

  // 注销 NFS 通道（切回 gRPC）
  rpc UnregisterNfsTransport(UnregisterNfsTransportRequest) returns (UnregisterNfsTransportResponse);
}

message RegisterNfsTransportRequest {
  string workspace_id = 1;
  string nfs_url = 2;
}

message RegisterNfsTransportResponse {
  Workspace workspace = 1;
}

message UnregisterNfsTransportRequest {
  string workspace_id = 1;
}

message UnregisterNfsTransportResponse {
  Workspace workspace = 1;
}
```

### 2.9 WorkspaceService 创建逻辑变更

**修改文件**: `server/src/service/workspace.rs`

Remote workspace 创建时与 managed workspace 的区别：
- **不创建本地目录**（由 FUSE 挂载代替，Client 连接后创建）
- **不导出 NFS**（FUSE 挂载后自动可用）
- **配额检查**（remote workspace 有独立的数量上限）

```rust
impl WorkspaceService {
    pub async fn create(&self, params: CreateWorkspaceParams) -> Result<Workspace> {
        info!("Creating workspace with name: {:?}", params.name);

        let storage_type = params.storage_type.clone().unwrap_or(StorageType::Managed);

        // Remote workspace 配额检查
        if storage_type == StorageType::Remote {
            let remote_count = self.repository.count_remote().await?;
            if remote_count >= self.config.max_remote_workspaces {
                return Err(Error::ResourceExhausted(format!(
                    "max remote workspaces limit ({}) reached",
                    self.config.max_remote_workspaces
                )));
            }
        }

        // 创建 DB 记录
        let workspace = self.repository.create(params).await?;
        let workspace_id = workspace.id.clone();

        // 获取 lease
        if let Err(e) = self.lease_manager.acquire(&workspace_id, &self.holder_id).await {
            error!("Failed to acquire lease for workspace {}: {}", workspace_id, e);
            let _ = self.repository.delete(&workspace_id).await;
            return Err(Error::Internal(format!("Failed to acquire workspace lease: {}", e)));
        }
        {
            let mut leases = self.active_leases.write().await;
            leases.insert(workspace_id.clone());
        }

        // Remote workspace 跳过本地目录创建和 NFS 导出
        if storage_type == StorageType::Remote {
            info!("Created remote workspace {} (waiting for client connection)", workspace_id);
            return Ok(workspace);
        }

        // === 以下仅 managed workspace 执行 ===

        // 创建本地目录
        if let Err(e) = self.storage.create_workspace_root(&workspace_id).await {
            error!("Failed to create workspace directory: {}", e);
            let _ = self.lease_manager.release(&workspace_id, &self.holder_id).await;
            self.remove_from_active_leases(&workspace_id).await;
            let _ = self.repository.delete(&workspace_id).await;
            return Err(Error::Internal(format!("Failed to create workspace directory: {}", e)));
        }

        // 导出 NFS
        match self.nfs_manager.export(&workspace_id).await {
            Ok(nfs_url) => {
                info!("NFS export created for workspace {}: {}", workspace_id, nfs_url);
                // ... 更新 nfs_url ...
            }
            Err(e) => {
                warn!("NFS export failed for workspace {}: {}", workspace_id, e);
                // NFS 失败不阻塞创建（非致命）
            }
        }

        Ok(workspace)
    }
}
```

**新增**: `WorkspaceRepository::count_remote` 方法：
```rust
pub async fn count_remote(&self) -> Result<usize> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM workspaces WHERE storage_type = 'remote'"
    )
    .fetch_one(&self.pool)
    .await?;
    Ok(row.0 as usize)
}
```

---

## 3. Phase 2: gRPC 反向流

### 3.1 RemoteStorageBackend

**新文件**: `server/src/infra/storage/remote.rs`

核心结构体，实现 `StorageBackend` trait。内部通过 gRPC 双向流将操作代理到 Client。

```rust
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot, Notify, Semaphore};
use uuid::Uuid;

use crate::infra::storage::{
    FileStat, FileType, FsStats, StorageBackend, StorageError, StorageResult,
};
use crate::proto::{
    ServerStorageMessage, StorageOperationRequest, StorageOperationSuccess,
    StorageOperationError, StorageErrorCode,
    // ... 各操作请求/响应类型 ...
};

/// 连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Pending,
    Connected,
    Disconnected,
}

/// Remote workspace 的 gRPC 反向流存储后端
pub struct RemoteStorageBackend {
    workspace_id: String,

    /// 发送消息到 Client 的 channel
    /// 绑定到当前活跃的 gRPC 流。断线后置为 None，重连后替换。
    stream_tx: tokio::sync::RwLock<Option<mpsc::Sender<ServerStorageMessage>>>,

    /// 等待中的请求：correlation_id → oneshot sender
    /// ListDir 等分页操作使用 PendingRequest::Paged 变体收集多批次
    pending_requests: DashMap<String, PendingRequest>,

    /// 等待中的数据流传输：transfer_id → pending transfer
    /// Read 场景：RPC handler 收集数据后通过 done 通知完成
    /// Write 场景：data 存储待写入内容，RPC handler 取出后流式发送给 Client
    pending_transfers: DashMap<String, PendingTransfer>,

    /// 并发控制信号量（默认 128）
    concurrency_semaphore: Semaphore,

    /// 连接状态
    state: tokio::sync::RwLock<ConnectionState>,

    /// 连接状态变更通知
    state_notify: Notify,

    /// 操作超时（默认 30s）
    operation_timeout: Duration,

    /// 数据流关联超时（默认 10s）
    transfer_timeout: Duration,

    /// 重连后需要全量缓存清除的标记
    needs_full_cache_purge: std::sync::atomic::AtomicBool,
}

/// 控制流 pending 请求的两种模式
pub enum PendingRequest {
    /// 普通请求：单条响应
    Single(oneshot::Sender<Result<StorageOperationSuccess, StorageError>>),
    /// 分页请求（ListDir）：多批次响应，收集到 Vec 后通过 done 通知
    Paged {
        collected: Arc<tokio::sync::Mutex<Vec<FileStatData>>>,
        done: oneshot::Sender<Result<Vec<FileStatData>, StorageError>>,
    },
}

/// 数据流 pending transfer
pub enum PendingTransfer {
    /// Server 从 Client 读取文件：RPC handler 收集 chunks 后通过 done 发送
    Read {
        done: oneshot::Sender<Result<Vec<u8>, StorageError>>,
    },
    /// Server 向 Client 写入文件：data 存储待写入内容，RPC handler 取出后流式发送
    Write {
        data: Arc<Vec<u8>>,
        done: oneshot::Sender<Result<u64, StorageError>>,
    },
}

/// 数据流传输结果（简化的 enum，用于 RPC handler 回调）
pub enum DataTransferResult {
    ReadComplete(Vec<u8>),
    WriteComplete(u64),
    Error(StorageError),
}

impl RemoteStorageBackend {
    pub fn new(workspace_id: String) -> Self {
        Self {
            workspace_id,
            stream_tx: tokio::sync::RwLock::new(None),
            pending_requests: DashMap::new(),
            pending_transfers: DashMap::new(),
            concurrency_semaphore: Semaphore::new(128),
            state: tokio::sync::RwLock::new(ConnectionState::Pending),
            state_notify: Notify::new(),
            operation_timeout: Duration::from_secs(30),
            transfer_timeout: Duration::from_secs(10),
            needs_full_cache_purge: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 绑定 gRPC 流（Client 连接 / 重连时调用）
    ///
    /// is_reconnect 为 true 时触发全量缓存清除（设计文档 3.6 节要求：
    /// Client 重连后 Server 执行全量缓存清除，确保不使用 stale 数据）
    pub async fn bind_stream(
        &self,
        tx: mpsc::Sender<ServerStorageMessage>,
        is_reconnect: bool,
    ) {
        *self.stream_tx.write().await = Some(tx);
        *self.state.write().await = ConnectionState::Connected;
        self.state_notify.notify_waiters();

        // 重连时触发全量缓存清除（由调用方配合 FuseMountManager 执行）
        // 此处仅设置标记，实际清除在 ClientStorageServiceImpl::connect 中执行
        if is_reconnect {
            self.needs_full_cache_purge.store(true, std::sync::atomic::Ordering::Release);
        }
    }

    /// 检查并消费"需要全量缓存清除"标记
    pub fn take_cache_purge_flag(&self) -> bool {
        self.needs_full_cache_purge.swap(false, std::sync::atomic::Ordering::AcqRel)
    }

    /// 解绑 gRPC 流（Client 断线时调用）
    pub async fn unbind_stream(&self) {
        *self.stream_tx.write().await = None;
        *self.state.write().await = ConnectionState::Disconnected;

        // 主动清理所有 pending 请求
        let keys: Vec<String> = self.pending_requests.iter()
            .map(|r| r.key().clone()).collect();
        for key in keys {
            if let Some((_, pending)) = self.pending_requests.remove(&key) {
                let err = StorageError::Io {
                    path: self.workspace_id.clone(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        "connection closed",
                    ),
                };
                match pending {
                    PendingRequest::Single(sender) => {
                        let _ = sender.send(Err(err));
                    }
                    PendingRequest::Paged { done, .. } => {
                        let _ = done.send(Err(err));
                    }
                }
            }
        }

        // 主动清理所有 pending 数据流传输
        let transfer_keys: Vec<String> = self.pending_transfers.iter()
            .map(|r| r.key().clone()).collect();
        for key in transfer_keys {
            if let Some((_, transfer)) = self.pending_transfers.remove(&key) {
                let err = StorageError::Io {
                    path: self.workspace_id.clone(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        "client disconnected",
                    ),
                };
                match transfer {
                    PendingTransfer::Read { done } => {
                        let _ = done.send(Err(err));
                    }
                    PendingTransfer::Write { done, .. } => {
                        let _ = done.send(Err(err));
                    }
                }
            }
        }
    }

    /// 处理 Client 发来的操作响应（从控制流接收消息的 handler 调用）
    ///
    /// 普通操作：单条响应 → 直接通过 oneshot 发送
    /// ListDir 分页：多条响应使用相同 correlation_id，最后一条标记 is_last=true
    pub fn handle_operation_response(&self, response: proto::StorageOperationResponse) {
        let correlation_id = &response.correlation_id;

        // 先检查是否是分页请求的中间批次（不移除 entry）
        if let Some(pending) = self.pending_requests.get(correlation_id) {
            if let PendingRequest::Paged { ref collected, .. } = *pending {
                // 解析响应中的 ListDir 数据
                match &response.result {
                    Some(storage_operation_response::Result::Success(success)) => {
                        if let Some(storage_operation_success::Data::ListDir(data)) = &success.data {
                            let mut vec = collected.blocking_lock();
                            vec.extend(data.entries.iter().cloned());
                        }
                        if success.is_last {
                            // 最后一批：移除 entry 并通过 done 发送完整结果
                            drop(pending);
                            if let Some((_, PendingRequest::Paged { collected, done })) =
                                self.pending_requests.remove(correlation_id)
                            {
                                let vec = Arc::try_unwrap(collected)
                                    .unwrap_or_else(|arc| arc.blocking_lock().clone())
                                    .into_inner();
                                let _ = done.send(Ok(vec));
                            }
                        }
                        // 非最后一批：不移除，等待后续批次
                        return;
                    }
                    Some(storage_operation_response::Result::Error(err)) => {
                        drop(pending);
                        if let Some((_, PendingRequest::Paged { done, .. })) =
                            self.pending_requests.remove(correlation_id)
                        {
                            let _ = done.send(Err(convert_error_code(err.clone())));
                        }
                        return;
                    }
                    None => {}
                }
                return;
            }
        }

        // 普通请求：单条响应
        if let Some((_, PendingRequest::Single(sender))) = self.pending_requests.remove(correlation_id) {
            match response.result {
                Some(storage_operation_response::Result::Success(success)) => {
                    let _ = sender.send(Ok(success));
                }
                Some(storage_operation_response::Result::Error(err)) => {
                    let _ = sender.send(Err(convert_error_code(err)));
                }
                None => {
                    let _ = sender.send(Err(StorageError::Internal("empty response".to_string())));
                }
            }
        }
        }
    }

    /// 完成数据流传输（ReadFileStream/WriteFileStream RPC handler 调用）
    ///
    /// 通过 transfer_id 找到对应的 pending transfer，将结果发送给等待方。
    /// 返回 false 表示 transfer_id 不存在（已超时清理或无效）。
    pub fn complete_data_transfer(
        &self,
        transfer_id: &str,
        result: DataTransferResult,
    ) -> bool {
        if let Some((_, transfer)) = self.pending_transfers.remove(transfer_id) {
            match (transfer, result) {
                (PendingTransfer::Read { done }, DataTransferResult::ReadComplete(data)) => {
                    let _ = done.send(Ok(data));
                }
                (PendingTransfer::Write { done, .. }, DataTransferResult::WriteComplete(n)) => {
                    let _ = done.send(Ok(n));
                }
                (PendingTransfer::Read { done }, DataTransferResult::Error(e)) => {
                    let _ = done.send(Err(e));
                }
                (PendingTransfer::Write { done, .. }, DataTransferResult::Error(e)) => {
                    let _ = done.send(Err(e));
                }
                _ => {
                    tracing::error!("transfer type mismatch for {}", transfer_id);
                    return false;
                }
            }
            true
        } else {
            false
        }
    }

    /// 获取 Write 类型 pending transfer 中的数据（WriteFileStream RPC handler 调用）
    ///
    /// 不移除 entry —— 数据发送完毕后由 complete_data_transfer 移除。
    /// 返回 None 表示 transfer_id 不存在或不是 Write 类型。
    pub fn get_write_data(&self, transfer_id: &str) -> Option<Arc<Vec<u8>>> {
        self.pending_transfers.get(transfer_id).and_then(|entry| {
            match entry.value() {
                PendingTransfer::Write { data, .. } => Some(data.clone()),
                _ => None,
            }
        })
    }

    /// 处理 Client 通知的数据流建立失败
    pub fn handle_data_transfer_failed(&self, transfer_id: &str, reason: &str) {
        tracing::warn!(
            "data transfer {} failed: {}", transfer_id, reason
        );
        if let Some((_, transfer)) = self.pending_transfers.remove(transfer_id) {
            let err = StorageError::Io {
                path: "".to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    format!("client failed to establish data stream: {}", reason),
                ),
            };
            match transfer {
                PendingTransfer::Read { done } => { let _ = done.send(Err(err)); }
                PendingTransfer::Write { done, .. } => { let _ = done.send(Err(err)); }
            }
        }
    }

    /// 通过数据流读取大文件（内部方法）
    async fn read_file_via_data_stream(&self, path: &str) -> StorageResult<Vec<u8>> {
        let transfer_id = Uuid::new_v4().to_string();

        // 注册 pending transfer（Read 类型）
        let (tx, rx) = oneshot::channel();
        self.pending_transfers.insert(
            transfer_id.clone(),
            PendingTransfer::Read { done: tx },
        );

        // 通过控制流通知 Client 发起 ReadFileStream RPC
        let stream_tx = self.stream_tx.read().await;
        if let Some(ref sender) = *stream_tx {
            let msg = ServerStorageMessage {
                message: Some(server_storage_message::Message::StartDataTransfer(
                    StartDataTransfer {
                        transfer_id: transfer_id.clone(),
                        operation: DataTransferOperation::ReadFile as i32,
                        path: path.to_string(),
                        file_size: None,
                    }
                )),
            };
            if sender.send(msg).await.is_err() {
                self.pending_transfers.remove(&transfer_id);
                return Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "stream closed",
                    ),
                });
            }
        } else {
            self.pending_transfers.remove(&transfer_id);
            return Err(StorageError::Io {
                path: path.to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "client not connected",
                ),
            });
        }
        drop(stream_tx);

        // 等待数据流完成（超时 = transfer_timeout）
        match tokio::time::timeout(self.transfer_timeout, rx).await {
            Ok(Ok(Ok(data))) => Ok(data),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => {
                self.pending_transfers.remove(&transfer_id);
                Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "transfer channel closed",
                    ),
                })
            }
            Err(_) => {
                self.pending_transfers.remove(&transfer_id);
                Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "data transfer timeout",
                    ),
                })
            }
        }
    }

    /// 通过数据流读取大 range（内部方法）
    ///
    /// 与 read_file_via_data_stream 类似，但 StartDataTransfer 中携带 offset/length，
    /// Client 只读取指定范围的数据。
    async fn read_file_range_via_data_stream(
        &self,
        path: &str,
        offset: u64,
        length: u32,
    ) -> StorageResult<Vec<u8>> {
        // 复用 read_file_via_data_stream 的逻辑，
        // 区别在于 StartDataTransfer 消息中携带 offset 和 length 字段。
        // Client 端根据这些参数执行 Pread 而非完整文件读取。
        let transfer_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending_transfers.insert(
            transfer_id.clone(),
            PendingTransfer::Read { done: tx },
        );

        let stream_tx = self.stream_tx.read().await;
        if let Some(ref sender) = *stream_tx {
            let msg = ServerStorageMessage {
                message: Some(server_storage_message::Message::StartDataTransfer(
                    StartDataTransfer {
                        transfer_id: transfer_id.clone(),
                        operation: DataTransferOperation::ReadFile as i32,
                        path: path.to_string(),
                        file_size: None,
                        // 扩展字段：range 读取参数
                        offset: Some(offset),
                        length: Some(length as u64),
                    }
                )),
            };
            if sender.send(msg).await.is_err() {
                self.pending_transfers.remove(&transfer_id);
                return Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stream closed"),
                });
            }
        } else {
            self.pending_transfers.remove(&transfer_id);
            return Err(StorageError::Io {
                path: path.to_string(),
                source: std::io::Error::new(std::io::ErrorKind::NotConnected, "client not connected"),
            });
        }
        drop(stream_tx);

        match tokio::time::timeout(self.transfer_timeout, rx).await {
            Ok(Ok(Ok(data))) => Ok(data),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => {
                self.pending_transfers.remove(&transfer_id);
                Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(std::io::ErrorKind::BrokenPipe, "transfer channel closed"),
                })
            }
            Err(_) => {
                self.pending_transfers.remove(&transfer_id);
                Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(std::io::ErrorKind::TimedOut, "data transfer timeout"),
                })
            }
        }
    }

    /// 通过数据流写入大文件（内部方法）
    async fn write_file_via_data_stream(&self, path: &str, content: &[u8]) -> StorageResult<()> {
        let transfer_id = Uuid::new_v4().to_string();

        // 注册 pending transfer（Write 类型，data 存储待写入内容）
        let (tx, rx) = oneshot::channel();
        self.pending_transfers.insert(
            transfer_id.clone(),
            PendingTransfer::Write {
                data: Arc::new(content.to_vec()),
                done: tx,
            },
        );

        // 通过控制流通知 Client 发起 WriteFileStream RPC
        let stream_tx = self.stream_tx.read().await;
        if let Some(ref sender) = *stream_tx {
            let msg = ServerStorageMessage {
                message: Some(server_storage_message::Message::StartDataTransfer(
                    StartDataTransfer {
                        transfer_id: transfer_id.clone(),
                        operation: DataTransferOperation::WriteFile as i32,
                        path: path.to_string(),
                        file_size: Some(content.len() as u64),
                    }
                )),
            };
            if sender.send(msg).await.is_err() {
                self.pending_transfers.remove(&transfer_id);
                return Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "stream closed",
                    ),
                });
            }
        } else {
            self.pending_transfers.remove(&transfer_id);
            return Err(StorageError::Io {
                path: path.to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "client not connected",
                ),
            });
        }
        drop(stream_tx);

        // WriteFileStream 由 Client 发起。Client 收到 StartDataTransfer 通知后，
        // 发起 WriteFileStream RPC 携带 transfer_id。Server 端 RPC handler 通过
        // backend.get_write_data(transfer_id) 取出 data（Arc<Vec<u8>>），分块流式发送给 Client。
        // Client 在本地完成写入后，RPC 正常结束，handler 调用 complete_data_transfer。

        match tokio::time::timeout(self.transfer_timeout, rx).await {
            Ok(Ok(Ok(_bytes_written))) => Ok(()),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => {
                self.pending_transfers.remove(&transfer_id);
                Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "transfer channel closed",
                    ),
                })
            }
            Err(_) => {
                self.pending_transfers.remove(&transfer_id);
                Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "data transfer timeout",
                    ),
                })
            }
        }
    }

    /// 发送操作请求并等待响应（内部通用方法）
    ///
    /// `operation` 仅包含操作内容（oneof 字段），不包含 correlation_id。
    /// correlation_id 由本方法内部生成，并组装到完整的 ServerStorageMessage 中发送。
    async fn send_request(
        &self,
        operation: impl Into<storage_operation_request::Operation>,
    ) -> StorageResult<StorageOperationSuccess> {
        // 1. 获取并发信号量
        let _permit = self.concurrency_semaphore.acquire().await
            .map_err(|_| StorageError::Internal("semaphore closed".to_string()))?;

        // 2. 检查连接状态，如果断线则等待重连（带超时）
        self.ensure_connected().await?;

        // 3. 生成 correlation_id，注册 PendingRequest::Single
        let correlation_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending_requests.insert(
            correlation_id.clone(),
            PendingRequest::Single(tx),
        );

        // 4. 发送到 gRPC 流
        self.send_to_stream(&correlation_id, operation).await?;

        // 5. 等待响应（带超时）
        match tokio::time::timeout(self.operation_timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                self.pending_requests.remove(&correlation_id);
                Err(StorageError::Io {
                    path: "".to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "response channel closed",
                    ),
                })
            }
            Err(_) => {
                self.pending_requests.remove(&correlation_id);
                Err(StorageError::Io {
                    path: "".to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "operation timeout",
                    ),
                })
            }
        }
    }

    /// 发送 ListDir 请求并等待分页响应（内部方法）
    ///
    /// ListDir 与普通请求不同：Client 将大目录分批发送（每批最多 200 条），
    /// 多条响应使用相同 correlation_id，最后一批标记 is_last=true。
    /// 使用 PendingRequest::Paged 收集所有批次后返回完整结果。
    async fn send_list_dir_request(
        &self,
        path: &str,
    ) -> StorageResult<Vec<FileStat>> {
        let _permit = self.concurrency_semaphore.acquire().await
            .map_err(|_| StorageError::Internal("semaphore closed".to_string()))?;

        self.ensure_connected().await?;

        let correlation_id = Uuid::new_v4().to_string();
        let collected = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let (tx, rx) = oneshot::channel();
        self.pending_requests.insert(
            correlation_id.clone(),
            PendingRequest::Paged {
                collected: collected.clone(),
                done: tx,
            },
        );

        let operation = storage_operation_request::Operation::ListDir(ListDirRequest {
            path: path.to_string(),
        });
        self.send_to_stream(&correlation_id, operation).await?;

        // 等待所有批次收集完毕（handle_operation_response 中处理分页逻辑）
        match tokio::time::timeout(self.operation_timeout, rx).await {
            Ok(Ok(result)) => {
                result.map(|entries| entries.into_iter().map(convert_file_stat).collect())
            }
            Ok(Err(_)) => {
                self.pending_requests.remove(&correlation_id);
                Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "response channel closed",
                    ),
                })
            }
            Err(_) => {
                self.pending_requests.remove(&correlation_id);
                Err(StorageError::Io {
                    path: path.to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "list_dir timeout",
                    ),
                })
            }
        }
    }

    /// 将操作消息发送到 gRPC 流（内部复用方法）
    async fn send_to_stream(
        &self,
        correlation_id: &str,
        operation: impl Into<storage_operation_request::Operation>,
    ) -> StorageResult<()> {
        let stream_tx = self.stream_tx.read().await;
        if let Some(ref sender) = *stream_tx {
            let msg = ServerStorageMessage {
                message: Some(server_storage_message::Message::OperationRequest(
                    StorageOperationRequest {
                        correlation_id: correlation_id.to_string(),
                        operation: Some(operation.into()),
                    }
                )),
            };
            if sender.send(msg).await.is_err() {
                self.pending_requests.remove(correlation_id);
                return Err(StorageError::Io {
                    path: "".to_string(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "stream closed",
                    ),
                });
            }
        } else {
            self.pending_requests.remove(correlation_id);
            return Err(StorageError::Io {
                path: "".to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "client not connected",
                ),
            });
        }
        Ok(())
    }

    /// 确保连接状态为 Connected，否则等待重连（带超时）
    async fn ensure_connected(&self) -> StorageResult<()> {
        let state = self.state.read().await;
        if *state == ConnectionState::Connected {
            return Ok(());
        }
        drop(state);
        tokio::time::timeout(self.operation_timeout, self.wait_for_connected())
            .await
            .map_err(|_| StorageError::Io {
                path: "".to_string(),
                source: std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "client not connected",
                ),
            })
    }

    async fn wait_for_connected(&self) {
        loop {
            let state = self.state.read().await;
            if *state == ConnectionState::Connected {
                return;
            }
            drop(state);
            self.state_notify.notified().await;
        }
    }
}

/// 错误码映射
fn convert_error_code(err: StorageOperationError) -> StorageError {
    match err.code() {
        StorageErrorCode::NotFound => StorageError::NotFound(err.message),
        StorageErrorCode::AlreadyExists => StorageError::AlreadyExists(err.message),
        StorageErrorCode::IsADirectory => StorageError::IsADirectory(err.message),
        StorageErrorCode::NotADirectory => StorageError::NotADirectory(err.message),
        StorageErrorCode::NotAFile => StorageError::NotAFile(err.message),
        StorageErrorCode::DirectoryNotEmpty => StorageError::DirectoryNotEmpty(err.message),
        StorageErrorCode::PermissionDenied => StorageError::PermissionDenied(err.message),
        StorageErrorCode::PathTraversalDenied => StorageError::PathTraversalDenied(err.message),
        StorageErrorCode::NotSupported => StorageError::NotSupported(err.message),
        _ => StorageError::Io {
            path: err.message.clone(),
            source: std::io::Error::new(std::io::ErrorKind::Other, err.message),
        },
    }
}

/// Proto FileStatData → Rust FileStat 转换
fn convert_file_stat(data: FileStatData) -> FileStat {
    FileStat {
        name: data.name,
        path: data.path,
        file_type: match data.file_type {
            1 => FileType::Directory,
            2 => FileType::Symlink,
            _ => FileType::File,  // 0 或未知值默认为 File
        },
        size: data.size,
        mode: data.mode,
        uid: data.uid,
        gid: data.gid,
        modified_at: data.modified_at.map(|ts| {
            DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                .unwrap_or_default()
        }),
        accessed_at: data.accessed_at.map(|ts| {
            DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                .unwrap_or_default()
        }),
        created_at: data.created_at.map(|ts| {
            DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                .unwrap_or_default()
        }),
    }
}

/// Rust FileStat → Proto FileStatData 转换（用于 Server→Client 方向，如有需要）
fn convert_to_proto_stat(stat: &FileStat) -> FileStatData {
    FileStatData {
        name: stat.name.clone(),
        path: stat.path.clone(),
        file_type: match stat.file_type {
            FileType::File => 0,
            FileType::Directory => 1,
            FileType::Symlink => 2,
        },
        size: stat.size,
        mode: stat.mode,
        uid: stat.uid,
        gid: stat.gid,
        modified_at: stat.modified_at.map(|dt| prost_types::Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }),
        accessed_at: stat.accessed_at.map(|dt| prost_types::Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }),
        created_at: stat.created_at.map(|dt| prost_types::Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }),
    }
}

/// Proto StatFsData → Rust FsStats 转换
fn convert_fs_stats(data: StatFsData) -> FsStats {
    FsStats {
        blocks: data.blocks,
        bfree: data.bfree,
        bavail: data.bavail,
        files: data.files,
        ffree: data.ffree,
        bsize: data.bsize,
        namelen: data.namelen,
        frsize: data.frsize,
    }
}

#[async_trait]
impl StorageBackend for RemoteStorageBackend {
    async fn stat(&self, _workspace_id: &str, path: &str) -> StorageResult<FileStat> {
        // send_request 接收操作内容（不含 correlation_id），内部生成 id 并组装完整消息
        let success = self.send_request(
            storage_operation_request::Operation::Stat(StatRequest {
                path: path.to_string(),
            })
        ).await?;
        match success.data {
            Some(storage_operation_success::Data::Stat(data)) => Ok(convert_file_stat(data)),
            _ => Err(StorageError::Internal("unexpected response type".to_string())),
        }
    }

    async fn list_dir(&self, _workspace_id: &str, path: &str) -> StorageResult<Vec<FileStat>> {
        // ListDir 使用专用的分页收集方法：Client 将大目录分批发送（每批最多 200 条），
        // 多条响应使用相同 correlation_id，最后一批标记 is_last=true。
        // send_list_dir_request 内部使用 PendingRequest::Paged 收集所有批次。
        self.send_list_dir_request(path).await
    }

    async fn read_file(&self, _workspace_id: &str, path: &str) -> StorageResult<Vec<u8>> {
        // read_file 是整文件读取，不预先知道文件大小。
        // 始终走数据流 RPC，避免额外的 stat RTT 开销。
        // 注：FUSE 层的 read 回调使用 read_file_range（已知偏移和长度），
        // 小于阈值的 range 读走控制流，不会触发 read_file。
        self.read_file_via_data_stream(path).await
    }

    async fn read_file_range(
        &self, _workspace_id: &str, path: &str, offset: u64, length: u32,
    ) -> StorageResult<Vec<u8>> {
        // 阈值检查：大于 DATA_STREAM_THRESHOLD (默认 64KB) 走数据流，
        // 避免大消息在控制流上造成 Head-of-Line Blocking
        const DATA_STREAM_THRESHOLD: u32 = 64 * 1024;
        if length > DATA_STREAM_THRESHOLD {
            // 大 range 读走数据流（复用 read_file_via_data_stream，
            // 但 StartDataTransfer 需要携带 offset/length 信息）
            // 注：当前 read_file_via_data_stream 读取整文件。对于带 offset 的大 range 读，
            // Client 端 ReadFileStream 需要支持 offset+length 参数。
            // 此处简化处理：仍通过数据流读取，在 StartDataTransfer 中携带 offset/length。
            return self.read_file_range_via_data_stream(path, offset, length).await;
        }

        // 小 range 读走控制流
        let success = self.send_request(
            storage_operation_request::Operation::ReadFileRange(ReadFileRangeRequest {
                path: path.to_string(),
                offset,
                length,
            })
        ).await?;
        match success.data {
            Some(storage_operation_success::Data::ReadData(data)) => Ok(data.data),
            _ => Err(StorageError::Internal("unexpected response type".to_string())),
        }
    }

    async fn write_file(&self, _workspace_id: &str, path: &str, content: &[u8]) -> StorageResult<()> {
        // write_file 可以直接根据 content.len() 判断，无需额外调用
        if content.len() <= 64 * 1024 {
            // 小文件走控制流
            self.write_file_at(_workspace_id, path, 0, content).await
        } else {
            // 大文件走数据流
            self.write_file_via_data_stream(path, content).await
        }
    }

    async fn write_file_at(
        &self, _workspace_id: &str, path: &str, offset: u64, data: &[u8],
    ) -> StorageResult<()> {
        let success = self.send_request(
            storage_operation_request::Operation::WriteFileAt(WriteFileAtRequest {
                path: path.to_string(),
                offset,
                data: data.to_vec(),
            })
        ).await?;
        Ok(())
    }

    async fn create_file(&self, _workspace_id: &str, path: &str, exclusive: bool) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::CreateFile(CreateFileRequest {
                path: path.to_string(),
                exclusive,
            })
        ).await?;
        Ok(())
    }

    async fn mkdir(&self, _workspace_id: &str, path: &str, recursive: bool) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::Mkdir(MkdirRequest {
                path: path.to_string(),
                recursive,
            })
        ).await?;
        Ok(())
    }

    async fn exists(&self, _workspace_id: &str, path: &str) -> StorageResult<bool> {
        let success = self.send_request(
            storage_operation_request::Operation::Exists(ExistsRequest {
                path: path.to_string(),
            })
        ).await?;
        match success.data {
            Some(storage_operation_success::Data::Exists(data)) => Ok(data.exists),
            _ => Err(StorageError::Internal("unexpected response type".to_string())),
        }
    }

    async fn remove_file(&self, _workspace_id: &str, path: &str) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::RemoveFile(RemoveFileRequest {
                path: path.to_string(),
            })
        ).await?;
        Ok(())
    }

    async fn remove_dir(&self, _workspace_id: &str, path: &str, recursive: bool) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::RemoveDir(RemoveDirRequest {
                path: path.to_string(),
                recursive,
            })
        ).await?;
        Ok(())
    }

    async fn rename(&self, _workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::Rename(RenameRequest {
                src: src.to_string(),
                dst: dst.to_string(),
                flags: 0, // normal rename
            })
        ).await?;
        Ok(())
    }

    async fn rename_noreplace(&self, _workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::Rename(RenameRequest {
                src: src.to_string(), dst: dst.to_string(), flags: 1,
            })
        ).await?;
        Ok(())
    }

    async fn rename_exchange(&self, _workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::Rename(RenameRequest {
                src: src.to_string(), dst: dst.to_string(), flags: 2,
            })
        ).await?;
        Ok(())
    }

    async fn copy(&self, _workspace_id: &str, src: &str, dst: &str) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::Copy(CopyRequest {
                src: src.to_string(), dst: dst.to_string(),
            })
        ).await?;
        Ok(())
    }

    async fn set_file_size(&self, _workspace_id: &str, path: &str, size: u64) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::SetFileSize(SetFileSizeRequest {
                path: path.to_string(), size,
            })
        ).await?;
        Ok(())
    }

    async fn set_permissions(&self, _workspace_id: &str, path: &str, mode: u32) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::SetPermissions(SetPermissionsRequest {
                path: path.to_string(), mode,
            })
        ).await?;
        Ok(())
    }

    async fn set_times(
        &self, _workspace_id: &str, path: &str,
        atime: Option<DateTime<Utc>>, mtime: Option<DateTime<Utc>>,
    ) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::SetTimes(SetTimesRequest {
                path: path.to_string(),
                atime: atime.map(|t| prost_types::Timestamp::from(t)),
                mtime: mtime.map(|t| prost_types::Timestamp::from(t)),
            })
        ).await?;
        Ok(())
    }

    async fn symlink(&self, _workspace_id: &str, link_path: &str, target: &str) -> StorageResult<()> {
        self.send_request(
            storage_operation_request::Operation::Symlink(SymlinkRequest {
                link_path: link_path.to_string(),
                target: target.to_string(),
            })
        ).await?;
        Ok(())
    }

    async fn readlink(&self, _workspace_id: &str, path: &str) -> StorageResult<String> {
        let success = self.send_request(
            storage_operation_request::Operation::ReadLink(ReadLinkRequest {
                path: path.to_string(),
            })
        ).await?;
        match success.data {
            Some(storage_operation_success::Data::ReadLink(data)) => Ok(data.target),
            _ => Err(StorageError::Internal("unexpected response type".to_string())),
        }
    }

    async fn stat_fs(&self, _workspace_id: &str) -> StorageResult<FsStats> {
        let success = self.send_request(
            storage_operation_request::Operation::StatFs(StatFsRequest {})
        ).await?;
        match success.data {
            Some(storage_operation_success::Data::StatFs(data)) => Ok(convert_fs_stats(data)),
            _ => Err(StorageError::Internal("unexpected response type".to_string())),
        }
    }

    async fn create_workspace_root(&self, _workspace_id: &str) -> StorageResult<()> {
        // remote workspace 不需要 Server 创建目录，由 FUSE 挂载代替
        Ok(())
    }

    async fn delete_workspace_root(&self, _workspace_id: &str) -> StorageResult<()> {
        // remote workspace 删除时只需 umount FUSE，不删除 Client 本地文件
        Ok(())
    }
}
```

### 3.2 ClientStorageServiceImpl（gRPC handler）

**新文件**: `server/src/api/grpc/client_storage.rs`

```rust
use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use crate::infra::storage::remote::RemoteStorageBackend;
use crate::infra::storage::router::StorageRouter;
use crate::proto::{
    client_storage_service_server::ClientStorageService,
    ClientMessage, ServerStorageMessage,
    ReadFileStreamRequest, ReadFileStreamResponse,
    WriteFileStreamRequest, WriteFileStreamResponse,
    client_message, server_storage_message,
};

/// 管理所有 remote workspace 的 RemoteStorageBackend 实例
pub struct RemoteStoragePool {
    backends: DashMap<String, Arc<RemoteStorageBackend>>,
    storage_router: Arc<StorageRouter>,
    workspace_repo: Arc<WorkspaceRepository>,
    fuse_mount_manager: Arc<FuseMountManager>,
    max_remote_workspaces: usize,
}

impl RemoteStoragePool {
    pub fn new(
        storage_router: Arc<StorageRouter>,
        workspace_repo: Arc<WorkspaceRepository>,
        fuse_mount_manager: Arc<FuseMountManager>,
        max_remote_workspaces: usize,
    ) -> Self {
        Self {
            backends: DashMap::new(),
            storage_router,
            workspace_repo,
            fuse_mount_manager,
            max_remote_workspaces,
        }
    }

    /// 获取已有的 RemoteStorageBackend，或为首次连接的 workspace 创建新实例。
    ///
    /// 首次连接时：
    /// 1. 验证 workspace 存在且 storage_type=remote
    /// 2. 检查配额限制
    /// 3. 创建 RemoteStorageBackend 并注册到 StorageRouter
    /// 4. 创建 Server 端 FUSE 挂载
    pub async fn get_or_create(
        &self,
        workspace_id: &str,
    ) -> Result<Arc<RemoteStorageBackend>, Status> {
        // 快速路径：已有实例（重连场景）
        if let Some(backend) = self.backends.get(workspace_id) {
            return Ok(backend.value().clone());
        }

        // 慢速路径：首次连接
        // 1. 验证 workspace
        let workspace = self.workspace_repo.get(workspace_id).await
            .map_err(|_| Status::not_found(format!("workspace {} not found", workspace_id)))?;
        if workspace.storage_type != StorageType::Remote {
            return Err(Status::invalid_argument(
                format!("workspace {} is not a remote workspace", workspace_id)
            ));
        }

        // 2. 检查配额
        if self.backends.len() >= self.max_remote_workspaces {
            return Err(Status::resource_exhausted(
                format!("max remote workspaces limit ({}) reached", self.max_remote_workspaces)
            ));
        }

        // 3. 创建 backend
        let backend = Arc::new(RemoteStorageBackend::new(workspace_id.to_string()));
        self.backends.insert(workspace_id.to_string(), backend.clone());
        self.storage_router.register(workspace_id, backend.clone() as Arc<dyn StorageBackend>);

        // 4. FUSE 挂载在 bind_stream 后由调用方触发（见 connect handler）
        Ok(backend)
    }

    /// 移除 workspace 的 backend（workspace 删除时调用）
    pub async fn remove(&self, workspace_id: &str) {
        self.backends.remove(workspace_id);
        self.storage_router.unregister(workspace_id);
    }
}

pub struct ClientStorageServiceImpl {
    pool: Arc<RemoteStoragePool>,
    /// 用于验证 Client token（复用现有 API key 机制）
    api_token: Option<String>,
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

        let (tx, rx) = mpsc::channel::<Result<ServerStorageMessage, Status>>(256);

        let api_token = self.api_token.clone();

        tokio::spawn(async move {
            // 1. 等待握手并验证认证
            let workspace_id = match inbound.next().await {
                Some(Ok(msg)) => match msg.message {
                    Some(client_message::Message::Handshake(hs)) => {
                        // 验证 token：复用现有 API key 机制
                        if let Some(ref expected_token) = api_token {
                            if hs.token != *expected_token {
                                let _ = tx.send(Ok(ServerStorageMessage {
                                    message: Some(server_storage_message::Message::HandshakeAck(
                                        StorageHandshakeAck {
                                            success: false,
                                            error: Some("authentication failed".to_string()),
                                        }
                                    )),
                                })).await;
                                return;
                            }
                        }

                        // 发送握手确认
                        let _ = tx.send(Ok(ServerStorageMessage {
                            message: Some(server_storage_message::Message::HandshakeAck(
                                StorageHandshakeAck { success: true, error: None }
                            )),
                        })).await;
                        hs.workspace_id
                    }
                    _ => {
                        let _ = tx.send(Ok(ServerStorageMessage {
                            message: Some(server_storage_message::Message::HandshakeAck(
                                StorageHandshakeAck {
                                    success: false,
                                    error: Some("expected handshake as first message".to_string()),
                                }
                            )),
                        })).await;
                        return;
                    }
                },
                _ => return,
            };

            // 2. 获取或创建 RemoteStorageBackend（含 workspace 验证和配额检查）
            let backend = match pool.get_or_create(&workspace_id).await {
                Ok(b) => b,
                Err(status) => {
                    tracing::error!("failed to create backend for {}: {}", workspace_id, status);
                    return;
                }
            };

            // 3. 创建到 Client 的发送通道并绑定流
            //    msg_tx 用于 RemoteStorageBackend 向 Client 发送操作请求，
            //    消息通过 tx（outbound channel）转发到 gRPC 流。
            let is_reconnect = pool.fuse_mount_manager.is_mounted(&workspace_id);
            let (msg_tx, mut msg_rx) = mpsc::channel::<ServerStorageMessage>(256);
            backend.bind_stream(msg_tx, is_reconnect).await;

            // 3.1 如果是首次连接（FUSE 未挂载），创建 FUSE 挂载
            // 重连时 FUSE 挂载已存在，跳过
            if let Err(e) = pool.fuse_mount_manager.mount_if_not_exists(
                &workspace_id, backend.clone()
            ).await {
                tracing::error!("failed to create FUSE mount for {}: {}", workspace_id, e);
                backend.unbind_stream().await;
                return;
            }

            // 3.2 重连时执行全量缓存清除（设计文档 3.6 节要求）
            if backend.take_cache_purge_flag() {
                tracing::info!("client reconnected for workspace {}, purging all caches", workspace_id);
                pool.fuse_mount_manager.purge_all_caches(&workspace_id);
            }

            tracing::info!("client connected for workspace {}", workspace_id);

            // 4. 启动 msg_rx → outbound 转发 task
            //    将 RemoteStorageBackend 发来的操作请求转发到 gRPC 流
            let outbound_tx = tx.clone();
            let forward_handle = tokio::spawn(async move {
                while let Some(msg) = msg_rx.recv().await {
                    if outbound_tx.send(Ok(msg)).await.is_err() {
                        break;
                    }
                }
            });

            // 5. 启动心跳 task
            let heartbeat_tx = tx.clone();
            let heartbeat_handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(15));
                loop {
                    interval.tick().await;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let ping = ServerStorageMessage {
                        message: Some(server_storage_message::Message::Ping(
                            StoragePing { timestamp: now }
                        )),
                    };
                    if heartbeat_tx.send(Ok(ping)).await.is_err() {
                        break;
                    }
                }
            });

            // 6. 处理 Client 消息（主循环）
            let mut last_activity = tokio::time::Instant::now();
            let heartbeat_timeout = Duration::from_secs(45);

            loop {
                tokio::select! {
                    msg = inbound.next() => {
                        match msg {
                            Some(Ok(client_msg)) => {
                                last_activity = tokio::time::Instant::now();
                                match client_msg.message {
                                    Some(client_message::Message::OperationResponse(resp)) => {
                                        backend.handle_operation_response(resp);
                                    }
                                    Some(client_message::Message::FileChanged(notif)) => {
                                        if notif.full_purge {
                                            // inotify 降级模式：全量缓存清除
                                            pool.fuse_mount_manager.purge_all_caches(&workspace_id);
                                        } else {
                                            // 正常模式：按路径失效缓存
                                            pool.fuse_mount_manager.invalidate_paths(
                                                &workspace_id, &notif.events
                                            );
                                        }
                                        metrics::increment_file_change_notifications(&workspace_id);
                                    }
                                    Some(client_message::Message::Pong(_)) => {
                                        // 心跳响应，已更新 last_activity
                                    }
                                    Some(client_message::Message::DataTransferFailed(failed)) => {
                                        backend.handle_data_transfer_failed(
                                            &failed.transfer_id,
                                            &failed.reason.unwrap_or_default(),
                                        );
                                    }
                                    _ => {}
                                }
                            }
                            Some(Err(e)) => {
                                tracing::error!("stream error for {}: {}", workspace_id, e);
                                break;
                            }
                            None => break, // Client 关闭了流
                        }
                    }
                    // 心跳超时检测：使用 sleep_until 基于 last_activity 计算精确超时
                    _ = tokio::time::sleep_until(last_activity + heartbeat_timeout) => {
                        tracing::warn!("heartbeat timeout for workspace {}", workspace_id);
                        break;
                    }
                }
            }

            // 7. 断线清理
            heartbeat_handle.abort();
            forward_handle.abort();
            backend.unbind_stream().await;
            metrics::set_connection_state(&workspace_id, 2); // disconnected
            tracing::info!("client disconnected for workspace {}", workspace_id);
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }

    /// ReadFileStream（client-streaming）：Client 读取本地文件并流式发送给 Server。
    ///
    /// 流程：
    /// 1. Server 之前通过控制流发送了 StartDataTransfer(READ_FILE, transfer_id)
    /// 2. Client 发起此 RPC，首条消息为 header（含 transfer_id 和 workspace_id）
    /// 3. Client 在本地读取文件，分块流式发送给 Server
    /// 4. Server 收集所有 chunks 后通过 transfer_id 将结果返回给等待方
    async fn read_file_stream(
        &self,
        request: Request<Streaming<ReadFileStreamRequest>>,
    ) -> Result<Response<ReadFileStreamResponse>, Status> {
        let mut inbound = request.into_inner();
        let pool = self.pool.clone();

        // 1. 解析首条消息获取 header（含 workspace_id 用于直接定位 backend）
        let header = match inbound.next().await {
            Some(Ok(msg)) => match msg.payload {
                Some(read_file_stream_request::Payload::Header(h)) => h,
                _ => return Err(Status::invalid_argument("first message must be header")),
            },
            Some(Err(e)) => return Err(e),
            None => return Err(Status::invalid_argument("empty stream")),
        };

        let transfer_id = header.transfer_id;
        let workspace_id = header.workspace_id; // header 中携带 workspace_id，避免遍历

        // 通过 workspace_id 直接定位 backend
        let backend = pool.backends.get(&workspace_id)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| Status::not_found(
                format!("no backend for workspace {}", workspace_id)
            ))?;

        // 2. 收集后续 data chunks
        let mut data = Vec::new();
        while let Some(msg) = inbound.next().await {
            match msg {
                Ok(req) => match req.payload {
                    Some(read_file_stream_request::Payload::Data(chunk)) => {
                        data.extend_from_slice(&chunk);
                    }
                    _ => {} // 忽略非 data payload（如 header 重复发送）
                },
                Err(e) => {
                    // 通知原始请求失败
                    backend.complete_data_transfer(
                        &transfer_id,
                        DataTransferResult::Error(StorageError::Io {
                            path: "".to_string(),
                            source: std::io::Error::new(
                                std::io::ErrorKind::BrokenPipe,
                                format!("read data stream error: {}", e),
                            ),
                        }),
                    );
                    return Err(e);
                }
            }
        }

        let bytes_read = data.len() as u64;

        // 3. 通知原始请求完成（将收集的数据传递给等待方）
        let found = backend.complete_data_transfer(
            &transfer_id,
            DataTransferResult::ReadComplete(data),
        );
        if !found {
            // transfer_id 已超时清理，返回 NOT_FOUND
            return Err(Status::not_found(
                format!("transfer {} expired or not found", transfer_id)
            ));
        }

        Ok(Response::new(ReadFileStreamResponse { bytes_read }))
    }

    type WriteFileStreamStream = Pin<Box<dyn Stream<Item = Result<WriteFileStreamResponse, Status>> + Send>>;

    /// WriteFileStream（server-streaming）：Server 流式发送待写入数据给 Client。
    ///
    /// 流程：
    /// 1. Server 之前通过控制流发送了 StartDataTransfer(WRITE_FILE, transfer_id, path)
    /// 2. Client 发起此 RPC，携带 transfer_id
    /// 3. Server 从 pending_transfers（PendingTransfer::Write）中取出待写入数据
    /// 4. Server 分块流式发送给 Client
    /// 5. Client 接收数据并写入本地文件
    /// 6. 最后一条消息为 done 标记，Client 据此关闭本地文件
    /// 7. RPC 正常结束后，通过 complete_data_transfer 通知等待方
    async fn write_file_stream(
        &self,
        request: Request<WriteFileStreamRequest>,
    ) -> Result<Response<Self::WriteFileStreamStream>, Status> {
        let req = request.into_inner();
        let transfer_id = req.transfer_id.clone();
        let workspace_id = req.workspace_id.clone(); // WriteFileStreamRequest 中携带 workspace_id
        let pool = self.pool.clone();

        // 通过 workspace_id 直接定位 backend，避免遍历全部 backends
        let backend = pool.backends.get(&workspace_id)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| Status::not_found(
                format!("no backend for workspace {}", workspace_id)
            ))?;

        // 从 pending_transfers 中取出待写入数据（由 write_file_via_data_stream 暂存）
        let data = match backend.get_write_data(&transfer_id) {
            Some(d) => d,
            None => return Err(Status::not_found(
                format!("write transfer {} expired or not found", transfer_id)
            )),
        };

        let (tx, rx) = mpsc::channel::<Result<WriteFileStreamResponse, Status>>(32);

        tokio::spawn(async move {
            const CHUNK_SIZE: usize = 64 * 1024; // 64KB per chunk
            let total_bytes = data.len() as u64;

            // 分块发送数据
            for chunk in data.chunks(CHUNK_SIZE) {
                if tx.send(Ok(WriteFileStreamResponse {
                    payload: Some(write_file_stream_response::Payload::Data(chunk.to_vec())),
                })).await.is_err() {
                    // Client 断开，通知等待方失败
                    backend.complete_data_transfer(
                        &transfer_id,
                        DataTransferResult::Error(StorageError::Io {
                            path: "".to_string(),
                            source: std::io::Error::new(
                                std::io::ErrorKind::BrokenPipe,
                                "write stream closed by client",
                            ),
                        }),
                    );
                    return;
                }
            }

            // 发送 done 标记
            let _ = tx.send(Ok(WriteFileStreamResponse {
                payload: Some(write_file_stream_response::Payload::Done(
                    WriteFileStreamDone { total_bytes }
                )),
            })).await;

            // RPC 正常结束即视为 Client 写入成功
            backend.complete_data_transfer(
                &transfer_id,
                DataTransferResult::WriteComplete(total_bytes),
            );
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream)))
    }
}
```

### 3.3 注册到 gRPC Server

**修改文件**: `server/src/main.rs`

```rust
use api::grpc::client_storage::ClientStorageServiceImpl;
use proto::client_storage_service_server::ClientStorageServiceServer;

// 在 grpc_router 构建中增加 ClientStorageService
let client_storage_service = ClientStorageServiceImpl::new(
    remote_storage_pool.clone(),
);

let grpc_router = Server::builder()
    .add_service(agent_grpc_server)
    .add_service(ClientStorageServiceServer::new(client_storage_service))  // 新增
    // ... 其余 service ...
```

---

## 4. Phase 3: Server 端 FUSE 挂载

### 4.1 提取共享 FUSE 库

当前 `fuse-client/src/` 中的 FUSE 逻辑需要提取为共享库，供 Server 端复用。

**方案**: 新建 workspace crate `fuse-core/`，提取公共逻辑。

```
fuse-core/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── inode.rs        ← 从 fuse-client/src/inode.rs 提取
│   ├── cache.rs        ← 从 fuse-client/src/cache.rs 提取
│   └── fuse_fs.rs      ← 通用 FUSE 实现，参数化 StorageBackend trait
```

**关键变更**: `WorkspaceFuse` 的后端从 `FileSystemRpcClient`（gRPC client）抽象为一个泛型 trait，让 Server 端可以直接注入 `RemoteStorageBackend`。

**同步/异步桥接设计**：`fuser::Filesystem` 的回调方法是同步的（`fn lookup(&mut self, ...)` 而非 `async fn`），但 `FuseBackend` 的方法是异步的。解决方案参考现有 `fuse-client`：`WorkspaceFuse` 持有一个 `tokio::runtime::Handle`，在同步回调中通过 `handle.block_on(async_method())` 桥接到异步世界。

- **fuse-client 场景**：创建独立的 `tokio::Runtime`，FUSE 主循环运行在该 Runtime 的阻塞线程上
- **Server 端场景**：Server 本身已有 tokio Runtime，传入 `Runtime::handle().clone()` 即可。**但需要注意**：`block_on` 不能在已有 tokio runtime 的线程上调用（会 panic）。由于 FUSE 事件循环通过 `spawn_blocking` 运行在独立线程上（不是 tokio worker 线程），因此可以安全使用 `handle.block_on()`

```rust
// fuse-core/src/lib.rs
use async_trait::async_trait;
use tokio::runtime::Handle;

/// FUSE 文件系统后端 trait
/// fuse-client 实现为 gRPC client，Server 端实现为 RemoteStorageBackend 适配器
#[async_trait]
pub trait FuseBackend: Send + Sync + 'static {
    async fn stat(&self, path: &str) -> Result<FileStat, FuseError>;
    async fn list_dir(&self, path: &str) -> Result<Vec<FileStat>, FuseError>;
    async fn read_at(&self, path: &str, offset: u64, size: u32) -> Result<Vec<u8>, FuseError>;
    async fn write_at(&self, path: &str, offset: u64, data: &[u8]) -> Result<u64, FuseError>;
    /// 创建文件。返回新文件的属性。
    /// 注：StorageBackend::create_file 返回 ()，适配器需要在 create 后额外调用 stat
    async fn create(&self, path: &str, mode: u32, exclusive: bool) -> Result<FileStat, FuseError>;
    /// 创建目录。返回新目录的属性。
    /// 注：StorageBackend::mkdir 返回 ()，适配器需要在 mkdir 后额外调用 stat
    async fn mkdir(&self, path: &str, mode: u32) -> Result<FileStat, FuseError>;
    async fn remove_file(&self, path: &str) -> Result<(), FuseError>;
    async fn remove_dir(&self, path: &str) -> Result<(), FuseError>;
    async fn rename(&self, src: &str, dst: &str, flags: u32) -> Result<(), FuseError>;
    async fn set_attr(&self, path: &str, attr: SetAttrRequest) -> Result<FileStat, FuseError>;
    async fn symlink(&self, link_path: &str, target: &str) -> Result<FileStat, FuseError>;
    async fn readlink(&self, path: &str) -> Result<String, FuseError>;
    async fn statfs(&self) -> Result<FsStats, FuseError>;
}

/// FUSE 配置
pub struct FuseConfig {
    pub entry_timeout_secs: u64,
    pub attr_timeout_secs: u64,
    pub read_cache_size_bytes: usize,
    pub block_size: usize,
}

impl Default for FuseConfig {
    fn default() -> Self {
        Self {
            entry_timeout_secs: 1,
            attr_timeout_secs: 1,
            read_cache_size_bytes: 64 * 1024 * 1024, // 64MB
            block_size: 128 * 1024, // 128KB
        }
    }
}

/// 通用 FUSE 文件系统实现
/// 从现有 fuse-client 的 WorkspaceFuse 提取，参数化后端为 FuseBackend trait
pub struct WorkspaceFuse {
    workspace_id: String,
    /// tokio runtime handle，用于在同步 FUSE 回调中调用异步方法
    /// fuse-client 模式：传入自创建的 Runtime 的 handle
    /// Server 模式：传入主 tokio Runtime 的 handle
    runtime_handle: Handle,
    backend: Arc<dyn FuseBackend>,
    inodes: InodeTable,
    meta_cache: MetadataCache,
    dir_cache: DirCache,
    read_cache: Arc<ReadCache>,
    statfs_cache: StatfsCache,
    config: FuseConfig,
    // 以下字段同现有 fuse-client WorkspaceFuse
    next_fh: AtomicU64,
    fh_table: RwLock<HashMap<u64, OpenFileHandle>>,
    readahead_state: RwLock<HashMap<u64, ReadaheadState>>,
    uid: u32,
    gid: u32,
}

impl WorkspaceFuse {
    pub fn new(
        workspace_id: String,
        runtime_handle: Handle,
        backend: Arc<dyn FuseBackend>,
        config: FuseConfig,
    ) -> Self {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let cache_ttl = Duration::from_secs(config.attr_timeout_secs);

        Self {
            workspace_id,
            runtime_handle,
            backend,
            inodes: InodeTable::new(),
            meta_cache: MetadataCache::new(cache_ttl),
            dir_cache: DirCache::new(cache_ttl),
            read_cache: Arc::new(ReadCache::with_max_size(
                config.block_size,
                config.read_cache_size_bytes,
            )),
            statfs_cache: StatfsCache::new(),
            config,
            // 同现有 fuse-client 模式：file handle 从 1 开始分配
            next_fh: AtomicU64::new(1),
            fh_table: RwLock::new(HashMap::new()),
            readahead_state: RwLock::new(HashMap::new()),
            uid,
            gid,
        }
    }
}

// fuser::Filesystem 实现中通过 runtime_handle.block_on() 桥接异步调用
impl fuser::Filesystem for WorkspaceFuse {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let path = /* resolve path from inode */;
        match self.runtime_handle.block_on(self.backend.stat(&path)) {
            Ok(stat) => { /* convert to FileAttr, reply.entry() */ }
            Err(e) => { reply.error(e.to_errno()); }
        }
    }
    // ... 其余回调方法同理，均通过 runtime_handle.block_on() 桥接 ...
}
```

### 4.2 Server 端 FUSE 挂载管理器

**新文件**: `server/src/infra/fuse_mount.rs`

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::task::JoinHandle;

use crate::infra::storage::remote::RemoteStorageBackend;

/// 管理所有 remote workspace 的 Server 端 FUSE 挂载
pub struct FuseMountManager {
    /// workspace_dir 基础路径 (e.g., /var/lib/workspace)
    base_dir: PathBuf,
    /// workspace_id → FUSE 挂载 task handle
    mounts: DashMap<String, FuseMount>,
    /// tokio runtime handle（用于 FUSE 同步/异步桥接）
    runtime_handle: tokio::runtime::Handle,
    /// FUSE 缓存配置
    fuse_config: fuse_core::FuseConfig,
}

struct FuseMount {
    /// FUSE 事件循环 task（进程内线程，通过 spawn_blocking 运行）
    task_handle: JoinHandle<()>,
    /// 挂载路径
    mount_path: PathBuf,
}

impl FuseMountManager {
    pub fn new(
        base_dir: PathBuf,
        runtime_handle: tokio::runtime::Handle,
        fuse_config: fuse_core::FuseConfig,
    ) -> Self {
        Self {
            base_dir,
            mounts: DashMap::new(),
            runtime_handle,
            fuse_config,
        }
    }

    /// 创建 FUSE 挂载（Client 首次连接时调用）
    pub async fn mount(
        &self,
        workspace_id: &str,
        backend: Arc<RemoteStorageBackend>,
    ) -> anyhow::Result<()> {
        let mount_path = self.base_dir.join(workspace_id);

        // 创建挂载点目录
        tokio::fs::create_dir_all(&mount_path).await?;

        // 构建 Server 端 FUSE backend 适配器
        let fuse_backend = ServerFuseBackend::new(workspace_id.to_string(), backend);

        // 创建 WorkspaceFuse 实例
        // 传入 Server 主 tokio runtime 的 handle，用于在 FUSE 同步回调中
        // 通过 handle.block_on() 调用 FuseBackend 的异步方法。
        // 这是安全的，因为 FUSE 事件循环通过 spawn_blocking 运行在独立线程上，
        // 不会与 tokio worker 线程冲突。
        let fuse_fs = fuse_core::WorkspaceFuse::new(
            workspace_id.to_string(),
            self.runtime_handle.clone(),
            Arc::new(fuse_backend),
            self.fuse_config.clone(),
        );

        // 在独立线程中运行 FUSE 事件循环
        let mount_path_clone = mount_path.clone();
        let ws_id = workspace_id.to_string();
        let task_handle = tokio::task::spawn_blocking(move || {
            // FUSE 挂载选项：allow_other 让其他用户/进程（Docker、NFS server）可访问
            let options = vec![
                fuser::MountOption::FSName("elevo-remote".to_string()),
                fuser::MountOption::AllowOther,
                fuser::MountOption::DefaultPermissions,
            ];

            // 捕获 panic，防止影响 Server 进程
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                fuser::mount2(fuse_fs, &mount_path_clone, &options)
            }));

            match result {
                Ok(Ok(())) => tracing::info!("FUSE unmounted for workspace {}", ws_id),
                Ok(Err(e)) => tracing::error!("FUSE error for workspace {}: {}", ws_id, e),
                Err(panic_info) => {
                    tracing::error!("FUSE panic for workspace {}: {:?}", ws_id, panic_info);
                    // panic 后 FUSE 挂载点变为 stale，健康检查会捕获并重建
                }
            }
        });

        self.mounts.insert(workspace_id.to_string(), FuseMount {
            task_handle,
            mount_path,
        });

        Ok(())
    }

    /// 如果 FUSE 未挂载则创建挂载（重连时跳过）
    pub async fn mount_if_not_exists(
        &self,
        workspace_id: &str,
        backend: Arc<RemoteStorageBackend>,
    ) -> anyhow::Result<()> {
        if self.mounts.contains_key(workspace_id) {
            return Ok(());
        }
        self.mount(workspace_id, backend).await
    }

    /// 卸载 FUSE（workspace 删除或通道切换时调用）
    ///
    /// 重要：必须先执行 fusermount -u 使 fuser::mount2 返回，然后再 abort task handle。
    /// 因为 spawn_blocking 中的阻塞系统调用无法被 tokio abort() 直接中断，
    /// fusermount -u 会使 FUSE 事件循环退出，从而让 mount2 自然返回。
    pub async fn umount(&self, workspace_id: &str) -> anyhow::Result<()> {
        if let Some((_, mount)) = self.mounts.remove(workspace_id) {
            // 步骤 1: fusermount -u 卸载 — 使 fuser::mount2 返回
            let status = tokio::process::Command::new("fusermount")
                .args(["-u", mount.mount_path.to_str().unwrap()])
                .status()
                .await?;

            if !status.success() {
                // 强制 lazy 卸载
                tokio::process::Command::new("fusermount")
                    .args(["-uz", mount.mount_path.to_str().unwrap()])
                    .status()
                    .await?;
            }

            // 步骤 2: 等待 task 自然退出（给 mount2 返回的时间），或超时后 abort
            tokio::select! {
                _ = mount.task_handle => {}
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    tracing::warn!(
                        "FUSE task for {} did not exit within 5s after umount, aborting",
                        workspace_id
                    );
                }
            }
        }
        Ok(())
    }

    /// 检查某个 workspace 是否已有 FUSE 挂载
    pub fn is_mounted(&self, workspace_id: &str) -> bool {
        self.mounts.contains_key(workspace_id)
    }

    /// 清除指定 workspace 的所有 FUSE 缓存（重连后调用）
    pub fn purge_all_caches(&self, workspace_id: &str) {
        // 通过 FUSE 内核的 invalidation 接口清除缓存
        // 具体实现依赖 fuse-core 暴露的缓存清除 API
        if let Some(mount) = self.mounts.get(workspace_id) {
            // 调用 fuse-core 的全量缓存清除接口
            tracing::info!("purging all FUSE caches for workspace {}", workspace_id);
        }
    }

    /// 通知 FUSE 缓存失效（文件变更通知到达时调用）
    pub fn invalidate_paths(&self, workspace_id: &str, events: &[FileChangeEvent]) {
        if let Some(mount) = self.mounts.get(workspace_id) {
            for event in events {
                // 调用 fuse-core 的 invalidate_path 接口
            }
        }
    }

    /// 健康检查（每 30 秒调用一次）
    pub async fn health_check(&self) -> Vec<String> {
        let mut unhealthy = Vec::new();
        for entry in self.mounts.iter() {
            let workspace_id = entry.key();
            let mount = entry.value();
            // 使用 statfs 检查挂载健康状态
            match tokio::time::timeout(
                Duration::from_secs(5),
                tokio::fs::metadata(&mount.mount_path),
            ).await {
                Ok(Ok(_)) => {} // 健康
                _ => unhealthy.push(workspace_id.clone()),
            }
        }
        unhealthy
    }
}

/// 适配器：将 RemoteStorageBackend 适配为 fuse-core 的 FuseBackend trait
///
/// FuseBackend 与 StorageBackend 的方法签名存在差异，适配器负责转换：
/// - FuseBackend::create/mkdir 返回 FileStat，而 StorageBackend::create_file/mkdir 返回 ()
///   → 适配器在 create/mkdir 后额外调用 stat 获取新创建项的属性
/// - FuseBackend 使用 FuseError，StorageBackend 使用 StorageError
///   → 适配器做错误类型转换
struct ServerFuseBackend {
    workspace_id: String,
    backend: Arc<RemoteStorageBackend>,
}

impl ServerFuseBackend {
    fn new(workspace_id: String, backend: Arc<RemoteStorageBackend>) -> Self {
        Self { workspace_id, backend }
    }

    /// StorageError → FuseError 转换
    fn convert_error(e: StorageError) -> fuse_core::FuseError {
        match e {
            StorageError::NotFound(_) => fuse_core::FuseError::NotFound,
            StorageError::AlreadyExists(_) => fuse_core::FuseError::AlreadyExists,
            StorageError::IsADirectory(_) => fuse_core::FuseError::IsADirectory,
            StorageError::NotADirectory(_) => fuse_core::FuseError::NotADirectory,
            StorageError::DirectoryNotEmpty(_) => fuse_core::FuseError::DirectoryNotEmpty,
            StorageError::PermissionDenied(_) => fuse_core::FuseError::PermissionDenied,
            _ => fuse_core::FuseError::Io(e.to_string()),
        }
    }

    /// StorageBackend FileStat → fuse-core FileStat 转换
    fn convert_stat(s: crate::infra::storage::FileStat) -> fuse_core::FileStat {
        fuse_core::FileStat {
            name: s.name,
            path: s.path,
            file_type: match s.file_type {
                crate::infra::storage::FileType::File => fuse_core::FileType::File,
                crate::infra::storage::FileType::Directory => fuse_core::FileType::Directory,
                crate::infra::storage::FileType::Symlink => fuse_core::FileType::Symlink,
            },
            size: s.size,
            mode: s.mode,
            uid: s.uid,
            gid: s.gid,
            modified_at: s.modified_at,
            accessed_at: s.accessed_at,
            created_at: s.created_at,
        }
    }
}

#[async_trait]
impl fuse_core::FuseBackend for ServerFuseBackend {
    async fn stat(&self, path: &str) -> Result<fuse_core::FileStat, fuse_core::FuseError> {
        self.backend.stat(&self.workspace_id, path).await
            .map(Self::convert_stat)
            .map_err(Self::convert_error)
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<fuse_core::FileStat>, fuse_core::FuseError> {
        self.backend.list_dir(&self.workspace_id, path).await
            .map(|entries| entries.into_iter().map(Self::convert_stat).collect())
            .map_err(Self::convert_error)
    }

    async fn read_at(&self, path: &str, offset: u64, size: u32) -> Result<Vec<u8>, fuse_core::FuseError> {
        self.backend.read_file_range(&self.workspace_id, path, offset, size).await
            .map_err(Self::convert_error)
    }

    async fn write_at(&self, path: &str, offset: u64, data: &[u8]) -> Result<u64, fuse_core::FuseError> {
        self.backend.write_file_at(&self.workspace_id, path, offset, data).await
            .map(|_| data.len() as u64)
            .map_err(Self::convert_error)
    }

    async fn create(&self, path: &str, _mode: u32, exclusive: bool) -> Result<fuse_core::FileStat, fuse_core::FuseError> {
        // StorageBackend::create_file 返回 ()，需要额外 stat 获取属性
        self.backend.create_file(&self.workspace_id, path, exclusive).await
            .map_err(Self::convert_error)?;
        self.stat(path).await
    }

    async fn mkdir(&self, path: &str, _mode: u32) -> Result<fuse_core::FileStat, fuse_core::FuseError> {
        // StorageBackend::mkdir 返回 ()，需要额外 stat 获取属性
        self.backend.mkdir(&self.workspace_id, path, false).await
            .map_err(Self::convert_error)?;
        self.stat(path).await
    }

    async fn remove_file(&self, path: &str) -> Result<(), fuse_core::FuseError> {
        self.backend.remove_file(&self.workspace_id, path).await
            .map_err(Self::convert_error)
    }

    async fn remove_dir(&self, path: &str) -> Result<(), fuse_core::FuseError> {
        self.backend.remove_dir(&self.workspace_id, path, false).await
            .map_err(Self::convert_error)
    }

    async fn rename(&self, src: &str, dst: &str, flags: u32) -> Result<(), fuse_core::FuseError> {
        match flags {
            0 => self.backend.rename(&self.workspace_id, src, dst).await,
            1 => self.backend.rename_noreplace(&self.workspace_id, src, dst).await,
            2 => self.backend.rename_exchange(&self.workspace_id, src, dst).await,
            _ => Err(StorageError::NotSupported("unknown rename flags".to_string())),
        }.map_err(Self::convert_error)
    }

    async fn set_attr(&self, path: &str, attr: fuse_core::SetAttrRequest) -> Result<fuse_core::FileStat, fuse_core::FuseError> {
        if let Some(size) = attr.size {
            self.backend.set_file_size(&self.workspace_id, path, size).await
                .map_err(Self::convert_error)?;
        }
        if let Some(mode) = attr.mode {
            self.backend.set_permissions(&self.workspace_id, path, mode).await
                .map_err(Self::convert_error)?;
        }
        if attr.atime.is_some() || attr.mtime.is_some() {
            self.backend.set_times(&self.workspace_id, path, attr.atime, attr.mtime).await
                .map_err(Self::convert_error)?;
        }
        self.stat(path).await
    }

    async fn symlink(&self, link_path: &str, target: &str) -> Result<fuse_core::FileStat, fuse_core::FuseError> {
        self.backend.symlink(&self.workspace_id, link_path, target).await
            .map_err(Self::convert_error)?;
        self.stat(link_path).await
    }

    async fn readlink(&self, path: &str) -> Result<String, fuse_core::FuseError> {
        self.backend.readlink(&self.workspace_id, path).await
            .map_err(Self::convert_error)
    }

    async fn statfs(&self) -> Result<fuse_core::FsStats, fuse_core::FuseError> {
        self.backend.stat_fs(&self.workspace_id).await
            .map(|s| fuse_core::FsStats {
                blocks: s.blocks, bfree: s.bfree, bavail: s.bavail,
                files: s.files, ffree: s.ffree, bsize: s.bsize,
                namelen: s.namelen, frsize: s.frsize,
            })
            .map_err(Self::convert_error)
    }
}
```

### 4.3 缓存失效接入

文件变更通知到达后，需要通知 FUSE 内核失效缓存。`fuse-core` 库需要暴露缓存失效接口。

```rust
// fuse-core/src/fuse_fs.rs
impl WorkspaceFuse {
    /// 收到文件变更通知后调用，清除指定路径的缓存
    pub fn invalidate_path(&self, path: &str, event_type: FileChangeType) {
        // 清除应用层缓存
        self.meta_cache.invalidate(path);
        self.dir_cache.invalidate(parent_of(path));

        // 通知内核缓存失效
        match event_type {
            FileChangeType::Modified | FileChangeType::AttrChanged => {
                if let Some(inode) = self.inodes.get_inode(path) {
                    // fuse_lowlevel_notify_inval_inode 等价操作
                    // 具体依赖 fuser crate 的 notify 能力
                }
            }
            FileChangeType::Created | FileChangeType::Deleted | FileChangeType::Renamed => {
                // fuse_lowlevel_notify_inval_entry 等价操作
            }
        }
    }
}
```

---

## 5. Phase 4: Go SDK

### 5.1 新增文件

```
sdk-go/
├── storage_provider.go       # StorageProvider 核心实现
├── storage_provider_ops.go   # 各文件操作的本地执行逻辑
├── storage_provider_path.go  # openat 路径安全校验
├── storage_provider_watch.go # fsnotify 文件变更监听
└── storage_provider_test.go  # 单元测试
```

### 5.2 StorageProvider 核心结构

**新文件**: `sdk-go/storage_provider.go`

```go
package sdk

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"time"

	"google.golang.org/grpc"
	pb "github.com/OpenElevo/ElevoSandbox/proto/workspace/v1"
)

// StorageProviderConfig 配置
type StorageProviderConfig struct {
	// 本地共享目录路径
	LocalDir string
	// Workspace ID
	WorkspaceID string
	// 认证 token（复用 API key 机制）
	Token string
	// 工作 goroutine 池大小（默认 64）
	WorkerPoolSize int
	// 响应 channel 容量（默认 256）
	ResponseBufferSize int
	// 最大并发数据流（默认 8）
	MaxConcurrentDataStreams int
	// 操作超时（默认 10s）
	OperationTimeout time.Duration
}

// StorageProvider 将本地目录通过 gRPC 反向流共享给 Server
type StorageProvider struct {
	config StorageProviderConfig
	conn   *grpc.ClientConn
	client pb.ClientStorageServiceClient

	// 控制流
	controlStream pb.ClientStorageService_ConnectClient
	// 响应 channel（串行化写入控制流）
	responseCh chan *pb.ClientMessage

	// 文件变更 watcher
	watcher *fileWatcher

	// 路径安全校验器（openat based）
	pathGuard *pathGuard

	// per-file 写锁
	fileLocks sync.Map // map[string]*chanMutex

	// 数据流并发控制
	dataStreamSem chan struct{}

	// 运行状态
	ctx    context.Context
	cancel context.CancelFunc
	wg     sync.WaitGroup

	// 连接状态
	connected atomic.Bool
}

func NewStorageProvider(conn *grpc.ClientConn, config StorageProviderConfig) *StorageProvider {
	if config.WorkerPoolSize == 0 {
		config.WorkerPoolSize = 64
	}
	if config.ResponseBufferSize == 0 {
		config.ResponseBufferSize = 256
	}
	if config.MaxConcurrentDataStreams == 0 {
		config.MaxConcurrentDataStreams = 8
	}
	if config.OperationTimeout == 0 {
		config.OperationTimeout = 10 * time.Second
	}

	ctx, cancel := context.WithCancel(context.Background())

	return &StorageProvider{
		config:        config,
		conn:          conn,
		client:        pb.NewClientStorageServiceClient(conn),
		responseCh:    make(chan *pb.ClientMessage, config.ResponseBufferSize),
		dataStreamSem: make(chan struct{}, config.MaxConcurrentDataStreams),
		ctx:           ctx,
		cancel:        cancel,
	}
}

// Share 启动共享（阻塞，直到 ctx 取消或出错）
func (sp *StorageProvider) Share(ctx context.Context) error {
	// 初始化路径安全校验器
	var err error
	sp.pathGuard, err = newPathGuard(sp.config.LocalDir)
	if err != nil {
		return fmt.Errorf("init path guard: %w", err)
	}

	// 启动文件变更监听
	sp.watcher, err = newFileWatcher(sp.config.LocalDir, sp.responseCh)
	if err != nil {
		return fmt.Errorf("init file watcher: %w", err)
	}

	// 指数退避重连循环
	backoff := time.Second
	maxBackoff := 30 * time.Second

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}

		connectedAt, err := sp.connectAndServe(ctx)
		if err != nil {
			log.Printf("connection error: %v, reconnecting in %v", err, backoff)
		}

		sp.connected.Store(false)

		// 如果连接曾经成功建立过，重置退避
		// 避免长时间稳定运行后的首次断线使用过大的退避值
		if !connectedAt.IsZero() {
			backoff = time.Second
		}

		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(backoff):
		}

		backoff = min(backoff*2, maxBackoff)
	}
}

// connectAndServe 建立连接、执行握手、启动工作循环。
// 返回 (connectedAt, error)：connectedAt 为连接成功的时间（zero 表示从未成功连接）。
func (sp *StorageProvider) connectAndServe(ctx context.Context) (time.Time, error) {
	// 1. 建立控制流
	stream, err := sp.client.Connect(ctx)
	if err != nil {
		return time.Time{}, fmt.Errorf("connect: %w", err)
	}
	sp.controlStream = stream

	// 2. 发送握手
	err = stream.Send(&pb.ClientMessage{
		Message: &pb.ClientMessage_Handshake{
			Handshake: &pb.StorageHandshake{
				WorkspaceId: sp.config.WorkspaceID,
				Token:       sp.config.Token,
			},
		},
	})
	if err != nil {
		return time.Time{}, fmt.Errorf("send handshake: %w", err)
	}

	// 3. 等待握手确认
	msg, err := stream.Recv()
	if err != nil {
		return time.Time{}, fmt.Errorf("recv handshake ack: %w", err)
	}
	ack := msg.GetHandshakeAck()
	if ack == nil || !ack.Success {
		return time.Time{}, fmt.Errorf("handshake failed: %s", ack.GetError())
	}

	connectedAt := time.Now()
	sp.connected.Store(true)

	// 4. 启动响应写入 goroutine
	sp.wg.Add(1)
	go sp.responseWriter(stream)

	// 5. 启动工作池
	requestCh := make(chan *pb.StorageOperationRequest, sp.config.WorkerPoolSize)
	for i := 0; i < sp.config.WorkerPoolSize; i++ {
		sp.wg.Add(1)
		go sp.worker(requestCh)
	}

	// 6. 主循环：从控制流读取请求
	for {
		msg, err := stream.Recv()
		if err != nil {
			return connectedAt, fmt.Errorf("recv: %w", err)
		}

		switch m := msg.Message.(type) {
		case *pb.ServerStorageMessage_OperationRequest:
			requestCh <- m.OperationRequest
		case *pb.ServerStorageMessage_Ping:
			sp.responseCh <- &pb.ClientMessage{
				Message: &pb.ClientMessage_Pong{
					Pong: &pb.StoragePong{Timestamp: m.Ping.Timestamp},
				},
			}
		case *pb.ServerStorageMessage_StartDataTransfer:
			go sp.handleDataTransfer(ctx, m.StartDataTransfer)
		}
	}
}

// responseWriter 串行化写入控制流
func (sp *StorageProvider) responseWriter(stream pb.ClientStorageService_ConnectClient) {
	defer sp.wg.Done()
	for {
		select {
		case <-sp.ctx.Done():
			return
		case msg := <-sp.responseCh:
			if err := stream.Send(msg); err != nil {
				return
			}
		}
	}
}

// worker 处理文件操作请求
func (sp *StorageProvider) worker(requestCh <-chan *pb.StorageOperationRequest) {
	defer sp.wg.Done()
	for {
		select {
		case <-sp.ctx.Done():
			return
		case req := <-requestCh:
			resp := sp.executeOperation(req)
			if resp != nil { // nil 表示分页操作已通过 responseCh 自行发送
				sp.responseCh <- resp
			}
		}
	}
}

// handleDataTransfer 处理 Server 发起的数据流传输请求
//
// Server 通过控制流发送 StartDataTransfer 通知，Client 据此发起独立的
// ReadFileStream（读取本地文件发给 Server）或 WriteFileStream（从 Server
// 接收数据写入本地文件）RPC。
//
// 使用 dataStreamSem 信号量限制并发数据流数量（默认 8）。
func (sp *StorageProvider) handleDataTransfer(ctx context.Context, req *pb.StartDataTransfer) {
	// 获取数据流信号量（限制并发）
	select {
	case sp.dataStreamSem <- struct{}{}:
		defer func() { <-sp.dataStreamSem }()
	case <-ctx.Done():
		return
	case <-time.After(sp.config.OperationTimeout):
		// 超时获取信号量，通知 Server 数据流建立失败
		sp.responseCh <- &pb.ClientMessage{
			Message: &pb.ClientMessage_DataTransferFailed{
				DataTransferFailed: &pb.DataTransferFailed{
					TransferId: req.TransferId,
					Reason: "data stream semaphore timeout",
				},
			},
		}
		return
	}

	switch req.Operation {
	case pb.DataTransferOperation_READ_FILE:
		sp.handleReadFileTransfer(ctx, req)
	case pb.DataTransferOperation_WRITE_FILE:
		sp.handleWriteFileTransfer(ctx, req)
	default:
		sp.responseCh <- &pb.ClientMessage{
			Message: &pb.ClientMessage_DataTransferFailed{
				DataTransferFailed: &pb.DataTransferFailed{
					TransferId: req.TransferId,
					Reason: fmt.Sprintf("unknown transfer operation: %v", req.Operation),
				},
			},
		}
	}
}

// handleReadFileTransfer 读取本地文件并通过 ReadFileStream RPC 流式发送给 Server
func (sp *StorageProvider) handleReadFileTransfer(ctx context.Context, req *pb.StartDataTransfer) {
	const chunkSize = 64 * 1024 // 64KB per chunk

	// 通过 openat 安全地打开文件
	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("path guard: %v", err))
		return
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	fd, err := unix.Openat(dirFd, name, unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("open file: %v", err))
		return
	}
	f := os.NewFile(uintptr(fd), req.Path)
	defer f.Close()

	// 发起 ReadFileStream RPC
	stream, err := sp.client.ReadFileStream(ctx)
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("open read stream: %v", err))
		return
	}

	// 发送 header
	err = stream.Send(&pb.ReadFileStreamRequest{
		Payload: &pb.ReadFileStreamRequest_Header{
			Header: &pb.ReadFileStreamHeader{
				TransferId:  req.TransferId,
				WorkspaceId: sp.config.WorkspaceID,
			},
		},
	})
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("send header: %v", err))
		return
	}

	// 分块读取并流式发送
	buf := make([]byte, chunkSize)
	for {
		n, readErr := f.Read(buf)
		if n > 0 {
			if err := stream.Send(&pb.ReadFileStreamRequest{
				Payload: &pb.ReadFileStreamRequest_Data{Data: buf[:n]},
			}); err != nil {
				sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("send chunk: %v", err))
				return
			}
		}
		if readErr != nil {
			break // EOF 或错误
		}
	}

	// 关闭发送端，等待 Server 确认
	if _, err := stream.CloseAndRecv(); err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("close stream: %v", err))
	}
}

// handleWriteFileTransfer 从 WriteFileStream RPC 接收数据并写入本地文件
func (sp *StorageProvider) handleWriteFileTransfer(ctx context.Context, req *pb.StartDataTransfer) {
	// 通过 openat 安全地打开文件（创建模式）
	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("path guard: %v", err))
		return
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	fd, err := unix.Openat(dirFd, name,
		unix.O_WRONLY|unix.O_CREAT|unix.O_TRUNC|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0644)
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("open file: %v", err))
		return
	}
	f := os.NewFile(uintptr(fd), req.Path)
	defer f.Close()

	// 发起 WriteFileStream RPC
	stream, err := sp.client.WriteFileStream(ctx, &pb.WriteFileStreamRequest{
		TransferId:  req.TransferId,
		WorkspaceId: sp.config.WorkspaceID,
	})
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("open write stream: %v", err))
		return
	}

	// 接收数据并写入本地文件
	for {
		resp, err := stream.Recv()
		if err != nil {
			sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("recv chunk: %v", err))
			return
		}

		switch p := resp.Payload.(type) {
		case *pb.WriteFileStreamResponse_Data:
			if _, err := f.Write(p.Data); err != nil {
				sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("write file: %v", err))
				return
			}
		case *pb.WriteFileStreamResponse_Done:
			// 写入完成，文件将在 defer f.Close() 中关闭
			return
		}
	}
}

func (sp *StorageProvider) sendDataTransferFailed(transferID string, reason string) {
	sp.responseCh <- &pb.ClientMessage{
		Message: &pb.ClientMessage_DataTransferFailed{
			DataTransferFailed: &pb.DataTransferFailed{
				TransferId: transferID,
				Reason:     reason,
			},
		},
	}
}
```

### 5.3 文件操作执行逻辑

**新文件**: `sdk-go/storage_provider_ops.go`

核心实现：将 Server 发来的操作请求分发到对应的本地文件操作函数。所有操作通过 `pathGuard`（openat）确保路径安全。

```go
package sdk

import (
	"fmt"
	"io"
	"os"
	"sync"
	"time"

	"golang.org/x/sys/unix"
	pb "github.com/OpenElevo/ElevoSandbox/proto/workspace/v1"
)

// executeOperation 分发操作请求到对应的处理函数
func (sp *StorageProvider) executeOperation(req *pb.StorageOperationRequest) *pb.ClientMessage {
	var result *pb.StorageOperationResponse

	switch op := req.Operation.(type) {
	case *pb.StorageOperationRequest_Stat:
		result = sp.opStat(req.CorrelationId, op.Stat)
	case *pb.StorageOperationRequest_ListDir:
		result = sp.opListDir(req.CorrelationId, op.ListDir)
	case *pb.StorageOperationRequest_Exists:
		result = sp.opExists(req.CorrelationId, op.Exists)
	case *pb.StorageOperationRequest_ReadFileRange:
		result = sp.opReadFileRange(req.CorrelationId, op.ReadFileRange)
	case *pb.StorageOperationRequest_WriteFileAt:
		result = sp.opWriteFileAt(req.CorrelationId, op.WriteFileAt)
	case *pb.StorageOperationRequest_CreateFile:
		result = sp.opCreateFile(req.CorrelationId, op.CreateFile)
	case *pb.StorageOperationRequest_Mkdir:
		result = sp.opMkdir(req.CorrelationId, op.Mkdir)
	case *pb.StorageOperationRequest_RemoveFile:
		result = sp.opRemoveFile(req.CorrelationId, op.RemoveFile)
	case *pb.StorageOperationRequest_RemoveDir:
		result = sp.opRemoveDir(req.CorrelationId, op.RemoveDir)
	case *pb.StorageOperationRequest_Rename:
		result = sp.opRename(req.CorrelationId, op.Rename)
	case *pb.StorageOperationRequest_Copy:
		result = sp.opCopy(req.CorrelationId, op.Copy)
	case *pb.StorageOperationRequest_SetFileSize:
		result = sp.opSetFileSize(req.CorrelationId, op.SetFileSize)
	case *pb.StorageOperationRequest_SetPermissions:
		result = sp.opSetPermissions(req.CorrelationId, op.SetPermissions)
	case *pb.StorageOperationRequest_SetTimes:
		result = sp.opSetTimes(req.CorrelationId, op.SetTimes)
	case *pb.StorageOperationRequest_Symlink:
		result = sp.opSymlink(req.CorrelationId, op.Symlink)
	case *pb.StorageOperationRequest_ReadLink:
		result = sp.opReadLink(req.CorrelationId, op.ReadLink)
	case *pb.StorageOperationRequest_StatFs:
		result = sp.opStatFs(req.CorrelationId)
	default:
		result = errorResponse(req.CorrelationId,
			pb.StorageErrorCode_STORAGE_ERROR_CODE_NOT_SUPPORTED, "unknown operation")
	}

	return &pb.ClientMessage{
		Message: &pb.ClientMessage_OperationResponse{
			OperationResponse: result,
		},
	}
}

// --- 各操作实现（均通过 pathGuard 的 openat 机制确保路径安全）---

func (sp *StorageProvider) opStat(corrID string, req *pb.StatRequest) *pb.StorageOperationResponse {
	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		return pathErrorResponse(corrID, err)
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	var stat unix.Stat_t
	err = unix.Fstatat(dirFd, name, &stat, unix.AT_SYMLINK_NOFOLLOW)
	if err != nil {
		return osErrorResponse(corrID, err)
	}

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_Stat{
			Stat: statToProto(req.Path, name, &stat),
		},
	})
}

func (sp *StorageProvider) opListDir(corrID string, req *pb.ListDirRequest) *pb.StorageOperationResponse {
	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path + "/.")
	if err != nil {
		return pathErrorResponse(corrID, err)
	}
	// 对于 ListDir，需要打开目标目录本身
	targetFd, err := unix.Openat(dirFd, name,
		unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_DIRECTORY|unix.O_CLOEXEC, 0)
	closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)
	if err != nil {
		return osErrorResponse(corrID, err)
	}

	// 使用 os.NewFile 包装 fd 来调用 ReadDir。
	// 注意：os.NewFile 接管 fd 的生命周期，GC 时会自动关闭 fd。
	// 因此不要再 defer unix.Close(targetFd)，改用 defer f.Close()。
	f := os.NewFile(uintptr(targetFd), req.Path)
	defer f.Close()

	entries, err := f.ReadDir(-1)
	if err != nil {
		return osErrorResponse(corrID, err)
	}

	var statEntries []*pb.FileStatData
	for _, entry := range entries {
		info, err := entry.Info()
		if err != nil {
			continue
		}
		statEntries = append(statEntries, fileInfoToProto(req.Path, entry.Name(), info))
	}

	// 分页返回（每批最多 200 条）
	// 大目录场景：如果条目数超过 200，分批发送，每批使用相同 correlation_id，
	// 最后一批标记 is_last=true。Server 端 PendingRequest::Paged 负责收集。
	const pageSize = 200
	if len(statEntries) <= pageSize {
		return successResponse(corrID, &pb.StorageOperationSuccess{
			Data:   &pb.StorageOperationSuccess_ListDir{ListDir: &pb.ListDirData{Entries: statEntries}},
			IsLast: true,
		})
	}

	// 大目录分页：多个响应使用相同 correlation_id
	for i := 0; i < len(statEntries); i += pageSize {
		end := i + pageSize
		isLast := end >= len(statEntries)
		if end > len(statEntries) {
			end = len(statEntries)
		}
		batch := statEntries[i:end]
		resp := successResponse(corrID, &pb.StorageOperationSuccess{
			Data:   &pb.StorageOperationSuccess_ListDir{ListDir: &pb.ListDirData{Entries: batch}},
			IsLast: isLast,
		})
		// 通过 responseCh 发送到控制流
		sp.responseCh <- &pb.ClientMessage{
			Message: &pb.ClientMessage_OperationResponse{
				OperationResponse: resp,
			},
		}
	}
	// 分页模式下已通过 responseCh 发送，返回 nil 通知 worker 不要再发送
	return nil
}

func (sp *StorageProvider) opReadFileRange(corrID string, req *pb.ReadFileRangeRequest) *pb.StorageOperationResponse {
	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		return pathErrorResponse(corrID, err)
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	fd, err := unix.Openat(dirFd, name, unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
	if err != nil {
		return osErrorResponse(corrID, err)
	}
	defer unix.Close(fd)

	buf := make([]byte, req.Length)
	n, err := unix.Pread(fd, buf, int64(req.Offset))
	if err != nil && err != io.EOF {
		return osErrorResponse(corrID, err)
	}

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_ReadData{ReadData: &pb.ReadData{Data: buf[:n]}},
	})
}

func (sp *StorageProvider) opWriteFileAt(corrID string, req *pb.WriteFileAtRequest) *pb.StorageOperationResponse {
	// 写操作需要获取 per-file 锁
	lock := sp.acquireFileLock(req.Path)
	defer sp.releaseFileLock(req.Path, lock)

	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		return pathErrorResponse(corrID, err)
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	fd, err := unix.Openat(dirFd, name, unix.O_WRONLY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
	if err != nil {
		return osErrorResponse(corrID, err)
	}
	defer unix.Close(fd)

	n, err := unix.Pwrite(fd, req.Data, int64(req.Offset))
	if err != nil {
		return osErrorResponse(corrID, err)
	}

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_WriteData{WriteData: &pb.WriteData{BytesWritten: uint64(n)}},
	})
}

func (sp *StorageProvider) opCreateFile(corrID string, req *pb.CreateFileRequest) *pb.StorageOperationResponse {
	lock := sp.acquireFileLock(req.Path)
	defer sp.releaseFileLock(req.Path, lock)

	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		return pathErrorResponse(corrID, err)
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	flags := unix.O_WRONLY | unix.O_CREAT | unix.O_NOFOLLOW | unix.O_CLOEXEC
	if req.Exclusive {
		flags |= unix.O_EXCL
	}
	fd, err := unix.Openat(dirFd, name, flags, 0644)
	if err != nil {
		return osErrorResponse(corrID, err)
	}
	unix.Close(fd)

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_Empty{Empty: &pb.Empty{}},
	})
}

func (sp *StorageProvider) opMkdir(corrID string, req *pb.MkdirRequest) *pb.StorageOperationResponse {
	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		return pathErrorResponse(corrID, err)
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	err = unix.Mkdirat(dirFd, name, 0755)
	if err != nil {
		return osErrorResponse(corrID, err)
	}

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_Empty{Empty: &pb.Empty{}},
	})
}

func (sp *StorageProvider) opRemoveFile(corrID string, req *pb.RemoveFileRequest) *pb.StorageOperationResponse {
	lock := sp.acquireFileLock(req.Path)
	defer sp.releaseFileLock(req.Path, lock)

	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		return pathErrorResponse(corrID, err)
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	err = unix.Unlinkat(dirFd, name, 0)
	if err != nil {
		return osErrorResponse(corrID, err)
	}

	// 惰性清理 per-file 锁条目
	sp.fileLocks.Delete(req.Path)

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_Empty{Empty: &pb.Empty{}},
	})
}

func (sp *StorageProvider) opRemoveDir(corrID string, req *pb.RemoveDirRequest) *pb.StorageOperationResponse {
	if req.Recursive {
		// 递归删除：通过 openat 安全地逐级遍历删除，
		// 避免字符串拼接 + os.RemoveAll 的 TOCTOU 竞态。
		dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
		if err != nil {
			return pathErrorResponse(corrID, err)
		}
		defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

		// 打开目标目录自身的 fd
		targetFd, err := unix.Openat(dirFd, name,
			unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_DIRECTORY|unix.O_CLOEXEC, 0)
		if err != nil {
			return osErrorResponse(corrID, err)
		}

		// 递归删除目录内容（基于 fd，无 TOCTOU 风险）
		if err := sp.removeAllAt(targetFd); err != nil {
			unix.Close(targetFd)
			return osErrorResponse(corrID, err)
		}
		unix.Close(targetFd)

		// 删除目录本身
		err = unix.Unlinkat(dirFd, name, unix.AT_REMOVEDIR)
		if err != nil {
			return osErrorResponse(corrID, err)
		}
	} else {
		dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
		if err != nil {
			return pathErrorResponse(corrID, err)
		}
		defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

		err = unix.Unlinkat(dirFd, name, unix.AT_REMOVEDIR)
		if err != nil {
			return osErrorResponse(corrID, err)
		}
	}

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_Empty{Empty: &pb.Empty{}},
	})
}

// removeAllAt 基于 fd 安全地递归删除目录内容（不跟踪符号链接）
func (sp *StorageProvider) removeAllAt(dirFd int) error {
	// 使用 os.NewFile 包装 fd（注意：不要 defer Close，调用方管理 fd 生命周期）
	// 使用 Dup 避免 os.File 析构时关闭原始 fd
	dupFd, err := unix.Dup(dirFd)
	if err != nil {
		return err
	}
	f := os.NewFile(uintptr(dupFd), "")
	entries, err := f.ReadDir(-1)
	f.Close() // 关闭 dup 的 fd，不影响原始 dirFd

	if err != nil {
		return err
	}

	for _, entry := range entries {
		name := entry.Name()
		if entry.IsDir() {
			// 递归处理子目录
			childFd, err := unix.Openat(dirFd, name,
				unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_DIRECTORY|unix.O_CLOEXEC, 0)
			if err != nil {
				return err
			}
			if err := sp.removeAllAt(childFd); err != nil {
				unix.Close(childFd)
				return err
			}
			unix.Close(childFd)
			// 删除空子目录
			if err := unix.Unlinkat(dirFd, name, unix.AT_REMOVEDIR); err != nil {
				return err
			}
		} else {
			// 删除文件/符号链接
			if err := unix.Unlinkat(dirFd, name, 0); err != nil {
				return err
			}
		}
	}
	return nil
}

func (sp *StorageProvider) opRename(corrID string, req *pb.RenameRequest) *pb.StorageOperationResponse {
	// rename 需要同时获取 src 和 dst 的锁（按字典序，避免死锁）
	path1, path2 := req.Src, req.Dst
	if path1 > path2 {
		path1, path2 = path2, path1
	}
	lock1 := sp.acquireFileLock(path1)
	lock2 := sp.acquireFileLock(path2)
	defer sp.releaseFileLock(path1, lock1)
	defer sp.releaseFileLock(path2, lock2)

	srcDirFd, srcName, err := sp.pathGuard.OpenParentDir(req.Src)
	if err != nil {
		return pathErrorResponse(corrID, err)
	}
	defer closeFdIfNotRoot(srcDirFd, sp.pathGuard.rootFd)

	dstDirFd, dstName, err := sp.pathGuard.OpenParentDir(req.Dst)
	if err != nil {
		return pathErrorResponse(corrID, err)
	}
	defer closeFdIfNotRoot(dstDirFd, sp.pathGuard.rootFd)

	var flags uint32
	switch req.Flags {
	case 1: // noreplace
		flags = unix.RENAME_NOREPLACE
	case 2: // exchange
		flags = unix.RENAME_EXCHANGE
	default:
		flags = 0
	}

	// renameat2 支持 flags（noreplace, exchange）
	err = unix.Renameat2(srcDirFd, srcName, dstDirFd, dstName, flags)
	if err != nil {
		return osErrorResponse(corrID, err)
	}

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_Empty{Empty: &pb.Empty{}},
	})
}

// opCopy, opSetFileSize, opSetPermissions, opSetTimes, opSymlink, opReadLink, opStatFs
// 实现模式同上：pathGuard.OpenParentDir → 对应系统调用 → 构造响应
// 此处省略具体实现，模式完全一致。

func (sp *StorageProvider) opExists(corrID string, req *pb.ExistsRequest) *pb.StorageOperationResponse {
	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		// 路径校验失败视为不存在
		return successResponse(corrID, &pb.StorageOperationSuccess{
			Data: &pb.StorageOperationSuccess_Exists{Exists: &pb.ExistsData{Exists: false}},
		})
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	var stat unix.Stat_t
	err = unix.Fstatat(dirFd, name, &stat, unix.AT_SYMLINK_NOFOLLOW)
	exists := err == nil

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_Exists{Exists: &pb.ExistsData{Exists: exists}},
	})
}

func (sp *StorageProvider) opStatFs(corrID string) *pb.StorageOperationResponse {
	var statfs unix.Statfs_t
	err := unix.Statfs(sp.config.LocalDir, &statfs)
	if err != nil {
		return osErrorResponse(corrID, err)
	}

	return successResponse(corrID, &pb.StorageOperationSuccess{
		Data: &pb.StorageOperationSuccess_StatFs{StatFs: &pb.StatFsData{
			Blocks:  statfs.Blocks,
			Bfree:   statfs.Bfree,
			Bavail:  statfs.Bavail,
			Files:   statfs.Files,
			Ffree:   statfs.Ffree,
			Bsize:   uint32(statfs.Bsize),
			Namelen: uint32(statfs.Namelen),
			Frsize:  uint32(statfs.Frsize),
		}},
	})
}

// --- per-file 锁管理 ---
//
// 使用 channel-based mutex 替代 sync.Mutex，支持可超时的加锁。
// 每个 fileLock 内部是一个容量为 1 的 channel：空 channel 表示未锁定，
// 写入一个值表示获取锁，读取一个值表示释放锁。
// 这避免了 goroutine 泄漏和死锁问题。

type chanMutex struct {
	ch chan struct{}
}

func newChanMutex() *chanMutex {
	ch := make(chan struct{}, 1)
	ch <- struct{}{} // 初始状态：可获取
	return &chanMutex{ch: ch}
}

func (sp *StorageProvider) acquireFileLock(path string) *chanMutex {
	actual, _ := sp.fileLocks.LoadOrStore(path, newChanMutex())
	mu := actual.(*chanMutex)
	// 带超时的锁获取（10 秒）
	select {
	case <-mu.ch:
		// 成功获取锁（从 channel 中取出 token）
		return mu
	case <-time.After(10 * time.Second):
		// 超时：返回 nil，调用方检查后返回 IO_ERROR
		// 不会泄漏 goroutine，因为没有启动额外 goroutine
		return nil
	}
}

func (sp *StorageProvider) releaseFileLock(path string, mu *chanMutex) {
	if mu != nil {
		// 释放锁：将 token 放回 channel
		mu.ch <- struct{}{}
	}
}

// --- 辅助函数 ---

func closeFdIfNotRoot(fd int, rootFd int) {
	if fd != rootFd {
		unix.Close(fd)
	}
}

func successResponse(corrID string, success *pb.StorageOperationSuccess) *pb.StorageOperationResponse {
	return &pb.StorageOperationResponse{
		CorrelationId: corrID,
		Result: &pb.StorageOperationResponse_Success{Success: success},
	}
}

func errorResponse(corrID string, code pb.StorageErrorCode, msg string) *pb.StorageOperationResponse {
	return &pb.StorageOperationResponse{
		CorrelationId: corrID,
		Result: &pb.StorageOperationResponse_Error{Error: &pb.StorageOperationError{
			Code: code, Message: msg,
		}},
	}
}

func osErrorResponse(corrID string, err error) *pb.StorageOperationResponse {
	code := pb.StorageErrorCode_STORAGE_ERROR_CODE_IO_ERROR
	switch {
	case os.IsNotExist(err) || err == unix.ENOENT:
		code = pb.StorageErrorCode_STORAGE_ERROR_CODE_NOT_FOUND
	case os.IsExist(err) || err == unix.EEXIST:
		code = pb.StorageErrorCode_STORAGE_ERROR_CODE_ALREADY_EXISTS
	case os.IsPermission(err) || err == unix.EACCES:
		code = pb.StorageErrorCode_STORAGE_ERROR_CODE_PERMISSION_DENIED
	case err == unix.EISDIR:
		code = pb.StorageErrorCode_STORAGE_ERROR_CODE_IS_A_DIRECTORY
	case err == unix.ENOTDIR:
		code = pb.StorageErrorCode_STORAGE_ERROR_CODE_NOT_A_DIRECTORY
	case err == unix.ENOTEMPTY:
		code = pb.StorageErrorCode_STORAGE_ERROR_CODE_DIRECTORY_NOT_EMPTY
	}
	return errorResponse(corrID, code, err.Error())
}

func pathErrorResponse(corrID string, err error) *pb.StorageOperationResponse {
	return errorResponse(corrID,
		pb.StorageErrorCode_STORAGE_ERROR_CODE_PATH_TRAVERSAL_DENIED, err.Error())
}
```

### 5.4 路径安全校验（openat）

**新文件**: `sdk-go/storage_provider_path.go`

```go
package sdk

import (
	"fmt"
	"path/filepath"
	"strings"

	"golang.org/x/sys/unix"
)

// pathGuard 基于 openat 的路径安全校验
type pathGuard struct {
	rootPath string
	rootFd   int // 共享目录根的 fd
}

func newPathGuard(rootPath string) (*pathGuard, error) {
	absRoot, err := filepath.Abs(rootPath)
	if err != nil {
		return nil, err
	}
	// 以 O_NOFOLLOW | O_DIRECTORY 打开根目录
	fd, err := unix.Open(absRoot, unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_DIRECTORY|unix.O_CLOEXEC, 0)
	if err != nil {
		return nil, fmt.Errorf("open root dir: %w", err)
	}
	return &pathGuard{rootPath: absRoot, rootFd: fd}, nil
}

func (pg *pathGuard) Close() {
	unix.Close(pg.rootFd)
}

// ValidatePath 第一层：快速路径预校验
func (pg *pathGuard) ValidatePath(relPath string) error {
	cleaned := filepath.Clean(relPath)
	if strings.HasPrefix(cleaned, "..") || strings.Contains(cleaned, "/../") {
		return fmt.Errorf("path traversal denied: %s", relPath)
	}
	return nil
}

// OpenParentDir 第二层：基于 fd 的安全文件操作
// 逐级以 O_NOFOLLOW | O_DIRECTORY 打开路径中的每个目录组件，
// 返回目标文件所在目录的 fd 和文件名。
// 调用方负责关闭返回的 dirFd（除非 dirFd == rootFd，即文件在根目录下）。
func (pg *pathGuard) OpenParentDir(relPath string) (dirFd int, fileName string, err error) {
	// 预校验
	if err := pg.ValidatePath(relPath); err != nil {
		return -1, "", err
	}

	cleaned := filepath.Clean(relPath)
	parts := strings.Split(cleaned, "/")

	// 特殊处理：文件直接在根目录下，无需逐级打开
	if len(parts) == 1 {
		return pg.rootFd, parts[0], nil
	}

	// 逐级 openat，仅跟踪当前 fd（前一个 fd 在拿到下一个后立即关闭）
	currentFd := pg.rootFd

	for i := 0; i < len(parts)-1; i++ {
		nextFd, openErr := unix.Openat(currentFd,
			parts[i],
			unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_DIRECTORY|unix.O_CLOEXEC,
			0,
		)
		if openErr != nil {
			// 关闭当前 fd（如果不是 rootFd）
			if currentFd != pg.rootFd {
				unix.Close(currentFd)
			}
			if openErr == unix.ELOOP {
				return -1, "", fmt.Errorf("path traversal denied (symlink): %s", relPath)
			}
			return -1, "", openErr
		}

		// 拿到 nextFd 后，关闭前一个中间 fd（rootFd 除外，不应关闭）
		if currentFd != pg.rootFd {
			unix.Close(currentFd)
		}
		currentFd = nextFd
	}

	// currentFd 是目标文件的父目录 fd
	// 调用方负责关闭（除非 currentFd == rootFd，但此分支已在上面特殊处理）
	return currentFd, parts[len(parts)-1], nil
}
```

### 5.5 文件变更监听

**新文件**: `sdk-go/storage_provider_watch.go`

```go
package sdk

import (
	"log"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/fsnotify/fsnotify"
	pb "github.com/OpenElevo/ElevoSandbox/proto/workspace/v1"
)

// 默认忽略的目录
var defaultIgnoreDirs = map[string]bool{
	".git": true, "node_modules": true, "__pycache__": true,
	"target": true, "build": true, ".elevo": true,
}

type fileWatcher struct {
	watcher    *fsnotify.Watcher
	rootDir    string
	responseCh chan<- *pb.ClientMessage

	// .elevoignore 忽略规则
	ignoreRules *ignoreRules

	// 事件合并（固定 50ms 窗口 + 200ms 最大延迟）
	pendingEvents map[string]*pb.FileChangeEvent
	mu            sync.Mutex
	timer         *time.Timer
	windowStart   time.Time // 当前合并窗口的起始时间

	// inotify 降级标记：watch 数量耗尽时降级为定期全量缓存清除
	degraded atomic.Bool
}

// ignoreRules 解析 .elevoignore 文件（格式同 .gitignore）
type ignoreRules struct {
	patterns []string
}

func loadIgnoreRules(rootDir string) *ignoreRules {
	rules := &ignoreRules{}
	data, err := os.ReadFile(filepath.Join(rootDir, ".elevoignore"))
	if err != nil {
		return rules // 无 .elevoignore 文件，不忽略任何路径
	}
	for _, line := range strings.Split(string(data), "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		rules.patterns = append(rules.patterns, line)
	}
	return rules
}

func (ir *ignoreRules) shouldIgnore(relPath string) bool {
	for _, pattern := range ir.patterns {
		// 使用 filepath.Match 做 glob 匹配
		if matched, _ := filepath.Match(pattern, filepath.Base(relPath)); matched {
			return true
		}
		// 也检查完整相对路径
		if matched, _ := filepath.Match(pattern, relPath); matched {
			return true
		}
	}
	return false
}

func newFileWatcher(rootDir string, responseCh chan<- *pb.ClientMessage) (*fileWatcher, error) {
	w, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, err
	}

	fw := &fileWatcher{
		watcher:       w,
		rootDir:       rootDir,
		responseCh:    responseCh,
		ignoreRules:   loadIgnoreRules(rootDir),
		pendingEvents: make(map[string]*pb.FileChangeEvent),
	}

	// 检查 inotify watch 数量限制
	watchLimit := fw.checkWatchLimit()

	// 递归添加所有子目录的 watch
	watchCount := 0
	err = filepath.Walk(rootDir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil // 跳过无法访问的目录
		}
		if info.IsDir() {
			name := info.Name()
			if defaultIgnoreDirs[name] {
				return filepath.SkipDir
			}
			// 检查 .elevoignore 规则
			relPath, _ := filepath.Rel(rootDir, path)
			if fw.ignoreRules.shouldIgnore(relPath) {
				return filepath.SkipDir
			}
			// 检查是否接近 inotify 限制
			if watchLimit > 0 && watchCount >= int(watchLimit*80/100) {
				// 已使用超过 80% 的 inotify watch 配额，降级
				log.Printf("warning: approaching inotify watch limit (%d/%d), degrading to periodic cache purge",
					watchCount, watchLimit)
				fw.degraded.Store(true)
				return filepath.SkipAll
			}
			if addErr := w.Add(path); addErr != nil {
				// watch 添加失败（可能是限制耗尽），降级
				log.Printf("warning: failed to add watch for %s: %v, degrading to periodic cache purge",
					path, addErr)
				fw.degraded.Store(true)
				return filepath.SkipAll
			}
			watchCount++
			return nil
		}
		return nil
	})
	if err != nil {
		w.Close()
		return nil, err
	}

	go fw.eventLoop()

	// 如果降级，启动定期全量缓存清除 goroutine（每 5 秒）
	if fw.degraded.Load() {
		go fw.degradedPollLoop()
	}

	return fw, nil
}

// checkWatchLimit 检查 inotify watch 数量限制，返回限制值（0 表示无法读取）
func (fw *fileWatcher) checkWatchLimit() int64 {
	data, err := os.ReadFile("/proc/sys/fs/inotify/max_user_watches")
	if err != nil {
		return 0
	}
	s := strings.TrimSpace(string(data))
	limit, err := strconv.ParseInt(s, 10, 64)
	if err != nil {
		return 0
	}
	if limit < 10000 {
		log.Printf("warning: inotify max_user_watches=%d is low, consider increasing via sysctl", limit)
	}
	return limit
}

// degradedPollLoop 在 inotify 降级模式下，每 5 秒发送一个全量缓存清除通知
func (fw *fileWatcher) degradedPollLoop() {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()
	for range ticker.C {
		fw.responseCh <- &pb.ClientMessage{
			Message: &pb.ClientMessage_FileChanged{
				FileChanged: &pb.FileChangedNotification{
					// 空 events 列表 + full_purge 标记 → Server 端执行全量缓存清除
					FullPurge: true,
				},
			},
		}
	}
}

func (fw *fileWatcher) eventLoop() {
	for {
		select {
		case event, ok := <-fw.watcher.Events:
			if !ok {
				return
			}
			fw.handleEvent(event)

		case err, ok := <-fw.watcher.Errors:
			if !ok {
				return
			}
			log.Printf("fsnotify error: %v", err)
		}
	}
}

func (fw *fileWatcher) handleEvent(event fsnotify.Event) {
	relPath, err := filepath.Rel(fw.rootDir, event.Name)
	if err != nil {
		return
	}

	// 检查 .elevoignore 规则
	if fw.ignoreRules.shouldIgnore(relPath) {
		return
	}

	// 新目录创建时动态添加 watch
	if event.Has(fsnotify.Create) {
		if info, err := os.Stat(event.Name); err == nil && info.IsDir() {
			name := info.Name()
			if !defaultIgnoreDirs[name] && !fw.ignoreRules.shouldIgnore(relPath) {
				fw.watcher.Add(event.Name)
				// 递归添加子目录
			}
		}
	}

	// 事件合并（固定 50ms 窗口）
	// 使用固定窗口而非滑动窗口：第一个事件触发 50ms 计时器，
	// 计时器到期后统一 flush，期间的新事件只累积不重置计时器。
	// 另设 200ms 最大延迟上限，防止高频事件下通知永远不发出。
	fw.mu.Lock()
	defer fw.mu.Unlock()

	changeType := mapFsnotifyOp(event.Op)
	fw.pendingEvents[relPath] = &pb.FileChangeEvent{
		Path:      relPath,
		EventType: changeType,
	}

	if fw.timer == nil {
		// 首个事件，启动固定 50ms 窗口
		fw.windowStart = time.Now()
		fw.timer = time.AfterFunc(50*time.Millisecond, fw.flush)
	} else if time.Since(fw.windowStart) >= 200*time.Millisecond {
		// 已超过最大延迟上限，立即 flush
		// 重要：必须先将 timer 置为 nil，再 Stop()，最后调用 flush。
		// 这避免了 AfterFunc 回调与手动 flush 的竞态：
		// - Stop() 返回 false 时回调可能已经在排队执行
		// - flush 内部检查 timer==nil 来判断是否已被调用过
		fw.timer.Stop()
		fw.timer = nil
		// 直接调用（已持有 mu），而非 go fw.flush()
		// 因为此处在 mu.Lock 内部，而 flush 也需要 mu，
		// 所以使用 flushLocked 避免重入死锁
		fw.flushLocked()
	}
	// 否则：窗口内的新事件，不重置计时器，等待原计时器到期
}

// flush 由 time.AfterFunc 回调调用（不在 mu.Lock 下）
func (fw *fileWatcher) flush() {
	fw.mu.Lock()
	// 防止重复 flush：如果 timer 已被 handleEvent 清除，说明已经 flushLocked 过
	if fw.timer == nil && len(fw.pendingEvents) == 0 {
		fw.mu.Unlock()
		return
	}
	fw.flushLocked()
	fw.mu.Unlock()
}

// flushLocked 在已持有 mu 的情况下执行 flush
func (fw *fileWatcher) flushLocked() {
	events := make([]*pb.FileChangeEvent, 0, len(fw.pendingEvents))
	for _, e := range fw.pendingEvents {
		events = append(events, e)
	}
	fw.pendingEvents = make(map[string]*pb.FileChangeEvent)
	fw.timer = nil

	if len(events) > 0 {
		// 注意：responseCh 发送不应在锁内阻塞。
		// responseCh 有 256 缓冲区，正常情况不会阻塞。
		// 如果 channel 满了（极端背压），此处会阻塞直到空间可用。
		fw.responseCh <- &pb.ClientMessage{
			Message: &pb.ClientMessage_FileChanged{
				FileChanged: &pb.FileChangedNotification{
					Events: events,
				},
			},
		}
	}
}
```

### 5.6 Go SDK Client 集成

**修改文件**: `sdk-go/client.go`

```go
// Client 新增 StorageProvider 工厂方法
func (c *Client) NewStorageProvider(config StorageProviderConfig) *StorageProvider {
	return NewStorageProvider(c.conn, config)
}
```

**修改文件**: `sdk-go/workspace_service.go`

```go
// WorkspaceService 新增方法
func (ws *WorkspaceService) RegisterNfsTransport(
	ctx context.Context,
	workspaceID string,
	nfsURL string,
) (*Workspace, error) {
	// ...
}

func (ws *WorkspaceService) UnregisterNfsTransport(
	ctx context.Context,
	workspaceID string,
) (*Workspace, error) {
	// ...
}
```

---

## 6. Phase 5: NFS 通道

### 6.1 NFS Mount 管理器

**新文件**: `server/src/infra/nfs_mount.rs`

管理 Server 端 mount 远程 NFS 的操作（区别于现有 `NfsManager` 管理的是 Server 端 NFS export）。

```rust
use std::net::IpAddr;
use std::path::PathBuf;

/// NFS 白名单配置
pub struct NfsAllowedConfig {
    /// 允许的 CIDR 列表（默认拒绝，必须显式配置）
    pub allowed_cidrs: Vec<ipnetwork::IpNetwork>,
    /// 允许的端口范围（默认仅 2049）
    pub allowed_ports: Vec<u16>,
}

/// 远程 NFS mount 管理器
pub struct RemoteNfsMountManager {
    base_dir: PathBuf,
    config: NfsAllowedConfig,
}

impl RemoteNfsMountManager {
    pub fn new(base_dir: PathBuf, config: NfsAllowedConfig) -> Self {
        Self { base_dir, config }
    }

    /// 校验 NFS URL 安全性
    ///
    /// NFS URL 格式: `nfs://host[:port]/export/path`
    /// 默认端口: 2049
    pub fn validate_nfs_url(&self, nfs_url: &str) -> Result<(IpAddr, u16, String), Error> {
        // 1. 解析 URL
        let url = url::Url::parse(nfs_url)
            .map_err(|e| Error::Internal(format!("Invalid NFS URL '{}': {}", nfs_url, e)))?;

        if url.scheme() != "nfs" {
            return Err(Error::Internal(format!(
                "Invalid NFS URL scheme '{}', expected 'nfs'", url.scheme()
            )));
        }

        let host_str = url.host_str()
            .ok_or_else(|| Error::Internal("NFS URL missing host".to_string()))?;
        let port = url.port().unwrap_or(2049);
        let export_path = url.path().to_string();

        if export_path.is_empty() || export_path == "/" {
            return Err(Error::Internal("NFS URL missing export path".to_string()));
        }

        // 2. DNS 解析（如果是主机名而非 IP）
        let ip: IpAddr = match host_str.parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => {
                // 主机名 → DNS 解析
                use std::net::ToSocketAddrs;
                let addr = format!("{}:{}", host_str, port)
                    .to_socket_addrs()
                    .map_err(|e| Error::Internal(format!(
                        "Failed to resolve NFS host '{}': {}", host_str, e
                    )))?
                    .next()
                    .ok_or_else(|| Error::Internal(format!(
                        "DNS resolution returned no addresses for '{}'", host_str
                    )))?;
                addr.ip()
            }
        };

        // 3. 检查 IP 是否在白名单 CIDR 内
        let ip_allowed = self.config.allowed_cidrs.iter().any(|cidr| cidr.contains(ip));
        if !ip_allowed {
            return Err(Error::Internal(format!(
                "NFS host IP {} is not in allowed CIDR list", ip
            )));
        }

        // 4. 检查端口是否允许
        if !self.config.allowed_ports.contains(&port) {
            return Err(Error::Internal(format!(
                "NFS port {} is not in allowed ports list", port
            )));
        }

        // 5. 返回验证后的 (ip, port, export_path)
        Ok((ip, port, export_path))
    }

    /// 执行 NFS mount
    pub async fn mount(
        &self,
        workspace_id: &str,
        nfs_url: &str,
        target_suffix: &str,  // "" 为正式路径, ".nfs-pending" 为临时路径
    ) -> anyhow::Result<PathBuf> {
        let (ip, port, export_path) = self.validate_nfs_url(nfs_url)?;

        let mount_path = self.base_dir.join(format!("{}{}", workspace_id, target_suffix));
        tokio::fs::create_dir_all(&mount_path).await?;

        let source = format!("{}:{}", ip, export_path);
        let mount_opts = format!("nosuid,nodev,soft,timeo=300,retry=0,port={}", port);
        let status = tokio::process::Command::new("mount")
            .args([
                "-t", "nfs",
                "-o", &mount_opts,
                &source,
                mount_path.to_str().unwrap(),
            ])
            .status()
            .await?;

        if !status.success() {
            return Err(anyhow::anyhow!("NFS mount failed"));
        }

        // mount 后验证
        let meta = tokio::fs::metadata(&mount_path).await?;
        if !meta.is_dir() {
            self.umount(&mount_path).await?;
            return Err(anyhow::anyhow!("NFS mount verification failed"));
        }

        Ok(mount_path)
    }

    /// 执行 umount
    pub async fn umount(&self, path: &PathBuf) -> anyhow::Result<()> {
        let status = tokio::process::Command::new("umount")
            .arg(path.to_str().unwrap())
            .status()
            .await?;
        if !status.success() {
            // 强制 umount
            tokio::process::Command::new("umount")
                .args(["-l", path.to_str().unwrap()])
                .status()
                .await?;
        }
        Ok(())
    }
}
```

### 6.2 通道切换逻辑

**新文件**: `server/src/service/remote_workspace.rs`

```rust
/// 通道切换服务
pub struct RemoteWorkspaceService {
    workspace_repo: Arc<WorkspaceRepository>,
    storage_router: Arc<StorageRouter>,
    fuse_mount_manager: Arc<FuseMountManager>,
    nfs_mount_manager: Arc<RemoteNfsMountManager>,
}

impl RemoteWorkspaceService {
    /// gRPC → NFS 通道切换
    pub async fn switch_to_nfs(
        &self,
        workspace_id: &str,
        nfs_url: &str,
    ) -> Result<()> {
        // 1. 更新 DB：记录切换开始
        let mut config = self.get_storage_config(workspace_id).await?;
        config.switching_to = Some(RemoteTransport::Nfs);
        config.switch_phase = Some(SwitchPhase::Pending);
        config.nfs_url = Some(nfs_url.to_string());
        self.workspace_repo.update_storage_config(workspace_id, &config).await?;

        // 2. 获取写锁（排空在途操作，超时 60 秒）
        let _guard = self.storage_router
            .write_lock(workspace_id, Duration::from_secs(60))
            .await?;

        // 3. mount NFS 到临时路径
        let temp_mount = match self.nfs_mount_manager
            .mount(workspace_id, nfs_url, ".nfs-pending")
            .await
        {
            Ok(path) => path,
            Err(e) => {
                // 回滚：清除 switching 状态
                config.switching_to = None;
                config.switch_phase = None;
                config.nfs_url = None;
                self.workspace_repo.update_storage_config(workspace_id, &config).await?;
                return Err(e.into());
            }
        };

        // 4. 更新 DB：mount 成功
        config.switch_phase = Some(SwitchPhase::Mounted);
        self.workspace_repo.update_storage_config(workspace_id, &config).await?;

        // 5. umount 原 FUSE
        self.fuse_mount_manager.umount(workspace_id).await?;

        // 6. 将 NFS mount 从临时路径移动到正式路径
        //    不能对活跃的 mount 点执行 rename，必须使用 mount --move。
        //    mount --move 是原子操作，不会产生不可用窗口。
        let final_path = self.nfs_mount_manager.base_dir.join(workspace_id);
        let status = tokio::process::Command::new("mount")
            .args(["--move", temp_mount.to_str().unwrap(), final_path.to_str().unwrap()])
            .status()
            .await?;
        if !status.success() {
            // mount --move 失败时的回退策略：umount 临时路径 → 重新 mount 到正式路径。
            // 此回退路径会产生一个短暂的不可用窗口（FUSE 已卸载，NFS 尚未挂载到正式路径）。
            // 但由于步骤 2 已获取 write_lock，所有文件操作都在等待锁释放，
            // 因此该窗口对调用方不可见——不会有操作在此期间到达存储后端。
            tracing::warn!(
                "mount --move failed for workspace {}, falling back to umount+remount (gap covered by write lock)",
                workspace_id
            );
            self.nfs_mount_manager.umount(&temp_mount).await?;
            self.nfs_mount_manager.mount(workspace_id, nfs_url, "").await?;
        }

        // 7. 替换 StorageRouter 中的后端
        // 使用独立的阻塞线程池，与全局默认后端隔离（设计文档 3.4 节要求）。
        // NFS 操作可能因网络延迟而阻塞较长时间，独立线程池防止影响本地 workspace。
        let nfs_blocking_pool = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .thread_name(format!("nfs-{}", workspace_id))
                .build()
                .expect("failed to create NFS blocking pool")
        );
        let nfs_backend = Arc::new(LocalStorageBackend::with_blocking_pool(
            self.nfs_mount_manager.base_dir.clone(),
            nfs_blocking_pool,
        ));
        self.storage_router.replace_backend(workspace_id, nfs_backend);

        // 8. 更新 DB：切换完成
        config.transport = RemoteTransport::Nfs;
        config.switching_to = None;
        config.switch_phase = None;
        self.workspace_repo.update_storage_config(workspace_id, &config).await?;

        Ok(())
    }

    /// Server 启动时恢复未完成的切换
    pub async fn recover_pending_switches(&self) -> Result<()> {
        let workspaces = self.workspace_repo.list_remote().await?;
        for ws in workspaces {
            if ws.storage_config.switching_to.is_some() {
                match ws.storage_config.switch_phase {
                    Some(SwitchPhase::Pending) => {
                        // 回滚：尝试 umount 临时路径，清除 switching 状态
                        let temp_path = self.nfs_mount_manager.base_dir
                            .join(format!("{}.nfs-pending", ws.id));
                        let _ = self.nfs_mount_manager.umount(&temp_path).await;
                        let mut config = ws.storage_config.clone();
                        config.switching_to = None;
                        config.switch_phase = None;
                        config.nfs_url = None;
                        self.workspace_repo.update_storage_config(&ws.id, &config).await?;
                    }
                    Some(SwitchPhase::Mounted) => {
                        // 继续完成切换（step 5-8）
                        // ...
                    }
                    None => {}
                }
            }
        }
        Ok(())
    }
}
```

---

## 7. Phase 6: 可靠性

### 7.1 Server 启动恢复流程

**修改文件**: `server/src/main.rs`

在 Server 启动时，增加 remote workspace 恢复逻辑：

```rust
async fn main() -> anyhow::Result<()> {
    // ... 现有初始化 ...

    // 初始化 remote workspace 基础设施
    let remote_storage_pool = Arc::new(RemoteStoragePool::new(storage_router.clone()));
    let fuse_mount_manager = Arc::new(FuseMountManager::new(workspace_dir.clone()));
    let nfs_mount_manager = Arc::new(RemoteNfsMountManager::new(workspace_dir.clone(), nfs_config));
    let remote_workspace_service = Arc::new(RemoteWorkspaceService::new(
        workspace_repository.clone(),
        storage_router.clone(),
        fuse_mount_manager.clone(),
        nfs_mount_manager.clone(),
    ));

    // Server 启动恢复
    // 1. 恢复未完成的通道切换
    remote_workspace_service.recover_pending_switches().await?;

    // 2. 恢复 NFS 通道的 remote workspace
    let remote_workspaces = workspace_repository.list_remote().await?;
    for ws in &remote_workspaces {
        if ws.storage_config.transport == RemoteTransport::Nfs {
            if let Some(ref nfs_url) = ws.storage_config.nfs_url {
                match nfs_mount_manager.mount(&ws.id, nfs_url, "").await {
                    Ok(_) => {
                        // 使用独立阻塞线程池（与通道切换逻辑一致）
                        let nfs_pool = Arc::new(
                            tokio::runtime::Builder::new_multi_thread()
                                .worker_threads(4)
                                .thread_name(format!("nfs-{}", ws.id))
                                .build()
                                .expect("failed to create NFS blocking pool")
                        );
                        let backend = Arc::new(LocalStorageBackend::with_blocking_pool(
                            workspace_dir.clone(), nfs_pool,
                        ));
                        storage_router.register(&ws.id, backend);
                        tracing::info!("recovered NFS mount for workspace {}", ws.id);
                    }
                    Err(e) => {
                        tracing::error!("failed to recover NFS mount for {}: {}", ws.id, e);
                    }
                }
            }
        }
        // gRPC 通道的 workspace：等待 Client 重连后再创建 FUSE 挂载
    }

    // 3. 启动 FUSE 健康监控 task
    let fuse_mgr = fuse_mount_manager.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let unhealthy = fuse_mgr.health_check().await;
            for ws_id in unhealthy {
                tracing::warn!("FUSE mount unhealthy for {}, rebuilding", ws_id);
                // 强制 umount + 重建
            }
        }
    });

    // 4. 启动 NFS mount 健康监控（复用 S3fsMountMonitor 模式）
    // ...

    // ... 启动 gRPC/HTTP server ...
}
```

### 7.2 Sandbox 自动恢复

**修改文件**: `server/src/service/sandbox.rs` 或新增文件

当 remote workspace 的存储恢复后（Client 重连 + FUSE 重建），需要自动重启关联的 Sandbox 容器。

```rust
/// 在 remote workspace 存储恢复后调用
pub async fn restart_associated_sandboxes(
    &self,
    workspace_id: &str,
) -> Result<()> {
    // 查询该 workspace 关联的所有 sandbox
    let sandboxes = self.sandbox_repo.list_by_workspace(workspace_id).await?;

    for sandbox in sandboxes {
        if sandbox.state == SandboxState::Running {
            tracing::info!(
                "restarting sandbox {} after workspace {} storage recovery",
                sandbox.id, workspace_id
            );
            // docker restart
            self.docker.restart_container(&sandbox.container_id).await?;
        }
    }

    Ok(())
}
```

---

## 8. Phase 7: 可观测性

### 8.1 Metrics

**修改文件**: `server/src/infra/metrics.rs`

新增 remote storage 相关指标。使用 `metrics` crate（与现有代码一致）。

```rust
use metrics::{counter, gauge, histogram};

// 操作延迟
pub fn record_remote_op_duration(workspace_id: &str, operation: &str, transport: &str, duration: Duration) {
    histogram!(
        "remote_storage_operation_duration_seconds",
        "workspace_id" => workspace_id.to_string(),
        "operation" => operation.to_string(),
        "transport" => transport.to_string(),
    )
    .record(duration.as_secs_f64());
}

// 操作计数
pub fn increment_remote_op(workspace_id: &str, operation: &str, transport: &str, status: &str) {
    counter!(
        "remote_storage_operation_total",
        "workspace_id" => workspace_id.to_string(),
        "operation" => operation.to_string(),
        "transport" => transport.to_string(),
        "status" => status.to_string(),
    )
    .increment(1);
}

// 缓存命中/未命中
pub fn increment_cache_hit(workspace_id: &str, cache_type: &str) {
    counter!(
        "remote_storage_cache_hit_total",
        "workspace_id" => workspace_id.to_string(),
        "cache_type" => cache_type.to_string(),
    )
    .increment(1);
}

pub fn increment_cache_miss(workspace_id: &str, cache_type: &str) {
    counter!(
        "remote_storage_cache_miss_total",
        "workspace_id" => workspace_id.to_string(),
        "cache_type" => cache_type.to_string(),
    )
    .increment(1);
}

// pending 请求数
pub fn set_pending_requests(workspace_id: &str, count: usize) {
    gauge!(
        "remote_storage_pending_requests",
        "workspace_id" => workspace_id.to_string(),
    )
    .set(count as f64);
}

// 连接状态
pub fn set_connection_state(workspace_id: &str, state: u8) {
    gauge!(
        "remote_storage_client_connection_state",
        "workspace_id" => workspace_id.to_string(),
    )
    .set(state as f64);
}

// FUSE 健康状态
pub fn set_fuse_health(workspace_id: &str, healthy: bool) {
    gauge!(
        "remote_storage_fuse_mount_healthy",
        "workspace_id" => workspace_id.to_string(),
    )
    .set(if healthy { 1.0 } else { 0.0 });
}

// 数据传输字节数
pub fn increment_data_transfer(workspace_id: &str, direction: &str, bytes: u64) {
    counter!(
        "remote_storage_data_transfer_bytes_total",
        "workspace_id" => workspace_id.to_string(),
        "direction" => direction.to_string(),
    )
    .increment(bytes);
}

// 文件变更通知数
pub fn increment_file_change_notifications(workspace_id: &str) {
    counter!(
        "remote_storage_file_change_notifications_total",
        "workspace_id" => workspace_id.to_string(),
    )
    .increment(1);
}
```

### 8.2 Metrics 埋点位置

| 指标 | 埋点位置 |
|------|----------|
| `operation_duration_seconds` | `RemoteStorageBackend::send_request` 方法返回前 |
| `operation_total` | `RemoteStorageBackend::send_request` 方法返回前 |
| `cache_hit/miss` | `fuse-core` 的 `MetadataCache` 和 `DirCache` 查询时 |
| `pending_requests` | `RemoteStorageBackend::send_request` 中信号量获取后/释放前更新 |
| `connection_state` | `RemoteStorageBackend::bind_stream` / `unbind_stream` 时更新 |
| `fuse_mount_healthy` | `FuseMountManager::health_check` 时更新 |
| `data_transfer_bytes` | 数据流 RPC handler 中每个 chunk 传输后累加 |
| `file_change_notifications` | `ClientStorageServiceImpl::connect` 中收到 `FileChanged` 消息时 |

### 8.3 Metrics 标签基数控制

`workspace_id` 作为 Prometheus label 在 workspace 数量较多时会导致时间序列爆炸（高基数问题）。采取以下策略：

- **聚合指标**（`operation_duration_seconds`、`operation_total`）：仅使用 `operation` 和 `transport` 作为标签，不包含 `workspace_id`。用于全局性能监控和告警
- **诊断指标**（`pending_requests`、`connection_state`、`fuse_mount_healthy`）：保留 `workspace_id` 标签，因为这些是 per-workspace 状态指标，数量受 `max_remote_workspaces` 限制（默认 200），时间序列上限可控
- **计数器**（`cache_hit/miss`、`data_transfer_bytes`、`file_change_notifications`）：仅使用 `transport` 和 `cache_type`/`direction` 标签。per-workspace 的细粒度数据通过结构化日志输出，不进入 Prometheus

如果需要 per-workspace 的操作延迟分析，通过日志中的 `workspace_id` 字段配合日志分析工具（如 Loki）查询。

---

## 9. 配置变更汇总

### 9.1 新增环境变量

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `WORKSPACE_MAX_REMOTE_WORKSPACES` | int | 200 | 单台 Server 最大 remote workspace 数量 |
| `WORKSPACE_REMOTE_OP_TIMEOUT_SECS` | int | 30 | gRPC 通道文件操作超时（秒） |
| `WORKSPACE_REMOTE_HEARTBEAT_INTERVAL_SECS` | int | 15 | 心跳间隔（秒） |
| `WORKSPACE_REMOTE_HEARTBEAT_TIMEOUT_SECS` | int | 45 | 心跳超时（秒） |
| `WORKSPACE_REMOTE_DATA_STREAM_THRESHOLD` | int | 65536 | 控制流/数据流切分阈值（字节） |
| `WORKSPACE_REMOTE_MAX_CONCURRENT_REQUESTS` | int | 128 | Server 端单 workspace 最大并发请求数 |
| `WORKSPACE_NFS_ALLOWED_CIDRS` | string | (空) | NFS 白名单 CIDR 列表，逗号分隔。空表示拒绝所有 NFS 注册 |
| `WORKSPACE_FUSE_ENTRY_TIMEOUT_SECS` | int | 1 | FUSE entry 缓存 TTL（秒） |
| `WORKSPACE_FUSE_ATTR_TIMEOUT_SECS` | int | 1 | FUSE attr 缓存 TTL（秒） |

### 9.2 Config 结构体变更

**修改文件**: `server/src/config.rs`

```rust
pub struct Config {
    // ... 现有字段 ...

    // Remote storage 配置
    pub max_remote_workspaces: usize,
    pub remote_op_timeout_secs: u64,
    pub remote_heartbeat_interval_secs: u64,
    pub remote_heartbeat_timeout_secs: u64,
    pub remote_data_stream_threshold: usize,
    pub remote_max_concurrent_requests: usize,
    pub nfs_allowed_cidrs: Vec<String>,
    pub fuse_entry_timeout_secs: u64,
    pub fuse_attr_timeout_secs: u64,
}
```

---

## 10. 测试计划

### 10.1 单元测试

| 测试 | 位置 | 覆盖内容 |
|------|------|----------|
| StorageRouter 路由 | `server/src/infra/storage/router.rs` | 默认后端 fallback、per-workspace 覆盖、注册/注销 |
| RemoteStorageBackend 超时 | `server/src/infra/storage/remote.rs` | 请求超时返回 EIO、断线时清理 pending 请求 |
| RemoteStorageBackend 反压 | `server/src/infra/storage/remote.rs` | 信号量满时阻塞新请求 |
| 错误码映射 | `server/src/infra/storage/remote.rs` | StorageErrorCode ↔ StorageError 双向转换 |
| 路径安全校验 | `sdk-go/storage_provider_path_test.go` | `..` 拒绝、符号链接拒绝、正常路径通过 |
| per-file 写锁 | `sdk-go/storage_provider_test.go` | 并发写串行化、rename 双锁顺序、delete 清理 |
| 事件合并 | `sdk-go/storage_provider_watch_test.go` | 50ms 窗口内同路径事件合并 |
| NFS URL 校验 | `server/src/infra/nfs_mount.rs` | 白名单拒绝、DNS rebinding 防护、端口限制 |
| storage_config 序列化 | `server/src/domain/workspace.rs` | JSON 序列化/反序列化、版本号校验 |
| 通道切换状态机 | `server/src/service/remote_workspace.rs` | 正常切换、mount 失败回滚、崩溃恢复 |

### 10.2 集成测试

| 测试 | 说明 |
|------|------|
| 端到端 gRPC 通道 | 创建 remote workspace → Go SDK 连接 → 消费方通过 FUSE 读写文件 → 验证数据一致性 |
| Client 断线重连 | 建立连接 → 强制断线 → 验证操作超时返回 EIO → 重连 → 验证操作恢复 |
| 大文件数据流 | 通过 gRPC 通道读写 > 64KB 的文件 → 验证数据流 RPC 正确触发 → 验证数据完整性 |
| 文件变更通知 | Client 本地修改文件 → 验证 Server 端缓存失效 → 消费方读取到最新数据 |
| NFS 通道切换 | gRPC → NFS 切换 → 验证文件操作正常 → NFS → gRPC 切回 → 验证文件操作正常 |
| Server 重启恢复 | 创建 remote workspace → Server 重启 → Client 重连 → 验证 FUSE 重建 → 验证 Sandbox 重启 |
| 通道切换崩溃恢复 | 模拟切换中途崩溃 → Server 重启 → 验证状态机恢复或回滚 |
| 容量限制 | 创建 max_remote_workspaces + 1 个 remote workspace → 验证返回 RESOURCE_EXHAUSTED |
| 路径穿越防护 | Server 发送包含 `../` 的路径 → 验证 Client 返回 PATH_TRAVERSAL_DENIED |
| 并发通道切换 | 两个请求同时切换同一 workspace 的通道 → 验证写锁互斥，仅一个成功 |
| FUSE panic 恢复 | 模拟 FUSE 线程 panic → 验证 `catch_unwind` 捕获 → 健康检查发现异常 → FUSE 重建 |
| inotify 限制降级 | 模拟 inotify watch 数量耗尽 → 验证降级为定期全量缓存清除（每 5 秒） |
| 数据流 transfer_id 过期 | Client 延迟发起数据流 RPC（超过 10 秒）→ 验证 Server 返回 NOT_FOUND |
| 大目录 ListDir 分页 | 创建超过 200 条目的目录 → 验证 Client 正确分页发送 → Server 正确合并 |
| 符号链接路径穿越 | 在共享目录内创建指向目录外的符号链接 → 验证 O_NOFOLLOW 拒绝穿越 |
| 认证失败 | 使用错误 token 连接 → 验证握手失败，不泄露 workspace 信息 |

### 10.3 性能测试

| 测试 | 指标 | 目标 |
|------|------|------|
| stat 延迟 | 单次 stat 的 P50/P95 | 同机房 < 5ms, 跨网络 < 50ms |
| 批量 stat | 1000 次 stat 总耗时 | 同机房 < 2s（利用并发） |
| 大文件吞吐 | 100MB 文件读写吞吐量 | 接近网络带宽上限 |
| 缓存命中率 | 重复 stat 同一文件 | 缓存命中率 > 95% |
| 并发写入 | 10 并发写不同文件 | 无数据损坏 |

### 10.4 新增 Cargo 依赖

**`server/Cargo.toml`**:
```toml
[dependencies]
# ... 现有依赖 ...
ipnetwork = "0.20"         # NFS CIDR 白名单校验
fuser = "0.14"             # Server 端 FUSE 挂载（如果 fuse-core 不包含）
```

**`sdk-go/go.mod`**:
```
require (
    // ... 现有依赖 ...
    github.com/fsnotify/fsnotify v1.7.0
    golang.org/x/sys v0.20.0    // openat 等系统调用
)
```

---

## 附录 A: 文件变更清单

### 新增文件

| 文件 | Phase | 说明 |
|------|-------|------|
| `server/migrations/20260310000000_add_remote_storage.sql` | 1 | DB 迁移 |
| `proto/workspace/v1/client_storage.proto` | 1 | ClientStorageService proto |
| `server/src/infra/storage/router.rs` | 1 | StorageRouter |
| `server/src/infra/storage/remote.rs` | 2 | RemoteStorageBackend |
| `server/src/api/grpc/client_storage.rs` | 2 | ClientStorageServiceImpl |
| `fuse-core/` | 3 | 共享 FUSE 库 |
| `server/src/infra/fuse_mount.rs` | 3 | FuseMountManager |
| `server/src/infra/nfs_mount.rs` | 5 | RemoteNfsMountManager |
| `server/src/service/remote_workspace.rs` | 5 | 通道切换逻辑 |
| `sdk-go/storage_provider.go` | 4 | StorageProvider 核心 |
| `sdk-go/storage_provider_ops.go` | 4 | 文件操作执行 |
| `sdk-go/storage_provider_path.go` | 4 | openat 路径安全 |
| `sdk-go/storage_provider_watch.go` | 4 | fsnotify 监听 |
| `sdk-go/storage_provider_test.go` | 4 | 单元测试 |

### 修改文件

| 文件 | Phase | 变更 |
|------|-------|------|
| `server/src/domain/workspace.rs` | 1 | 新增 StorageType, RemoteStorageConfig |
| `server/src/infra/workspace_repository.rs` | 1 | 新增字段查询、update_storage_config |
| `server/src/infra/storage/mod.rs` | 1 | 注册 router 模块 |
| `server/src/main.rs` | 1, 2, 6 | StorageRouter 接线、ClientStorageService 注册、启动恢复 |
| `server/src/config.rs` | 1 | 新增 remote 相关配置项 |
| `workspace-proto/build.rs` | 1 | 增加 client_storage.proto 编译步骤 |
| `server/src/api/grpc/mod.rs` | 2 | 注册 client_storage 模块 |
| `server/src/infra/metrics.rs` | 7 | 新增 remote storage metrics |
| `proto/workspace/v1/workspace.proto` | 1 | 扩展 CreateWorkspaceRequest、Workspace 消息 |
| `server/src/api/grpc/workspace.rs` | 1, 5 | 处理 storage_type、注册/注销 NFS |
| `server/src/service/workspace.rs` | 1 | remote workspace 创建逻辑（跳过本地目录创建） |
| `sdk-go/client.go` | 4 | 新增 NewStorageProvider |
| `sdk-go/workspace_service.go` | 4, 5 | storage_type 参数、NFS 注册方法 |
| `sdk-go/types.go` | 4 | Workspace 结构体新增 StorageType 等字段 |
| `fuse-client/src/fuse_fs.rs` | 3 | 提取逻辑到 fuse-core |
| `fuse-client/src/inode.rs` | 3 | 提取到 fuse-core |
| `fuse-client/src/cache.rs` | 3 | 提取到 fuse-core |
| `server/Cargo.toml` | 1-7 | 新增依赖 |
| `sdk-go/go.mod` | 4 | 新增依赖 |
