# FUSE Client 设计方案

## 1. 背景与目标

### 1.1 问题

当前 Workspace Server 对外提供两种文件访问接口：

1. **HTTP REST API**：支持 `read/write/list/mkdir/delete/move/copy`，但无法满足 POSIX 文件系统语义（无 `open/seek/stat/chmod` 等），不能被开发工具直接当作本地文件系统使用
2. **NFS**：支持 POSIX 语义，但 mount 数量增大后性能退化（每个 sandbox 一个 NFS export），且 NFS 协议本身在跨广域网场景下表现不佳

### 1.2 目标

提供一个 **FUSE 客户端工具** (`workspace-fuse`)，让用户可以：

- 将远程 workspace 挂载为本地目录，完整支持 POSIX 文件操作
- **非 root 用户** 即可挂载
- 按 **workspace 粒度** 独立挂载（每个 workspace 一个挂载点）
- 支持标准开发工具链（IDE、编译器、git 等）直接操作

> ⚠️ **重要限制**：同一 workspace 同一时间建议只由**单个 client 进行写入操作**。多 client 同时写入可能导致数据不一致或丢失，因为当前设计不包含分布式锁或冲突检测机制。多 client 只读访问是安全的。

```bash
# 典型用法
workspace-fuse mount \
  --server http://workspace-server:9090 \
  --workspace ws_abc123 \
  --token <API_KEY> \
  --target ~/projects/my-workspace

# 卸载
workspace-fuse umount ~/projects/my-workspace
# 或者
fusermount -u ~/projects/my-workspace
```

## 2. 整体架构

```
┌───────────────────────────────────────────────┐
│              Client Machine                    │
│                                                │
│   Application (IDE / gcc / git / ...)          │
│       │  POSIX syscall (open, read, write...) │
│       ▼                                        │
│   ┌─────────────┐                              │
│   │  Linux VFS  │                              │
│   └──────┬──────┘                              │
│          │  /dev/fuse                          │
│          ▼                                      │
│   ┌──────────────────┐                          │
│   │  workspace-fuse   │  (用户态 FUSE daemon)   │
│   │                    │                         │
│   │  ┌──────────────┐ │                         │
│   │  │   Cache Layer │ │  元数据缓存 + 读缓存   │
│   │  └──────┬───────┘ │                         │
│   │         │          │                         │
│   │  ┌──────▼───────┐ │                         │
│   │  │  gRPC Client  │ │  流式读写 + Token 认证 │
│   │  └──────┬───────┘ │                         │
│   └─────────┼──────────┘                         │
│             │  gRPC / HTTP2                      │
└─────────────┼────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────┐
│           Workspace Server                       │
│                                                  │
│   gRPC FileSystemService (Token 认证拦截器)      │
│       │                                          │
│       ▼                                          │
│   StorageBackend                                 │
│       └── LocalStorageBackend                    │
│           (local 磁盘 或 s3fs-fuse 挂载点)      │
└─────────────────────────────────────────────────┘
```

## 3. 传输协议：gRPC FileService

### 3.1 为什么选 gRPC

| 对比项 | gRPC | HTTP REST | 自定义 TCP |
|--------|------|-----------|-----------|
| 流式传输 | 原生支持（stream） | 不支持（chunked transfer 有限） | 需自研 |
| 大文件读写 | streaming 分块传输 | 单次请求体大小受限 | 需自研分包 |
| 连接复用 | HTTP/2 多路复用 | HTTP/1.1 keep-alive | 需自研 |
| 类型安全 | protobuf 强类型 | JSON 弱类型 | 需自研 |
| 开发成本 | 中等（已有 tonic 基础） | 低 | 高 |
| 已有基础 | Server 已有 gRPC + tonic | Server 已有 HTTP API | 无 |

### 3.2 Proto 定义

新增文件：`proto/workspace/v1/filesystem.proto`

```protobuf
syntax = "proto3";

package workspace.v1;

option go_package = "github.com/OpenElevo/ElevoSandbox/proto/workspace/v1";

import "google/protobuf/timestamp.proto";

// 文件系统服务 - 为 FUSE 客户端提供完整的 POSIX 语义文件操作
//
// 认证：所有 RPC 调用需在 gRPC metadata 中携带 "authorization" = "Bearer <token>"，
// 由 Server 端 AuthInterceptor 统一校验（见 3.5 节）。
service FileSystemService {
    // 获取文件/目录元信息
    rpc Stat(FsStatRequest) returns (FsStatResponse);

    // 读取文件内容（流式，支持大文件）
    rpc ReadFile(FsReadFileRequest) returns (stream FsReadFileResponse);

    // 写入文件内容（流式，支持大文件）
    rpc WriteFile(stream FsWriteFileRequest) returns (FsWriteFileResponse);

    // 列出目录内容（流式，支持大目录）
    rpc ListDir(FsListDirRequest) returns (stream FsListDirResponse);

    // 创建目录
    rpc Mkdir(FsMkdirRequest) returns (FsMkdirResponse);

    // 删除文件（对应 FUSE unlink / POSIX unlink）
    // 如果目标是目录，返回 EISDIR 错误
    rpc RemoveFile(FsRemoveFileRequest) returns (FsRemoveFileResponse);

    // 删除目录（对应 FUSE rmdir / POSIX rmdir）
    // 如果目标是文件，返回 ENOTDIR 错误
    rpc RemoveDir(FsRemoveDirRequest) returns (FsRemoveDirResponse);

    // 重命名/移动文件或目录
    rpc Rename(FsRenameRequest) returns (FsRenameResponse);

    // 创建文件
    rpc Create(FsCreateRequest) returns (FsCreateResponse);

    // 设置文件属性（大小、权限等）
    rpc SetAttr(FsSetAttrRequest) returns (FsSetAttrResponse);

    // 创建符号链接
    rpc Symlink(FsSymlinkRequest) returns (FsSymlinkResponse);

    // 读取符号链接目标
    rpc ReadLink(FsReadLinkRequest) returns (FsReadLinkResponse);

    // 基于 offset 的随机读取（FUSE read 回调使用）
    rpc ReadAt(FsReadAtRequest) returns (FsReadAtResponse);

    // 基于 offset 的随机写入（FUSE write 回调使用）
    rpc WriteAt(FsWriteAtRequest) returns (FsWriteAtResponse);

    // 获取文件系统统计信息（对应 FUSE statfs / POSIX statvfs）
    rpc StatFs(FsStatFsRequest) returns (FsStatFsResponse);
}

// 文件属性信息（对应 FUSE fattr）
message FsFileAttr {
    // 文件类型
    FsFileType file_type = 1;

    // 文件大小（字节）
    uint64 size = 2;

    // 实际占用的 512 字节块数（对应 POSIX st_blocks）
    // 对于稀疏文件，blocks * 512 可能小于 size
    uint64 blocks = 3;

    // Unix 权限模式
    uint32 mode = 4;

    // 硬链接数
    uint32 nlink = 5;

    // 所有者 UID（Server 端原始值）
    uint32 uid = 6;

    // 所有者 GID（Server 端原始值）
    uint32 gid = 7;

    // 最后访问时间
    google.protobuf.Timestamp atime = 8;

    // 最后修改时间
    google.protobuf.Timestamp mtime = 9;

    // 状态变更时间
    google.protobuf.Timestamp ctime = 10;
}

// 注意：uid/gid 由 Server 返回原始存储值，Client 端在转换为 FUSE FileAttr 时
// 统一映射为运行 workspace-fuse 的当前用户的 uid/gid，确保文件权限检查正常工作。
// 详见 4.12 节「uid/gid 映射」。

// 文件类型枚举
enum FsFileType {
    FS_FILE_TYPE_UNSPECIFIED = 0;
    FS_FILE_TYPE_FILE = 1;
    FS_FILE_TYPE_DIRECTORY = 2;
    FS_FILE_TYPE_SYMLINK = 3;
}

// 目录项
message FsDirEntry {
    string name = 1;
    FsFileAttr attr = 2;
}

// --- Stat ---
message FsStatRequest {
    string workspace_id = 1;
    string path = 2;
}

message FsStatResponse {
    FsFileAttr attr = 1;
}

// --- ReadFile (流式) ---
message FsReadFileRequest {
    string workspace_id = 1;
    string path = 2;
}

message FsReadFileResponse {
    bytes data = 1;    // 每个 chunk 最大 64KB
    bool eof = 2;
}

// --- WriteFile (流式) ---
message FsWriteFileRequest {
    // 第一个消息必须包含 header
    oneof payload {
        FsWriteFileHeader header = 1;
        bytes data = 2;
    }
}

message FsWriteFileHeader {
    string workspace_id = 1;
    string path = 2;
    bool truncate = 3;  // 是否清空已有内容
}

message FsWriteFileResponse {
    uint64 bytes_written = 1;
}

// --- ReadAt (随机读) ---
message FsReadAtRequest {
    string workspace_id = 1;
    string path = 2;
    uint64 offset = 3;
    uint32 size = 4;     // 最大读取字节数
}

message FsReadAtResponse {
    bytes data = 1;
    // eof = true 表示已到文件末尾。
    // 注：也可通过 len(data) < requested_size 判断，保留此字段是为了语义明确。
    bool eof = 2;
}

// --- WriteAt (随机写) ---
message FsWriteAtRequest {
    string workspace_id = 1;
    string path = 2;
    uint64 offset = 3;
    bytes data = 4;
}

message FsWriteAtResponse {
    uint64 bytes_written = 1;
}

// --- ListDir (流式) ---
message FsListDirRequest {
    string workspace_id = 1;
    string path = 2;
}

// 每个 response 消息包含一批目录项，避免单条消息过大
// Server 端按 batch_size（默认 100 条）分批发送
// 每个 FsDirEntry 包含完整 FsFileAttr（约 100-200 bytes），100 条约 10-20KB，远低于 gRPC 默认 4MB 限制
message FsListDirResponse {
    repeated FsDirEntry entries = 1;
}

// --- Mkdir ---
message FsMkdirRequest {
    string workspace_id = 1;
    string path = 2;
    uint32 mode = 3;
}

message FsMkdirResponse {
    FsFileAttr attr = 1;
}

// --- RemoveFile (删除文件) ---
message FsRemoveFileRequest {
    string workspace_id = 1;
    string path = 2;
}

message FsRemoveFileResponse {}

// --- RemoveDir (删除目录) ---
message FsRemoveDirRequest {
    string workspace_id = 1;
    string path = 2;
    // recursive = true 时递归删除目录及所有内容（用于 WorkspaceService 的 delete 场景）
    // recursive = false 时目录必须为空（对应 FUSE rmdir 语义）
    bool recursive = 3;
}

message FsRemoveDirResponse {}

// --- Rename ---
// rename flags 常量，对应 Linux renameat2(2) 的 flags（位掩码）
// 使用 uint32 而非 enum，便于未来扩展（如 RENAME_WHITEOUT = 4）
// 常量定义（供参考，实际值在代码中定义）：
//   RENAME_NOREPLACE = 1  // 目标已存在时返回错误
//   RENAME_EXCHANGE  = 2  // 原子交换 source 和 destination

message FsRenameRequest {
    string workspace_id = 1;
    string source = 2;
    string destination = 3;
    // rename flags 位掩码，0 表示默认行为（覆盖已存在的目标）
    // 支持的 flags: RENAME_NOREPLACE (1), RENAME_EXCHANGE (2)
    uint32 flags = 4;
}

message FsRenameResponse {}

// --- Create ---
message FsCreateRequest {
    string workspace_id = 1;
    string path = 2;
    uint32 mode = 3;
    // exclusive = true 时文件已存在则返回错误（对应 O_EXCL）
    bool exclusive = 4;
}

message FsCreateResponse {
    FsFileAttr attr = 1;
}

// --- SetAttr ---
message FsSetAttrRequest {
    string workspace_id = 1;
    string path = 2;
    optional uint64 size = 3;     // truncate to this size
    optional uint32 mode = 4;     // chmod
    optional uint32 uid = 5;
    optional uint32 gid = 6;
    optional google.protobuf.Timestamp atime = 7;
    optional google.protobuf.Timestamp mtime = 8;
}

message FsSetAttrResponse {
    FsFileAttr attr = 1;
}

// --- Symlink ---
message FsSymlinkRequest {
    string workspace_id = 1;
    string link_path = 2;
    string target = 3;
}

message FsSymlinkResponse {
    FsFileAttr attr = 1;
}

// --- ReadLink ---
message FsReadLinkRequest {
    string workspace_id = 1;
    string path = 2;
}

message FsReadLinkResponse {
    string target = 1;
}

// --- StatFs ---
message FsStatFsRequest {
    string workspace_id = 1;
}

// 文件系统统计信息（对应 POSIX statvfs）
message FsStatFsResponse {
    uint64 total_bytes = 1;     // 总容量
    uint64 free_bytes = 2;      // 可用容量
    uint64 available_bytes = 3; // 非特权用户可用容量
    uint64 total_inodes = 4;    // 总 inode 数
    uint64 free_inodes = 5;     // 可用 inode 数
    uint32 block_size = 6;      // 文件系统块大小
    uint32 max_name_length = 7; // 最大文件名长度
}
```

#### 3.2.1 Proto 设计说明

**为什么 RemoveFile 和 RemoveDir 拆分为两个 RPC**：

POSIX 语义中 `unlink`（删除文件）和 `rmdir`（删除目录）是不同的系统调用，FUSE 也有独立的 `unlink` 和 `rmdir` 回调。错误语义不同：

- 对目录调用 `unlink` 应返回 `EISDIR`
- 对文件调用 `rmdir` 应返回 `ENOTDIR`

这与 `StorageBackend` trait 中 `remove_file` / `remove_dir` 的拆分保持一致。如果合并为单个 `Remove` RPC，Server 端需要先 stat 判断文件类型再路由到正确的 backend 方法，增加不必要的额外开销。

**为什么 ListDir 使用 server streaming**：

目录可能包含大量文件（如 `node_modules` 目录动辄上万文件）。单条 gRPC 消息有默认 4MB 大小限制，一次性返回所有条目可能超限。改为 server streaming 后：

- Server 按批次发送（默认每批 100 条，每个 FsDirEntry 约 100-200 bytes，100 条约 10-20KB）
- Client 可以边接收边构建本地 readdir 缓存，降低首次响应延迟
- 与 ReadFile 的 streaming 模式保持一致

**为什么需要 StatFs**：

`df`、IDE（VSCode、JetBrains）、文件管理器等工具会调用 `statvfs` 获取文件系统容量信息。不实现 `statfs` 回调会导致这些工具报错或显示异常。Server 端实现时：

- Local 模式：直接调用 `statvfs` 获取磁盘信息
- S3 模式：返回 S3 bucket 配额信息或合理的默认值

**为什么 FsRenameRequest 需要 flags**：

FUSE 的 `rename` 回调包含 `flags` 参数（Linux 4.2+ 内核通过 `renameat2` 系统调用传递）。常用的 flags：

- `RENAME_NOREPLACE` (1)：如果目标已存在则失败，而非默认的原子覆盖。部分文件系统工具依赖此语义
- `RENAME_EXCHANGE` (2)：原子交换两个路径的内容
- `RENAME_WHITEOUT` (4)：创建 whiteout 对象（overlay 文件系统使用，当前不支持）

使用 `uint32` 位掩码而非 enum，便于未来扩展新的 flags 而无需修改 proto 定义。

Server 端 `StorageBackend::rename` 当前不支持 flags 时，可在 gRPC 层做降级处理（NOREPLACE → stat + rename, EXCHANGE → 返回 ENOSYS）。

### 3.3 Server 端实现

在 Server 中新增 gRPC service 实现：`server/src/api/grpc/filesystem.rs`

```rust
pub struct FileSystemServiceImpl {
    storage: Arc<dyn StorageBackend>,
}

#[tonic::async_trait]
impl FileSystemService for FileSystemServiceImpl {
    // 每个方法调用 self.storage 的对应方法
    // ReadFile 和 WriteFile 使用 streaming
    // ListDir 使用 server streaming，按批次发送（每批 100 条）
    // ReadAt/WriteAt 直接调用 storage.read_file_range/write_file_at
    // RemoveFile → storage.remove_file()
    // RemoveDir → storage.remove_dir(recursive)
    // StatFs → 调用系统 statvfs() 获取挂载点信息
}
```

在 `main.rs` 中注册到 gRPC server（通过认证拦截器包装）：
```rust
use tonic::service::interceptor;

let auth_interceptor = AuthInterceptor::new(config.api_token.clone());

Server::builder()
    .add_service(agent_grpc_server)
    .add_service(
        FileSystemServiceServer::with_interceptor(
            filesystem_service,
            auth_interceptor,
        )
    )
    .serve_with_shutdown(grpc_addr, shutdown_signal())
```

### 3.4 与现有 WorkspaceService proto 的关系

现有 `workspace.proto` 中已有 `ReadFile`、`WriteFile` 等 RPC 定义，但它们是为管理 API 设计的（全量读写、无 offset 支持）。新增的 `FileSystemService` 是为 FUSE 客户端设计的，有以下区别：

| 特性 | WorkspaceService (现有) | FileSystemService (新增) |
|------|----------------------|------------------------|
| 读写方式 | 全量 | 支持 offset 随机读写 |
| 大文件 | 受 gRPC message size 限制 | streaming 分块 |
| 元数据 | 仅 name/size/type/mtime | 完整 POSIX attr (mode/uid/gid/nlink/atime/ctime) |
| 用途 | HTTP 网关 / 管理 API | FUSE 文件系统后端 |

两者共用同一个 `StorageBackend` 实例，数据一致。

### 3.5 认证机制

`FileSystemService` 面向网络暴露，必须有认证机制防止未授权访问。采用 Token-based (API Key) 方案。

#### 3.5.1 认证流程

```
workspace-fuse                          Workspace Server
     │                                        │
     │  gRPC request                          │
     │  metadata: {"authorization":           │
     │    "Bearer <API_KEY>"}                 │
     │ ─────────────────────────────────────→ │
     │                                        │  AuthInterceptor
     │                                        │  校验 token 有效性
     │                                        │
     │  ← 200 OK / 401 UNAUTHENTICATED ───── │
```

#### 3.5.2 Server 端实现

```rust
/// gRPC 认证拦截器
pub struct AuthInterceptor {
    /// 有效的 API token（启动时从配置加载）
    valid_token: String,
}

impl tonic::service::Interceptor for AuthInterceptor {
    fn call(&mut self, request: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        let token = request.metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        match token {
            Some(t) if t == self.valid_token => Ok(request),
            _ => Err(tonic::Status::unauthenticated("invalid or missing token")),
        }
    }
}
```

#### 3.5.3 配置

新增环境变量：

| 环境变量 | 说明 | 默认值 |
|---------|------|--------|
| `WORKSPACE_FS_API_TOKEN` | FileSystemService 的 API token | 必填（S3/FUSE 模式时） |

- Server 启动时，如果 `WORKSPACE_FS_API_TOKEN` 未设置，`FileSystemService` 不注册到 gRPC server（降级为不可用），避免无认证暴露
- Token 由运维人员在部署时生成（推荐 `openssl rand -hex 32`）
- 客户端通过 `--token` 参数或 `WORKSPACE_FUSE_TOKEN` 环境变量传入

> ⚠️ **安全警告**：Token 通过 gRPC metadata 传输。**生产环境必须使用 TLS（`https://`）**，否则 Token 将明文传输，存在被窃取风险。开发环境可使用 `http://`，但需确保网络隔离。

#### 3.5.4 Client 端实现

```rust
/// 在每个 gRPC 请求中附加 token
impl FileSystemRpcClient {
    pub async fn connect(server_url: &str, token: &str) -> anyhow::Result<Self> {
        let token: MetadataValue<_> = format!("Bearer {}", token).parse()?;
        let channel = Channel::from_shared(server_url.to_string())?
            .connect()
            .await?;
        let client = FileSystemServiceClient::with_interceptor(
            channel,
            move |mut req: tonic::Request<()>| {
                req.metadata_mut().insert("authorization", token.clone());
                Ok(req)
            },
        );
        Ok(Self { client })
    }
}
```

## 4. FUSE Client 设计

### 4.1 项目结构

新增 workspace 成员 crate：

```
fuse-client/
├── Cargo.toml
└── src/
    ├── main.rs           # CLI 入口
    ├── cli.rs            # 命令行参数解析
    ├── fuse_fs.rs        # fuser::Filesystem trait 实现
    ├── rpc.rs            # gRPC 客户端封装
    ├── inode.rs          # Inode ↔ Path 映射管理
    └── cache.rs          # 元数据和读缓存
```

### 4.2 依赖

```toml
[package]
name = "workspace-fuse"
version.workspace = true
edition.workspace = true

[dependencies]
workspace-proto = { path = "../workspace-proto" }  # 共享 proto 生成代码（见 5.1 节）
fuser = "0.16"
tonic = { workspace = true }
prost = { workspace = true }
prost-types = { workspace = true }
tokio = { workspace = true }
bytes = { workspace = true }
clap = { version = "4", features = ["derive"] }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
moka = { version = "0.12", features = ["sync"] }  # 高性能并发缓存
libc = { workspace = true }
anyhow = { workspace = true }
```

### 4.3 CLI 设计

```
workspace-fuse - FUSE client for Elevo Workspace

USAGE:
    workspace-fuse mount [OPTIONS] --server <URL> --workspace <ID> --target <PATH>
    workspace-fuse umount <PATH>

SUBCOMMANDS:
    mount     挂载远程 workspace 到本地目录
    umount    卸载已挂载的 workspace

MOUNT OPTIONS:
    -s, --server <URL>         Workspace Server gRPC 地址 (如 http://server:9090)
    -w, --workspace <ID>       要挂载的 workspace ID
    -t, --target <PATH>        本地挂载点路径
    --token <TOKEN>            API 认证 token（不推荐，会在 ps 中可见）
    --token-file <PATH>        从文件读取 token（推荐）
    --token-stdin              从 stdin 读取 token
    -f, --foreground           前台运行（调试用，默认后台 daemon）
    --cache-ttl <SECS>         元数据缓存 TTL 秒数 (默认 5)
    --read-cache-size <MB>     读缓存大小 MB (默认 128)
    --allow-other              允许其他用户访问挂载点
    --read-only                只读挂载

ENVIRONMENT VARIABLES:
    WORKSPACE_FUSE_TOKEN       API 认证 token（推荐，优先级高于命令行参数）
```

> **安全提示**：Token 传递方式优先级（从高到低）：
> 1. `WORKSPACE_FUSE_TOKEN` 环境变量（推荐，不会在进程列表中暴露）
> 2. `--token-file` 从文件读取（推荐，适合自动化场景）
> 3. `--token-stdin` 从标准输入读取（适合管道传递）
> 4. `--token` 命令行参数（不推荐，会在 `ps aux` 中可见）

> 注意：`--server` 使用 `http://` 或 `https://` scheme，这是 tonic gRPC 客户端要求的 URI 格式（底层使用 HTTP/2 传输）。不要使用 `grpc://`。**生产环境强烈建议使用 `https://`**。

### 4.4 非 Root 挂载

Linux FUSE 支持非 root 用户挂载，需满足以下条件：

1. 用户对 `/dev/fuse` 有读写权限（通常 `fuse` 用户组成员即可）
2. 挂载点目录用户有写权限
3. 如需 `--allow-other`，需要 `/etc/fuse.conf` 中启用 `user_allow_other`

`workspace-fuse` 不需要 root 权限运行。`fuser` crate 的 `mount2` 函数在用户有 `/dev/fuse` 权限时即可工作。

```rust
// 挂载时自动设置正确的选项
let mut options = vec![
    MountOption::FSName(format!("workspace:{}", workspace_id)),
    MountOption::AutoUnmount,
    MountOption::DefaultPermissions,
];
if allow_other {
    options.push(MountOption::AllowOther);
}
if read_only {
    options.push(MountOption::RO);
}

fuser::mount2(fuse_fs, &mount_point, &options)?;
```

### 4.5 Inode 管理

FUSE 协议使用 64-bit inode 编号标识文件，需要维护 inode ↔ path 的双向映射。

> 注意：此处使用 `std::sync::RwLock`（而非 `tokio::sync::RwLock`），因为 `fuser::Filesystem` 回调是同步的，在 FUSE 工作线程中调用，不在 async 上下文中。

```rust
use std::sync::RwLock;

/// Inode ↔ Path 双向映射表
///
/// 设计说明：
/// - 使用单一 RwLock 保护两个 HashMap，避免多锁导致的死锁风险
/// - 所有路径在存入前需要规范化（去除 `.`、`..`、多余斜杠）
pub struct InodeTable {
    next_inode: AtomicU64,
    /// 内部状态，单一锁保护避免死锁
    inner: RwLock<InodeTableInner>,
}

struct InodeTableInner {
    /// path → inode
    path_to_inode: HashMap<String, u64>,
    /// inode → path
    inode_to_path: HashMap<u64, String>,
}

impl InodeTable {
    pub fn new() -> Self {
        let mut inner = InodeTableInner {
            path_to_inode: HashMap::new(),
            inode_to_path: HashMap::new(),
        };
        // 注册根目录（空字符串表示根路径）
        inner.path_to_inode.insert(String::new(), fuser::FUSE_ROOT_ID);
        inner.inode_to_path.insert(fuser::FUSE_ROOT_ID, String::new());

        Self {
            next_inode: AtomicU64::new(2),  // fuser::FUSE_ROOT_ID = 1
            inner: RwLock::new(inner),
        }
    }

    /// 规范化路径：去除 `.`、`..`、多余斜杠、首尾斜杠
    ///
    /// 例如：
    /// - "./foo/../bar" → "bar"
    /// - "/foo/bar/" → "foo/bar"
    /// - "foo//bar" → "foo/bar"
    fn normalize_path(path: &str) -> String {
        let mut components: Vec<&str> = Vec::new();
        for part in path.split('/') {
            match part {
                "" | "." => continue,
                ".." => { components.pop(); }
                _ => components.push(part),
            }
        }
        components.join("/")
    }

    /// 获取 path 对应的 inode，不存在则分配新的
    ///
    /// 整个 check + insert 在写锁内完成，避免两个线程为同一 path 分配不同 inode 的竞态。
    pub fn get_or_create(&self, path: &str) -> u64 {
        let normalized = Self::normalize_path(path);

        // 先尝试读锁快速路径
        {
            let inner = self.inner.read().unwrap();
            if let Some(&ino) = inner.path_to_inode.get(&normalized) {
                return ino;
            }
        }
        // 读锁未命中，获取写锁后再次检查（double-check）
        let mut inner = self.inner.write().unwrap();
        if let Some(&ino) = inner.path_to_inode.get(&normalized) {
            return ino;
        }
        let ino = self.next_inode.fetch_add(1, Ordering::Relaxed);
        inner.path_to_inode.insert(normalized.clone(), ino);
        inner.inode_to_path.insert(ino, normalized);
        ino
    }

    /// 根据 inode 查找 path
    pub fn get_path(&self, inode: u64) -> Option<String> {
        let inner = self.inner.read().unwrap();
        inner.inode_to_path.get(&inode).cloned()
    }

    /// 删除 inode 条目（文件/目录删除时）
    ///
    /// 如果 path 是目录，同时删除所有子路径的映射，避免内存泄漏。
    pub fn remove_by_path(&self, path: &str) {
        let normalized = Self::normalize_path(path);
        let mut inner = self.inner.write().unwrap();

        // 收集需要删除的路径（path 自身 + 所有以 path/ 为前缀的子路径）
        let prefix = format!("{}/", normalized);
        let to_remove: Vec<String> = inner.path_to_inode.keys()
            .filter(|p| *p == &normalized || p.starts_with(&prefix))
            .cloned()
            .collect();

        for p in to_remove {
            if let Some(ino) = inner.path_to_inode.remove(&p) {
                inner.inode_to_path.remove(&ino);
            }
        }
    }

    /// 重命名（rename 操作时更新映射）
    ///
    /// 如果 old_path 是目录，需要同时更新所有子路径的映射（递归 rename）。
    /// 如果 new_path 已存在（POSIX rename 覆盖语义），先删除旧映射。
    pub fn rename(&self, old_path: &str, new_path: &str) {
        let old_normalized = Self::normalize_path(old_path);
        let new_normalized = Self::normalize_path(new_path);
        let mut inner = self.inner.write().unwrap();

        // POSIX rename 语义：如果 new_path 已存在，先删除它（及其子路径）
        let new_prefix = format!("{}/", new_normalized);
        let to_remove: Vec<String> = inner.path_to_inode.keys()
            .filter(|p| *p == &new_normalized || p.starts_with(&new_prefix))
            .cloned()
            .collect();
        for p in to_remove {
            if let Some(ino) = inner.path_to_inode.remove(&p) {
                inner.inode_to_path.remove(&ino);
            }
        }

        // 收集需要更新的路径（old_path 自身 + 所有以 old_path/ 为前缀的子路径）
        let old_prefix = format!("{}/", old_normalized);
        let to_rename: Vec<(String, u64)> = inner.path_to_inode.iter()
            .filter(|(p, _)| *p == &old_normalized || p.starts_with(&old_prefix))
            .map(|(p, &ino)| (p.clone(), ino))
            .collect();

        for (old_p, ino) in to_rename {
            inner.path_to_inode.remove(&old_p);
            let new_p = if old_p == old_normalized {
                new_normalized.clone()
            } else {
                format!("{}{}", new_normalized, &old_p[old_normalized.len()..])
            };
            inner.path_to_inode.insert(new_p.clone(), ino);
            inner.inode_to_path.insert(ino, new_p);
        }
    }
}
```

### 4.6 缓存策略

网络 FUSE 的主要瓶颈是每次 syscall 都触发网络请求。缓存是性能的关键。

#### 4.6.1 元数据缓存

```rust
use moka::sync::Cache;

pub struct MetadataCache {
    /// path → FsFileAttr
    /// moka 内置 TTL 和并发安全，无需额外 RwLock
    cache: Cache<String, FsFileAttr>,
}

impl MetadataCache {
    pub fn new(ttl: Duration, max_capacity: u64) -> Self {
        let cache = Cache::builder()
            .time_to_live(ttl)
            .max_capacity(max_capacity)
            .build();
        Self { cache }
    }

    pub fn get(&self, path: &str) -> Option<FsFileAttr> {
        self.cache.get(path)
    }

    pub fn put(&self, path: &str, attr: FsFileAttr) {
        self.cache.insert(path.to_string(), attr);
    }

    pub fn invalidate(&self, path: &str) {
        self.cache.invalidate(path);
    }

    /// 失效指定路径及其所有子路径（目录删除/重命名时使用）
    pub fn invalidate_prefix(&self, prefix: &str) {
        let prefix_owned = prefix.to_string();
        let prefix_with_slash = format!("{}/", prefix);
        // 失效 prefix 自身
        self.cache.invalidate(&prefix_owned);
        // 使用 invalidate_entries_if 遍历失效所有子路径
        self.cache.invalidate_entries_if(move |k, _| k.starts_with(&prefix_with_slash));
    }
}
```

- `lookup`、`getattr` 操作先查缓存
- 缓存命中且未过期则直接返回
- 写操作（create/write/setattr/remove/rename）时主动失效相关缓存条目
- 默认 TTL = 5 秒（可配置）

#### 4.6.2 目录内容缓存

```rust
pub struct DirCache {
    /// path → Vec<DirEntry>
    cache: Cache<String, Vec<FsDirEntry>>,
}

impl DirCache {
    pub fn new(ttl: Duration, max_capacity: u64) -> Self {
        let cache = Cache::builder()
            .time_to_live(ttl)
            .max_capacity(max_capacity)
            .build();
        Self { cache }
    }
}
```

- `readdir` 操作先查缓存
- 该目录下发生 create/remove/rename 时失效
- TTL 与元数据缓存一致

#### 4.6.3 读数据缓存

```rust
use bytes::Bytes;

pub struct ReadCache {
    /// (path, block_index) → data
    /// block_size = 64KB（与 gRPC streaming chunk 大小一致，对小文件友好）
    cache: Cache<(String, u64), Bytes>,
    block_size: usize,
}

impl ReadCache {
    pub fn new(max_size_bytes: u64, block_size: usize) -> Self {
        // moka 使用条目数作为容量，这里用字节数估算条目数
        let max_entries = max_size_bytes / block_size as u64;
        let cache = Cache::builder()
            .max_capacity(max_entries)
            // 读缓存不设 TTL，依赖写操作主动失效
            .build();
        Self { cache, block_size }
    }

    /// 获取指定文件的 block 数据
    pub fn get(&self, path: &str, block_index: u64) -> Option<Bytes> {
        self.cache.get(&(path.to_string(), block_index))
    }

    /// 缓存指定文件的 block 数据
    pub fn put(&self, path: &str, block_index: u64, data: Bytes) {
        self.cache.insert((path.to_string(), block_index), data);
    }

    /// 失效指定文件的所有 block 缓存
    pub fn invalidate_file(&self, path: &str) {
        let path_owned = path.to_string();
        self.cache.invalidate_entries_if(move |(p, _block_idx), _| p == &path_owned);
    }

    /// 获取 block 大小
    pub fn block_size(&self) -> usize {
        self.block_size
    }
}
```

> **为什么选择 moka**：`moka` 是高性能并发缓存库，内部使用分片锁，支持无锁读取。相比 `LruCache + RwLock` 的组合，`moka` 在高并发场景下（如 IDE 同时打开多个文件）性能显著更好。`moka` 还内置 TTL 支持，简化了过期逻辑。

- FUSE `read(ino, offset, size)` 先检查 block 缓存
- 缓存 miss 时向 server 发送 `ReadAt` 请求（一次请求 64KB = 一个 block）
- 顺序读检测：如果连续读取同一文件的相邻 block，预读下一个 block
- 文件被 write 或 truncate 时失效该文件的所有 block 缓存
- `ReadAt` 的 `size` 参数设为 `block_size`（64KB），与 gRPC streaming 的 64KB/chunk 一致

#### 4.6.4 写缓冲

写操作不缓存，直接通过 `WriteAt` 写到 server。原因：
- FUSE `write` 回调需要同步返回写入字节数
- 缓冲写会增加数据丢失风险（client 崩溃时丢缓冲数据）
- server 端（特别是 S3 后端）已有自己的写优化策略

如果后续发现写性能不足，可以考虑增加 write-back 缓存，但需配合 `fsync` 实现来保证数据安全。

### 4.7 Filesystem Trait 实现

核心结构：

```rust
pub struct WorkspaceFuse {
    workspace_id: String,
    /// Tokio runtime，用于在同步 FUSE 回调中执行异步 gRPC 调用
    runtime: tokio::runtime::Runtime,
    /// gRPC 客户端（Arc 包装以便在 block_on 中使用，避免借用冲突）
    rpc: Arc<FileSystemRpcClient>,
    inodes: InodeTable,
    meta_cache: MetadataCache,
    dir_cache: DirCache,
    read_cache: ReadCache,
    /// 文件句柄计数器（FUSE open 返回的 fh 标识）
    next_fh: AtomicU64,
    /// 文件句柄表：fh → (inode, path, has_written)
    /// 用于 release 时知道哪个文件被关闭，以及是否需要失效缓存
    fh_table: RwLock<HashMap<u64, FileHandle>>,
    /// 缓存 TTL 配置
    cache_ttl: Duration,
    /// 预读状态：path → 上次读取的 block_index（用于顺序读检测）
    readahead_state: RwLock<HashMap<String, u64>>,
}

/// 文件句柄信息
struct FileHandle {
    /// 对应的 inode
    ino: u64,
    /// 文件路径
    path: String,
    /// 是否有过写操作
    has_written: bool,
    /// 打开时的 flags
    flags: u32,
}
```

关键方法实现思路：

#### `open(ino, flags)` / `release(ino, fh)`

```
open:
1. 从 InodeTable 获取 path
2. 分配 fh = next_fh.fetch_add(1)
3. 在 fh_table 中记录 fh → FileHandle { ino, path, has_written: false, flags }
4. 不向 server 发送请求（server 端接口是无状态的 path-based）
5. 返回 (fh, flags)

release:
1. 从 fh_table 中移除 fh，获取 FileHandle
2. 如果 has_written == true，失效该文件的 MetadataCache（size 可能变化）
3. 清理该文件的 readahead_state
4. 不向 server 发送请求
```

> 设计说明：由于 gRPC 的 `ReadAt`/`WriteAt` 是基于 `(workspace_id, path)` 的无状态操作，server 端不维护文件打开状态。`open`/`release` 纯粹在客户端管理，fh 仅用于 FUSE 内核层标识，不影响 server 通信。

#### `lookup(parent, name)`

```
1. 从 InodeTable 获取 parent 的 path
2. 拼接 child_path = parent_path + "/" + name
3. 查 MetadataCache
4. 未命中则调用 rpc.stat(workspace_id, child_path)
5. 分配/获取 inode
6. 缓存结果
7. 返回 (TTL, attr)
```

#### `getattr(ino)`

```
1. 从 InodeTable 获取 path
2. 查 MetadataCache
3. 未命中则调用 rpc.stat(workspace_id, path)
4. 返回 attr
```

#### `read(ino, fh, offset, size)`

```
1. 从 InodeTable 获取 path
2. 计算涉及的 block 范围：start_block = offset / block_size, end_block = (offset + size - 1) / block_size
3. 逐个 block 查 ReadCache
4. 未命中的 block 调用 rpc.read_at(workspace_id, path, block_offset, block_size)
5. 顺序读检测与预读：
   - 检查 readahead_state[path] 是否等于 start_block - 1
   - 如果是顺序读，异步预读 end_block + 1（不阻塞当前请求）
   - 更新 readahead_state[path] = end_block
6. 拼接返回请求的 [offset, offset+size) 范围数据
```

#### `write(ino, fh, offset, data)`

```
1. 从 fh_table 获取 FileHandle，标记 has_written = true
2. 从 InodeTable 获取 path
3. 调用 rpc.write_at(workspace_id, path, offset, data)
4. 失效该文件的 ReadCache
5. 更新 MetadataCache 中的 size（如果 offset + data.len > old_size）
6. 返回 data.len()
```

#### `readdir(ino, fh, offset)`

```
1. 从 InodeTable 获取 path
2. 查 DirCache
3. 未命中则调用 rpc.list_dir(workspace_id, path)（收集所有 streaming 批次）
4. 为每个 entry 分配 inode 并缓存 metadata
5. 按 offset 返回 entries
```

#### `create(parent, name, mode)`

```
1. 从 InodeTable 获取 parent path
2. child_path = parent_path + "/" + name
3. 调用 rpc.create(workspace_id, child_path, mode, exclusive=false)
4. 分配 inode
5. 分配 fh = next_fh.fetch_add(1)
6. 在 fh_table 中记录 fh → FileHandle { ino, path: child_path, has_written: false, flags }
7. 缓存 metadata
8. 失效 parent 的 DirCache
9. 返回 (TTL, attr, generation=0, fh, flags)
```

#### `unlink(parent, name)` / `rmdir(parent, name)`

```
unlink:
1. child_path = parent_path + "/" + name
2. 调用 rpc.remove_file(workspace_id, child_path)
3. 从 InodeTable 移除 child_path
4. 失效 child_path 的 MetadataCache 和 ReadCache
5. 失效 parent 的 DirCache

rmdir:
1. child_path = parent_path + "/" + name
2. 调用 rpc.remove_dir(workspace_id, child_path, recursive=false)
3. 从 InodeTable 移除 child_path 及其所有子路径
4. 失效相关 cache
5. 失效 parent 的 DirCache
```

#### `statfs(ino)`

```
1. 查 statfs_cache（TTL 30 秒）
2. 未命中则调用 rpc.stat_fs(workspace_id)
3. 缓存结果
4. 转换为 fuser::ReplyStatfs 格式返回
```

> 注意：statfs 结果缓存 30 秒，避免 `df` 或 IDE 频繁调用导致的性能问题。

### 4.8 不支持的 POSIX 操作

以下 FUSE 回调明确不支持，返回 `libc::ENOSYS`：

| FUSE 回调 | 对应 POSIX 操作 | 不支持原因 |
|-----------|----------------|-----------|
| `link` | hard link | `StorageBackend` 不支持 hard link；S3 后端也不支持 |
| `flock` | 文件锁 (advisory lock) | 网络文件系统的文件锁需要分布式锁协调，当前不实现 |
| `setlk` / `getlk` | POSIX record lock | 同上 |
| `getxattr` / `setxattr` / `listxattr` / `removexattr` | 扩展属性 | `StorageBackend` 不支持 xattr；S3 metadata 语义不同 |
| `mknod` | 创建设备文件/FIFO | workspace 场景不需要设备文件 |

> 返回 `ENOSYS` 后 FUSE 内核层会记住该操作不支持，后续调用直接返回错误而不再调用用户态 daemon，无性能影响。
> 编辑器（vim、emacs、VSCode）在 `flock` 返回 `ENOSYS` 时会 fallback 到无锁模式正常工作。

### 4.9 gRPC Client 封装

```rust
pub struct FileSystemRpcClient {
    client: FileSystemServiceClient<InterceptedService<Channel, AuthInterceptor>>,
}

impl FileSystemRpcClient {
    pub async fn connect(server_url: &str, token: &str) -> anyhow::Result<Self> {
        let token: MetadataValue<_> = format!("Bearer {}", token).parse()?;
        let channel = Channel::from_shared(server_url.to_string())?
            .connect()
            .await?;
        let client = FileSystemServiceClient::with_interceptor(
            channel,
            move |mut req: tonic::Request<()>| {
                req.metadata_mut().insert("authorization", token.clone());
                Ok(req)
            },
        );
        Ok(Self { client })
    }

    pub async fn stat(&self, workspace_id: &str, path: &str) -> Result<FsFileAttr> { ... }
    pub async fn read_at(&self, workspace_id: &str, path: &str, offset: u64, size: u32) -> Result<(Vec<u8>, bool)> { ... }
    pub async fn write_at(&self, workspace_id: &str, path: &str, offset: u64, data: &[u8]) -> Result<u64> { ... }
    /// 流式接收所有批次，拼接为完整列表
    pub async fn list_dir(&self, workspace_id: &str, path: &str) -> Result<Vec<FsDirEntry>> { ... }
    pub async fn mkdir(&self, workspace_id: &str, path: &str, mode: u32) -> Result<FsFileAttr> { ... }
    pub async fn remove_file(&self, workspace_id: &str, path: &str) -> Result<()> { ... }
    pub async fn remove_dir(&self, workspace_id: &str, path: &str, recursive: bool) -> Result<()> { ... }
    pub async fn rename(&self, workspace_id: &str, src: &str, dst: &str, flags: u32) -> Result<()> { ... }
    pub async fn create(&self, workspace_id: &str, path: &str, mode: u32, exclusive: bool) -> Result<FsFileAttr> { ... }
    pub async fn set_attr(&self, workspace_id: &str, path: &str, attr: SetAttrParams) -> Result<FsFileAttr> { ... }
    pub async fn stat_fs(&self, workspace_id: &str) -> Result<FsStatFsResponse> { ... }
    // ...
}
```

### 4.10 异步适配

`fuser::Filesystem` trait 的方法是同步的（在 FUSE 工作线程中调用），但 gRPC 调用是异步的。需要在同步方法中 block on 异步 future。

方案：使用独立的 Tokio runtime，在 FUSE 回调中 `runtime.block_on()` 调用异步方法。

```rust
impl fuser::Filesystem for WorkspaceFuse {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        // 需要先 clone Arc 引用，避免 block_on 借用 self.runtime 的同时
        // 异步闭包再借用 self 的其他字段（borrow conflict）
        let rpc = Arc::clone(&self.rpc);
        let workspace_id = self.workspace_id.clone();
        let path = self.inodes.resolve_child(parent, name);

        let result = self.runtime.block_on(async {
            rpc.stat(&workspace_id, &path).await
        });
        match result {
            Ok(attr) => {
                let ino = self.inodes.get_or_create(&path);
                self.meta_cache.put(&path, &attr);
                reply.entry(&self.cache_ttl(), &to_file_attr(ino, &attr), 0);
            }
            Err(e) => reply.error(e.to_errno()),
        }
    }
    // ...
}
```

> **关键实现细节**：`self.runtime.block_on()` 不可变借用 `self.runtime`，如果异步闭包内再通过 `self` 访问 `rpc`、`inodes` 等字段，会与 `&mut self`（Filesystem 方法签名）产生借用冲突。解决方法是在调用 `block_on` 之前将所需字段 clone/Arc::clone 到局部变量。
>
> `fuser::mount2` 默认使用**单线程**处理 FUSE 请求。如需并发处理多个 FUSE 请求（如 IDE 同时读取多个文件），应使用 `fuser::Session::new` + `session.run()` 配合 FUSE 的 `-o max_background=...` 选项，或在 `fuser::MountOption` 中传入 `CUSTOM("max_read=131072")` 等调优参数。Tokio runtime 使用多线程调度器来并行处理 gRPC 请求。

### 4.11 连接断开与重连

gRPC 连接断开时的处理策略：

1. **单次请求超时**：每个 gRPC 调用设置 timeout（默认 30 秒），超时后 FUSE 回调返回 `libc::EIO`
2. **连接级重连**：tonic `Channel` 内置连接重试机制，短暂断网后会自动重连
3. **长时间断网**：连续 N 次请求失败后（默认 N=10），FUSE daemon 输出警告日志但不自动卸载——卸载决策交给用户
4. **挂载点行为**：断网期间所有 FUSE 操作返回 `EIO`，应用程序（IDE、编译器）会感知到错误并提示用户

```rust
impl FileSystemRpcClient {
    /// 带 timeout 和重试的 gRPC 调用模板
    async fn call_with_timeout<F, T>(&self, f: F) -> Result<T>
    where
        F: Future<Output = Result<T, tonic::Status>>,
    {
        match tokio::time::timeout(self.timeout, f).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(status)) => Err(FuseError::from_grpc_status(status)),
            Err(_) => Err(FuseError::Timeout),
        }
    }
}
```

### 4.12 uid/gid 映射

Server 端返回的 `FsFileAttr` 中包含原始存储的 `uid/gid`，但 FUSE client 和 server 通常运行在不同机器上，用户体系可能不一致。

**策略**：Client 端在将 `FsFileAttr` 转换为 FUSE `FileAttr` 时，**统一将 uid/gid 映射为运行 workspace-fuse 进程的当前用户**。

```rust
fn to_file_attr(ino: u64, attr: &FsFileAttr) -> fuser::FileAttr {
    // 获取当前进程的 uid/gid
    let current_uid = unsafe { libc::getuid() };
    let current_gid = unsafe { libc::getgid() };

    fuser::FileAttr {
        ino,
        size: attr.size,
        // 使用 proto 返回的 blocks 字段（对稀疏文件更准确）
        // 如果 server 未返回 blocks（旧版本兼容），则按 size 估算
        blocks: if attr.blocks > 0 { attr.blocks } else { (attr.size + 511) / 512 },
        atime: to_system_time(&attr.atime),
        mtime: to_system_time(&attr.mtime),
        ctime: to_system_time(&attr.ctime),
        crtime: to_system_time(&attr.ctime),  // macOS creation time，用 ctime 代替
        kind: to_file_type(attr.file_type),
        perm: attr.mode as u16,
        nlink: attr.nlink,
        uid: current_uid,   // 映射为当前用户
        gid: current_gid,   // 映射为当前组
        rdev: 0,
        blksize: 4096,
        flags: 0,
    }
}
```

**原因**：
- 避免权限检查失败（FUSE 默认启用 `DefaultPermissions`，会根据 uid/gid 检查访问权限）
- 避免 `ls -l` 显示数字 uid 而非用户名（本地无对应用户时）
- 简化使用体验，用户无需配置映射规则

> 注意：`SetAttr` RPC 中的 `uid/gid` 字段仍然保留，但 FUSE client 在调用 `setattr` 时忽略 chown 请求（返回成功但不实际修改），因为远程 workspace 的 uid/gid 语义与本地不同。

### 4.13 gRPC 错误码映射

Server 端使用 gRPC status code 返回错误，Client 需要将其映射为 POSIX errno。

#### 4.13.1 结构化错误传递

为避免基于字符串匹配的脆弱性，Server 端应使用 gRPC `details` 字段传递结构化错误信息：

```protobuf
// 在 filesystem.proto 中添加
message FsErrorDetail {
    // POSIX errno 值（如 EISDIR=21, ENOTDIR=20, ENOTEMPTY=39）
    int32 errno = 1;
    // 可选的详细描述
    string description = 2;
}
```

Server 端返回错误时：
```rust
use tonic_types::{ErrorDetails, StatusExt};

fn to_grpc_error(err: std::io::Error) -> tonic::Status {
    let errno = err.raw_os_error().unwrap_or(libc::EIO);
    let code = match errno {
        libc::ENOENT => Code::NotFound,
        libc::EEXIST => Code::AlreadyExists,
        libc::EACCES | libc::EPERM => Code::PermissionDenied,
        libc::ENOSPC => Code::ResourceExhausted,
        libc::EISDIR | libc::ENOTDIR | libc::ENOTEMPTY | libc::EINVAL => Code::FailedPrecondition,
        _ => Code::Internal,
    };

    // 使用 tonic-types 的 ErrorDetails 传递结构化信息
    let mut details = ErrorDetails::new();
    details.set_error_info(
        &format!("ERRNO_{}", errno),  // reason
        "workspace.v1",               // domain
        [("errno".to_string(), errno.to_string())].into(),  // metadata
    );

    tonic::Status::with_error_details(code, err.to_string(), details)
}
```

#### 4.13.2 Client 端解析

```rust
use tonic_types::StatusExt;

impl FuseError {
    pub fn from_grpc_status(status: tonic::Status) -> Self {
        // 优先从 details 中提取 errno
        if let Ok(details) = status.get_error_details() {
            if let Some(error_info) = details.error_info() {
                if let Some(errno_str) = error_info.metadata.get("errno") {
                    if let Ok(errno) = errno_str.parse::<i32>() {
                        return Self::Errno(errno);
                    }
                }
            }
        }

        // 降级：基于 status code 映射（兼容旧版本 server）
        match status.code() {
            Code::NotFound => Self::Errno(libc::ENOENT),
            Code::AlreadyExists => Self::Errno(libc::EEXIST),
            Code::PermissionDenied => Self::Errno(libc::EACCES),
            Code::ResourceExhausted => Self::Errno(libc::ENOSPC),
            Code::InvalidArgument | Code::FailedPrecondition => {
                // 降级：解析 message（不推荐，仅用于兼容）
                let msg = status.message();
                if msg.contains("EISDIR") {
                    Self::Errno(libc::EISDIR)
                } else if msg.contains("ENOTDIR") {
                    Self::Errno(libc::ENOTDIR)
                } else if msg.contains("ENOTEMPTY") {
                    Self::Errno(libc::ENOTEMPTY)
                } else {
                    Self::Errno(libc::EIO)
                }
            }
            _ => Self::Errno(libc::EIO),
        }
    }
}
```

#### 4.13.3 错误码映射表

| gRPC Status | errno | 说明 |
|-------------|-------|------|
| `NOT_FOUND` | `ENOENT` | 文件或目录不存在 |
| `ALREADY_EXISTS` | `EEXIST` | 文件已存在（O_EXCL 场景） |
| `PERMISSION_DENIED` | `EACCES` | 权限不足 |
| `FAILED_PRECONDITION` | 从 details 获取 | EISDIR/ENOTDIR/ENOTEMPTY 等 |
| `RESOURCE_EXHAUSTED` | `ENOSPC` | 磁盘空间不足 |
| `UNAVAILABLE` | `EIO` | 服务不可用 |
| `DEADLINE_EXCEEDED` | `EIO` | 请求超时 |
| `UNAUTHENTICATED` | `EACCES` | Token 无效 |

> **依赖说明**：需要在 `Cargo.toml` 中添加 `tonic-types = "0.12"` 依赖。

### 4.14 符号链接语义

`Symlink` 创建的链接目标（target）按原样存储，不做任何转换。解析时：

- **相对路径**：在 FUSE 挂载点内相对解析（推荐使用）
- **绝对路径**：指向 **client 本地文件系统**的绝对路径，而非 server 端

```
# 假设挂载点为 ~/projects/my-workspace

# 相对路径符号链接（推荐）
ln -s ../shared/config.json ~/projects/my-workspace/app/config.json
# → 正确指向 ~/projects/my-workspace/shared/config.json

# 绝对路径符号链接（需谨慎）
ln -s /etc/hosts ~/projects/my-workspace/hosts-link
# → 指向 client 本地的 /etc/hosts，而非 server 端
```

> ⚠️ **注意**：包含绝对路径符号链接的 workspace 在不同 client 上挂载时可能行为不一致。建议在 workspace 内只使用相对路径符号链接。

## 5. 文件结构总览

### 5.1 共享 Proto Crate

当前 Server 的 `build.rs` 配置为 `build_client(false)` 仅生成服务端代码。fuse-client 需要客户端代码。为避免两个 crate 各自编译同一份 proto，提取为共享 crate：

```
workspace-proto/
├── Cargo.toml
├── build.rs          # tonic_build: build_server(true) + build_client(true)
└── src/
    └── lib.rs        # pub mod workspace { pub mod v1 { include!(...) } }
```

`workspace-proto` 的 `build.rs` 同时生成 server 和 client 代码。`server` 和 `fuse-client` 都依赖此 crate：

```toml
# workspace-proto/Cargo.toml
[package]
name = "workspace-proto"
version.workspace = true
edition.workspace = true

[dependencies]
tonic = { workspace = true }
prost = { workspace = true }
prost-types = { workspace = true }

[build-dependencies]
tonic-build = { workspace = true }
```

> 注意：迁移时需要同步修改 server 的 `build.rs` 和 `src/proto/` 引用，改为从 `workspace-proto` crate 导入。这是一次性的重构，影响面可控。

### 5.2 新增/修改的文件

```
新增/修改的文件：

workspace-proto/                     # 新增：共享 proto 生成 crate
├── Cargo.toml
├── build.rs
└── src/lib.rs

proto/workspace/v1/
└── filesystem.proto                 # 新增：FileSystemService proto 定义

server/src/api/grpc/
├── mod.rs                           # 修改：注册 FileSystemService + AuthInterceptor
└── filesystem.rs                    # 新增：FileSystemService gRPC 实现

server/Cargo.toml                    # 修改：依赖 workspace-proto 替代本地 proto 生成
server/build.rs                      # 修改：移除 proto 编译（迁移到 workspace-proto）
server/src/proto/                    # 删除：改为从 workspace-proto 导入

fuse-client/
├── Cargo.toml                       # 新增
└── src/
    ├── main.rs                      # 新增：CLI 入口
    ├── cli.rs                       # 新增：命令行参数解析
    ├── fuse_fs.rs                   # 新增：fuser::Filesystem 实现
    ├── rpc.rs                       # 新增：gRPC 客户端封装
    ├── inode.rs                     # 新增：Inode 管理
    └── cache.rs                     # 新增：缓存层

Cargo.toml                          # 修改：workspace members 新增 workspace-proto, fuse-client
```

## 6. 实施阶段

| 阶段 | 内容 | 依赖 |
|------|------|------|
| **F0** | 创建 `workspace-proto` 共享 crate，迁移 server 的 proto 生成 | 无 |
| **F1** | 定义 `filesystem.proto`，在 `workspace-proto` 中生成 Rust 代码 | F0 |
| **F2** | 实现 Server 端 `FileSystemServiceImpl` + `AuthInterceptor` | F1 + storage-backend-ha P1-P3 |
| **F3** | 注册 gRPC service，集成到 main.rs，配置 `WORKSPACE_FS_API_TOKEN` | F2 |
| **F4** | 实现 `fuse-client` 骨架：CLI + gRPC 连接 + Token 认证 | F1 |
| **F5** | 实现 Inode 管理和缓存层 | F4 |
| **F6** | 实现 `fuser::Filesystem` 核心方法（open/release/lookup/getattr/read/write/readdir/statfs） | F4 + F5 |
| **F7** | 实现剩余方法（create/mkdir/unlink/rmdir/rename/symlink/setattr） + 不支持操作返回 ENOSYS | F6 |
| **F8** | 端到端测试（挂载 → 文件操作 → 卸载） | F3 + F7 |

> F0 是基础设施重构，建议在所有其他阶段之前完成。F0 完成后，server 的 proto 引用需要统一调整。

## 7. 与 storage-backend-ha 的关系

```
                  storage-backend-ha                  fuse-client
                  ┌──────────────┐                    ┌──────────────┐
   P1-P2 ────────→│ StorageBackend │←── F2 ───────────│ gRPC Server  │
                  │ trait + Local  │                   │ (FileSystem  │
                  │ Backend 实现   │                   │  ServiceImpl)│
                  └──────┬───────┘                    └──────┬───────┘
                         │                                    │
   P3-P4 ────────→ WorkspaceService               F4-F7 ──→ FUSE Client
                   + NFS 层迁移                               Binary
                         │
   P5 ──────────→ 配置 + s3fs-fuse 集成
   P6 ──────────→ 并发控制（租约）
   P7 ──────────→ 集成测试 + 文档
```

- `fuse-client` 的 F2 依赖 `storage-backend-ha` 的 P1-P2（StorageBackend trait + LocalStorageBackend 实现）
- F0（共享 proto crate）是独立的基础设施工作，可以最先开始
- 其余部分独立，可以并行开发
- 优先级建议：先完成 F0 + P1-P2 + F1-F3，打通 gRPC 文件操作链路；然后 FUSE client (F4-F8) 和 S3 backend (P5-P7) 可并行

> 注意：storage-backend-ha 中只有 `LocalStorageBackend` 一个实现。S3 模式通过 s3fs-fuse 挂载后，`LocalStorageBackend` 直接操作挂载点目录，不存在单独的 S3StorageBackend。fuse-client 不感知后端是 local 还是 S3——所有请求通过 gRPC 到达 Server，由 Server 统一通过 StorageBackend 访问。

## 8. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| FUSE 性能瓶颈 | 频繁小文件操作延迟高 | moka 高性能并发缓存 + readahead + 元数据 TTL |
| gRPC message size | 大文件传输受限 | ReadFile/WriteFile streaming 分块（64KB/chunk）；ListDir streaming 分批发送 |
| 网络中断 | FUSE 操作阻塞/失败 | gRPC 30s timeout + FUSE reply EIO + Channel 内置重连 + 客户端连续失败告警 |
| inode 溢出 | 长时间运行后 u64 空间耗尽 | 实际不可能（u64 最大约 1.8 * 10^19） |
| 缓存一致性 | 多 client 同时写入同一 workspace 时数据不一致 | 文档明确警告：同一 workspace 同时只建议单 client 写入（见 1.2 节）；短 TTL 缓解只读场景的过期数据问题 |
| client 宿主机 FUSE 依赖 | 部分环境无 FUSE 内核模块 | 安装说明 + docker 镜像内置 FUSE 支持 |
| fuser 同步 API | 需要在同步回调中 block_on 异步操作 | 独立 Tokio runtime，block_on 前 clone Arc 避免借用冲突 |
| 不支持 flock/xattr | 部分工具依赖文件锁或扩展属性 | 返回 ENOSYS，编辑器/工具 fallback 到无锁模式；文档说明限制 |
| Token 明文传输 | HTTP 模式下 Token 可被窃取 | **生产环境强制使用 TLS（https://）**；Token 通过环境变量传入（不在命令行历史留痕）；运维定期轮换 |
| proto 共享 crate 迁移 | 影响现有 server 的 proto 引用 | F0 阶段独立完成并充分测试，一次性迁移 |
| 符号链接跨平台 | 绝对路径符号链接在不同 client 上行为不一致 | 文档建议只使用相对路径符号链接（见 4.14 节） |
| uid/gid 不一致 | Server 与 Client 用户体系不同 | Client 端统一映射为当前用户（见 4.12 节） |
