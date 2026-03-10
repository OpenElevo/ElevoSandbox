# Client 本地目录远程共享

## 1. 背景与目标

### 1.1 现状

当前 Workspace 的文件存储方向是单向的——文件始终存储在 Server 端（本地磁盘或 S3），Client 通过 NFS、FUSE、gRPC API 等方式消费。

```
Server (文件源)
  ├── NFS Export    ──→ Client 挂载
  ├── gRPC FileSystem ──→ FUSE Client 挂载
  └── Docker Bind Mount ──→ Sandbox 容器
```

如果 Client 想将**自己本地的目录**通过 Workspace 机制共享给其他消费方（其他 Client、Sandbox 等），目前没有途径。

### 1.2 目标

让 Client 可以将本地目录注册为一个 Workspace 的存储源，Server 作为中转枢纽，其他消费方通过现有机制（NFS、FUSE、API）正常访问。

```
Client A (本地目录 /my/project)
  │
  │ 注册为 Workspace 存储源
  ▼
Server (中转)
  ├── NFS Export     ──→ Client B
  ├── gRPC FileSystem ──→ Client C (FUSE)
  └── Sandbox        ──→ 容器内访问
```

### 1.3 核心约束

- Client 可能在 NAT/防火墙后面，Server 无法主动连接 Client
- 读写共享——消费方和 Client 都可以读写文件
- Client 断线后，消费方在超时时间内等待重连，超时后报错
- SDK 层面先支持 Go

---

## 2. 传输通道设计

提供两种传输通道，**默认使用 gRPC 反向流**，可按 Workspace 粒度注册为 NFS。

### 2.1 gRPC 反向流（默认通道）

Client 主动连接 Server，建立 gRPC 双向流。Server 通过这条流向 Client 发送文件操作请求，Client 在本地执行后返回结果。

```
Client                          Server
  │                               │
  │── Connect(workspace_id) ─────→│  Client 主动发起双向流
  │                               │  Server 绑定该流到 Workspace
  │                               │
  │←── Stat(path) ────────────────│  消费方触发文件操作
  │── StatResponse(attr) ────────→│  Client 本地执行并返回
  │                               │
  │←── ReadAt(path,offset,size) ──│
  │── ReadAtResponse(data) ──────→│
  │                               │
```

**适用场景**：公网、NAT 后面、跨网络环境——只要 Client 能连到 Server 就行。

**特点**：
- 无需 Client 暴露端口
- 每次操作经过一次网络 RTT
- Server 端通过 FUSE 将远程存储挂载为本地目录，Sandbox、NFS 等上层功能无感知（详见 3.2 节）

### 2.2 NFS 通道（可选，按 Workspace 注册）

Client 本地运行 NFS server 导出目录，Server mount 到本地路径。之后使用现有 `LocalStorageBackend` 直接操作 mount 点。

```
Client                          Server
  │                               │
  │  nfs-kernel-server 导出       │
  │  /{workspace_id} → /my/data   │
  │                               │
  │←──── NFS mount ───────────────│  Server mount 到
  │                               │  /var/lib/workspace/{workspace_id}/
  │                               │
  │  之后所有文件操作走内核 NFS    │
```

**适用场景**：Client 和 Server 在同一局域网，Client 可以暴露 NFS 端口。

**特点**：
- 内核级 VFS 缓存，性能接近本地文件系统
- 支持文件锁（NLM）、mmap
- Client 需要安装 NFS server 并具有相应权限
- Client 需要对 Server 暴露 NFS 端口（默认 2049）

### 2.3 两种通道的选择逻辑

不做自动检测。创建 Workspace 时默认使用 gRPC 通道，之后可以通过 API 注册为 NFS 通道。

| 操作 | 说明 |
|------|------|
| 创建 Workspace（storage_type=remote） | 默认 gRPC 通道，等待 Client 连接 |
| 注册 NFS（提供 NFS URL） | 切换为 NFS 通道，Server 执行 mount |
| 注销 NFS | 切回 gRPC 通道，Server 执行 umount |

同一时刻只有一种通道生效。切换时通过读写锁排空正在进行的文件操作（详见 3.5 节）。

---

## 3. 存储架构变更

### 3.1 Per-Workspace 存储路由

当前系统使用全局单一 `Arc<dyn StorageBackend>`。为支持远程存储，需要改为 **per-workspace 路由**。

引入 `StorageRouter`，持有全局默认后端和 per-workspace 覆盖后端。每次文件操作先按 workspace_id 查找是否有专用后端，没有则 fallback 到全局默认后端。

```
                   ┌─ workspace_abc → RemoteStorageBackend (gRPC 流)
StorageRouter ─────┼─ workspace_def → LocalStorageBackend  (NFS mount 点)
                   └─ 其他 workspace → 全局默认后端 (Local 或 S3)
```

`StorageRouter` 自身也实现 `StorageBackend` trait，对上层（NFS、gRPC FileSystem API、WorkspaceService）完全透明——它们无需知道底层是本地存储、S3、gRPC 还是 NFS mount。

### 3.2 Server 端 FUSE 挂载——让远程存储表现为本地目录

gRPC 通道的核心设计：**Server 在 `/var/lib/workspace/{workspace_id}/` 创建一个 FUSE 挂载点**，底层对接 `RemoteStorageBackend`。这样远程存储在 Server 上表现为一个普通本地目录。

```
RemoteStorageBackend (gRPC 反向流 → Client)
  ▲
  │ 调用 StorageBackend trait 方法
  │
Server 端 FUSE Filesystem
  │ 挂载到 /var/lib/workspace/{workspace_id}/
  │
  └── 对外表现为普通目录
        ├── Sandbox bind mount   → 正常工作，零改动
        ├── NFS export           → 正常工作，零改动
        ├── ls / cat / vim       → 正常工作
        └── 任何读写本地文件的工具 → 正常工作
```

**这个设计的关键收益**：所有现有代码路径（Sandbox 创建、NFS 导出、Docker bind mount、工作目录存在性检查）全部零改动。remote workspace 和 local workspace 在 Server 视角完全一致——都是 `/var/lib/workspace/{workspace_id}/` 下的一个目录。

**实现方式**：复用现有 `fuse-client` 中的 `WorkspaceFuse` 核心逻辑（inode 管理、缓存、FUSE 协议处理），将底层的 gRPC client 替换为直接调用 `StorageBackend` trait。提取为一个共享的 FUSE 库，Server 端和独立 FUSE client 共用。

**运行架构**：Server 端 FUSE daemon 以**进程内线程**方式运行（而非独立子进程）。每个 remote workspace 的 FUSE 挂载由一个独立的 tokio task 驱动。这意味着：
- FUSE daemon 与 Server 共生——Server 进程崩溃时 FUSE 挂载自动失效，恢复流程统一走 Server 重启逻辑（见 8.3 节）
- 无需 IPC 开销，FUSE 操作通过函数调用直接路由到 `RemoteStorageBackend`
- FUSE 线程 panic 不会导致 Server 进程退出——使用 `std::panic::catch_unwind` 捕获，记录错误日志并尝试重建该 workspace 的 FUSE 挂载

**FUSE 挂载健康监控**：Server 端定期（每 30 秒）检查所有 remote workspace 的 FUSE 挂载点健康状态：
- 对挂载点执行 `statfs` 调用，超时 5 秒
- 如果 `statfs` 超时或返回 `Transport endpoint is not connected`，标记为 stale
- stale 挂载点执行 `fusermount -u` 强制卸载，然后重建 FUSE 挂载
- 重建期间该 workspace 的文件操作返回 `StorageError::Io`，FUSE 重建完成后自动恢复

**部署要求**：Server 端 FUSE 挂载点需要被 Docker 容器（Sandbox bind mount）和 NFS server 进程等其他用户/进程访问。因此：
- 挂载时必须使用 `allow_other` 选项，允许非挂载者用户访问
- Server 部署环境需要在 `/etc/fuse.conf` 中启用 `user_allow_other`
- 如果 Server 以 root 运行，`allow_other` 默认可用；非 root 用户则依赖上述配置
- Docker 的 bind mount 需要使用 `rshared` mount propagation，确保 FUSE 挂载在容器内可见（详见 8.3 节 Server 重启恢复设计）

**缓存策略**：为缓解 gRPC RTT 对密集元数据操作（如 `git status`）的性能影响，Server 端 FUSE 采用多级缓存：

- **内核缓存**（FUSE entry/attr timeout）：
  - `entry_timeout`：目录项缓存，默认 1 秒（可通过 Workspace 级配置调整）
  - `attr_timeout`：文件属性缓存，默认 1 秒（可通过 Workspace 级配置调整）
  - 这两个值保持较短，因为 Client 可能随时修改文件
  - 对于只读或低频写入的场景，可适当增大（如 5-10 秒）以提升性能
- **应用层缓存**（复用 `WorkspaceFuse` 已有的 inode 缓存）：
  - inode → 属性映射缓存，TTL 与内核缓存一致
  - 目录内容缓存，TTL 1 秒（可配置）
- **缓存失效**：依赖 Client 端文件变更通知主动失效（详见 3.6 节），收到 `FileChanged` 事件后立即清除对应路径的缓存并通过 `fuse_lowlevel_notify_inval_entry` / `fuse_lowlevel_notify_inval_inode` 通知内核失效。未收到通知时，依赖 TTL 自然过期
- **写操作**：写操作完成后立即更新本地缓存（write-through），无需等待 TTL 过期

**FUSE 挂载的生命周期**：
- Client 首次 Connect 成功时，Server 创建 FUSE 挂载点
- Client 断线时，FUSE 挂载保留，文件操作会收到 EIO（超时后）
- Client 重连后，FUSE 挂载自动恢复工作（底层 RemoteStorageBackend 重新绑定流）
- Workspace 删除时，先停止关联 Sandbox → umount FUSE → 清理（详见 7.1 节）

**FUSE Client 直连优化（接口预留）**：

当消费方本身也是 FUSE Client 时，数据路径会经过两次 FUSE 内核穿越和两次 gRPC 往返，性能损耗显著。为后续优化预留架构接口：

- `StorageRouter` 在路由时，除了返回 `StorageBackend`，还提供 `get_remote_connection_info(workspace_id)` 方法，返回 remote workspace 的连接元信息（是否为 remote、Client 是否在线）
- gRPC `FileSystemService` 在处理 remote workspace 请求时，可通过此接口判断是否为 remote workspace，为后续实现"Server 作为代理直接转发请求到 Client，绕过 Server 端 FUSE"提供入口
- 本期不实现直连逻辑，所有请求仍经过 Server 端 FUSE 挂载点。直连优化列入已知限制的未来规划（见第 9 节）

### 3.3 RemoteStorageBackend（gRPC 通道的存储后端）

实现 `StorageBackend` trait 的新后端。内部持有到 Client 的 gRPC 双向流，将每个 trait 方法转换为流上的请求-响应对。

**流模型——控制流与数据流分离**：

为避免单条流成为并发瓶颈（大文件传输阻塞元数据操作），采用双流设计：

- **控制流**（双向流）：承载元数据操作（stat、list_dir、mkdir、rename、symlink 等）和心跳。这些操作消息体小、延迟敏感。
- **数据流**（独立 streaming RPC）：承载大文件读写（read_file、write_file）。每次大文件操作建立独立的 streaming 调用，不阻塞控制流。

小文件读写（read_file_range、write_file_at，通常 < 64KB）仍走控制流，避免为小操作建立独立流的开销。阈值可配置，默认 64KB。

**64KB 阈值的选择依据**：
- **下界考量**：低于 4KB 的阈值会导致几乎所有文件操作都走数据流，数据流的建连开销（gRPC stream 建立约 1-2ms）远超直接在控制流发送的耗时
- **上界考量**：控制流是有序的，一条 1MB 的消息按 10MB/s 的典型带宽需要 100ms 传输，期间会阻塞所有元数据操作（Head-of-Line Blocking）
- **64KB 折中**：传输耗时约 6ms（10MB/s 带宽），对后续元数据操作的阻塞可接受；同时避免了大量小文件操作频繁建立数据流的开销
- 此阈值标注为可调参数（`remote_data_stream_threshold`），可在实测后根据网络环境调整

**数据流关联机制**：大文件读写需要在控制流和数据流之间建立关联。流程如下：

1. Server 生成唯一 `transfer_id`（UUID），通过控制流发送 `StartDataTransfer` 通知：
   ```
   Server → Client (控制流): StartDataTransfer {
     transfer_id: "uuid-xxx",
     operation: READ_FILE / WRITE_FILE,
     path: "/some/file",
     // READ_FILE 时无需额外参数
     // WRITE_FILE 时携带 file_size（如已知）
   }
   ```
2. Client 收到通知后，发起对应的独立数据流 RPC，携带 `transfer_id` 作为请求元数据：
   ```
   Client → Server: ReadFileStream { transfer_id: "uuid-xxx" }
   // 或
   Client → Server: WriteFileStream { transfer_id: "uuid-xxx", data chunks... }
   ```
3. Server 通过 `transfer_id` 将数据流 RPC 与原始请求匹配（`DashMap<transfer_id, oneshot::Sender>`）
4. 超时处理：如果 Client 在 10 秒内未发起对应的数据流 RPC，Server 超时取消该请求，返回 `StorageError::Io`
5. Client 断线处理：断线时 Server 主动清理所有 pending 的 transfer（同控制流的 pending 请求清理）

**数据流故障处理**：
- **Client 收到 `StartDataTransfer` 但数据流 RPC 建立失败**：Client 通过控制流发送 `DataTransferFailed { transfer_id, reason }` 通知 Server。Server 收到后立即取消该请求，返回 `StorageError::Io`，无需等待 10 秒超时
- **数据流 RPC 迟到**（Server 已因超时清理了 `transfer_id`）：Server 返回 gRPC `NOT_FOUND` 状态码（语义为 `TRANSFER_EXPIRED`），Client 丢弃该数据流。不做重试——原始文件操作已超时失败，上层调用方会根据需要重试
- **数据流传输中途中断**（网络抖动）：gRPC streaming 断开时，Server 和 Client 均感知到流结束。Server 端对应的文件操作返回 `StorageError::Io`。Client 端清理本地状态（关闭文件句柄等）。不做断点续传——上层 FUSE 会将 EIO 返回给调用方，由调用方决定是否重试

**Client 断线重连时的在途操作行为**：
- **控制流上的 pending 请求**：断线时已全部通过 DashMap 清理并返回错误（见超时机制），FUSE 层收到 EIO。Client 重连后，这些操作不会自动恢复——依赖上层调用方重试
- **数据流上的在途传输**：gRPC stream 断开后自动终止。Server 端未完成的写操作回滚（不保留部分写入），未完成的读操作返回 EIO。Client 重连后不恢复这些传输——新的文件操作将发起新的请求
- **FUSE 层的挂起操作**：Client 断线期间，新的 FUSE 操作会进入 `RemoteStorageBackend` 的等待队列（超时 30 秒）。Client 在超时前重连的，队列中的操作自然恢复执行；超时后的返回 EIO

**请求-响应匹配**：控制流上每个请求携带唯一 correlation_id，Client 返回时附带相同 id。Server 端用 `DashMap<correlation_id, oneshot::Sender>` 做异步等待。这个模式和现有 `AgentService.Connect` 完全一致。

**超时机制**：每个请求设置超时（默认 30 秒）。Client 断线后，等待中的请求在超时后返回 `StorageError::Io`。与现有 Agent 模式不同，Client 断线时需要**主动清理所有 pending 请求**（遍历 DashMap 发送错误），而不是等待各自超时，避免资源泄漏。

**心跳**：Server 端每 15 秒通过控制流发送 Ping。心跳判断基于**流活跃度**——收到任何消息（Pong 或操作响应）都重置心跳计时器。这样 Client 在忙于处理大量操作时不会被误判为断线。连续 45 秒无任何消息视为断线。

**反压机制**：Server 并发发送大量请求（如消费方执行 `find /`）时，Client 处理能力可能跟不上。通过以下机制实现反压：
- **Server 端并发控制**：`RemoteStorageBackend` 维护一个信号量（`tokio::sync::Semaphore`），限制同时在途的控制流请求数量（默认上限 128）。超出上限时，新请求阻塞等待信号量，而非无限制发送到控制流
- **gRPC flow control**：依赖 HTTP/2 的内置流控——当 Client 端 gRPC 接收缓冲区满时，TCP 层的流控会自然减慢 Server 的发送速率
- **Client 端溢出保护**：Client 端响应 channel（容量 256）写满时，工作 goroutine 阻塞在 channel 写入上，不再从请求 channel 消费新请求，形成自然的反压链路。当 channel 持续满载超过 5 秒时，Client 输出警告日志

**重连**：Client 断线后重新 Connect，Server 重新绑定流到对应 Workspace。FUSE 挂载层无需感知重连——底层 RemoteStorageBackend 流恢复后，挂起的 FUSE 操作自然恢复。

### 3.4 NFS 通道的存储处理

NFS 通道不需要新的 StorageBackend 实现，也不需要 Server 端 FUSE 挂载。Server 直接将 Client 的 NFS export mount 到 `/var/lib/workspace/{workspace_id}/`，之后 `LocalStorageBackend` 直接工作在这个 mount 点上，和本地存储完全一致。

mount 状态需要监控（参考现有 S3fsMountMonitor 的做法）：定期检查 `/proc/mounts`，mount 丢失时尝试 remount。

**线程池隔离**：NFS mount 点的文件操作如果因 Client 响应慢而阻塞，会占满 `tokio::fs` 的阻塞线程池，影响同一 Server 上所有 workspace 的文件操作。因此 remote-nfs workspace 的 `LocalStorageBackend` 应使用独立的阻塞线程池，与全局默认后端隔离。

### 3.5 通道切换的排空机制

从 gRPC 切换到 NFS（或反向）时，需要保证正在进行的文件操作全部完成后再切换，避免数据不一致。

`StorageRouter` 为每个 remote workspace 维护一把读写锁：
- 正常文件操作获取读锁（不互斥，高并发）
- 通道切换获取写锁（等待所有读锁释放）
- 写锁获取后，替换该 workspace 的 StorageBackend 引用，然后释放

写锁获取设置超时（默认 60 秒）。超时后切换失败，返回错误，不影响当前通道。

**切换状态机**：

为保证切换过程的原子性（Server 在切换中途崩溃后可恢复），引入持久化的切换状态机。切换进度记录在 DB 的 `storage_config` 中：

```
状态流转（以 gRPC → NFS 为例）：

  grpc (正常)
    │
    │  发起切换请求
    ▼
  switching_to_nfs_pending    ← DB 记录目标状态和 NFS URL
    │
    │  mount 到临时路径并验证
    ▼
  switching_to_nfs_mounted    ← DB 记录临时路径已 mount
    │
    │  umount 旧 FUSE + 切换路径 + 更新后端引用
    ▼
  nfs (正常)
```

**切换流程与回滚**（以 gRPC → NFS 为例）：

1. 更新 DB `storage_config` 为 `{"transport": "grpc", "switching_to": "nfs", "switch_phase": "pending", "nfs_url": "..."}`
2. 获取写锁（排空在途操作）
3. 执行 NFS mount 到临时路径 `/var/lib/workspace/{workspace_id}.nfs-pending/`
4. 验证 NFS mount 可用（执行一次 stat 检查）
5. 如果 mount 或验证失败 → umount 临时路径 → 清除 DB 中的 switching 状态 → 释放写锁 → 返回错误，**gRPC 通道不受影响**
6. 更新 DB `switch_phase` 为 `"mounted"`
7. umount 原 FUSE 挂载点
8. rename 临时路径到正式路径 `/var/lib/workspace/{workspace_id}/`（或 `mount --move`）
9. 更新 StorageRouter 中该 workspace 的后端引用
10. 更新 DB `storage_config` 为 `{"transport": "nfs", "nfs_url": "..."}`（清除 switching 字段）
11. 释放写锁

**Server 崩溃恢复**：Server 启动时扫描 DB 中所有包含 `switching_to` 字段的 workspace，根据 `switch_phase` 执行恢复：
- `pending` 阶段崩溃：临时路径可能未 mount 或 mount 不完整 → 尝试 umount 临时路径（忽略错误）→ 清除 switching 状态 → 回滚到旧通道
- `mounted` 阶段崩溃：临时路径已 mount 成功 → 继续执行 step 7-10 完成切换（如果旧通道已 umount 则直接 rename；如果旧通道还在则先 umount）

反向切换（NFS → gRPC）同理：先在临时路径创建 FUSE 挂载并验证可用，成功后再 umount NFS。状态机流转方向相反。

**关键原则**：旧通道在新通道完全就绪前不会被拆除，保证切换过程中不存在两种通道都不可用的窗口。切换进度持久化到 DB，Server 崩溃后可以确定性地恢复或回滚。

### 3.6 文件变更通知（gRPC 通道）

Client 本地目录的文件可能被 Client 自身的进程修改（编辑器、构建工具等），Server 端需要及时感知这些变更以失效缓存。

**Client 端**：
- Go SDK 使用 `fsnotify` 库监听共享目录的文件系统事件
- **递归监听实现**：Linux inotify 不支持原生递归监听。SDK 需要手动管理 watch：
  1. 启动时递归遍历共享目录，为每个子目录添加 inotify watch（`IN_CREATE | IN_DELETE | IN_MODIFY | IN_MOVED_FROM | IN_MOVED_TO | IN_ATTRIB`）
  2. 收到 `IN_CREATE` 事件且目标是目录时，动态为新目录及其子目录添加 watch
  3. 收到 `IN_DELETE` 或 `IN_MOVED_FROM` 事件且目标是目录时，inotify 自动移除已删除目录的 watch（无需手动处理）
  4. 收到 `IN_MOVED_TO` 事件且目标是目录时，为移入的目录树递归添加 watch
  5. 支持 `.elevoignore` 文件（语法同 `.gitignore`），匹配的目录不添加 watch，减少资源占用。默认忽略 `.git`、`node_modules`、`__pycache__`、`target`、`build` 等常见构建产物目录
- 事件去重与合并：同一路径的事件在 50ms 窗口内合并为一条通知（避免编辑器保存时产生大量事件）
- 通过控制流向 Server 发送 `FileChanged` 消息（Client → Server 方向）

**Server 端**：
- 收到 `FileChanged` 后，清除对应路径的应用层缓存（inode 属性、目录内容）
- 调用 FUSE 内核接口通知缓存失效：
  - 文件修改/属性变更 → `fuse_lowlevel_notify_inval_inode`
  - 文件创建/删除/重命名 → `fuse_lowlevel_notify_inval_entry`（父目录）
- 不主动推送给消费方——消费方下次访问时从 Client 拿到最新数据

**消息格式**：
```
Client → Server (控制流): FileChanged {
  events: [
    { path: "src/main.rs", event_type: MODIFIED },
    { path: "src/lib.rs", event_type: CREATED },
    { path: "old_name.rs", new_path: "new_name.rs", event_type: RENAMED },
  ]
}
```

**event_type 枚举**：`CREATED | MODIFIED | DELETED | RENAMED | ATTR_CHANGED`

**可靠性**：文件变更通知是尽力而为的（best-effort）。如果通知丢失（Client 断线期间的变更），缓存会在 TTL 过期后自然失效。Client 重连后，Server 会执行一次全量缓存清除，确保不会使用 stale 数据。

**inotify 资源限制**：大型项目可能超出 inotify watch 数量限制（默认 8192）。Client SDK 需要：
- 启动时检查 `/proc/sys/fs/inotify/max_user_watches` 并在不足时输出警告日志
- 超出限制时降级为定期全量缓存清除（每 5 秒），而非放弃变更通知

---

## 4. 协议设计

### 4.1 新增 gRPC Service：ClientStorageService

专门用于 Client 反向流连接的 service。包含一条控制流（双向流）和独立的数据传输 RPC。

**控制流**：`Connect(stream ClientMessage) returns (stream ServerMessage)` — 承载元数据操作和心跳。

**数据流**：
- `ReadFileStream(ReadFileStreamRequest) returns (stream ReadFileStreamResponse)` — Server 从 Client 读取大文件
- `WriteFileStream(stream WriteFileStreamRequest) returns (WriteFileStreamResponse)` — Server 向 Client 写入大文件

每个数据流 RPC 必须携带 `transfer_id`（由 Server 通过控制流的 `StartDataTransfer` 消息下发），Server 据此将数据流与原始请求关联。详细的关联流程见 3.3 节。

Client 连接 Server 后，先通过控制流的 `Connect` 建立会话。大文件读写时，Server 通过控制流发送 `StartDataTransfer` 通知 Client 发起独立的数据流 RPC。

**控制流消息结构**：

- **Server → Client（请求）**：操作类型 + correlation_id + 操作参数
- **Client → Server（响应）**：correlation_id + 操作结果（成功数据或错误码）

**控制流承载的操作类型**：

**Server → Client 方向（请求）**：

| 操作 | 对应 StorageBackend 方法 | 说明 |
|------|--------------------------|------|
| Stat | stat | 获取文件元数据 |
| ListDir | list_dir | 列出目录内容（流式分页返回，见下方说明） |
| Exists | exists | 检查文件是否存在 |
| ReadFileRange | read_file_range | 读取文件片段（< 阈值，默认 64KB） |
| WriteFileAt | write_file_at | 在指定偏移写入（< 阈值，默认 64KB） |
| CreateFile | create_file | 创建文件 |
| Mkdir | mkdir | 创建目录 |
| RemoveFile | remove_file | 删除文件 |
| RemoveDir | remove_dir | 删除目录 |
| Rename | rename, rename_noreplace, rename_exchange | 重命名/移动 |
| Copy | copy | 复制文件 |
| SetFileSize | set_file_size | 截断文件 |
| SetPermissions | set_permissions | 设置权限 |
| SetTimes | set_times | 设置时间戳 |
| Symlink | symlink | 创建符号链接 |
| ReadLink | readlink | 读取符号链接目标 |
| StatFs | stat_fs | 获取文件系统统计 |
| StartDataTransfer | - | 通知 Client 发起数据流 RPC（携带 transfer_id） |
| Ping | - | 心跳检测 |

**Client → Server 方向（响应 + 主动推送）**：

| 操作 | 说明 |
|------|------|
| 操作响应 | correlation_id + 操作结果（成功数据或错误码） |
| FileChanged | 文件变更通知（主动推送，详见 3.6 节） |
| Pong | 心跳响应 |

这些操作覆盖了 `StorageBackend` trait 的全部方法。大文件的 `read_file` 和 `write_file` 走独立的数据流 RPC（通过 `StartDataTransfer` 触发）。

**ListDir 流式分页**：为避免大目录（数千条目）的响应在控制流上造成 Head-of-Line Blocking，`ListDir` 采用分页返回。Client 将目录内容分批发送（每批最多 200 条），每批使用相同的 `correlation_id`，最后一批标记 `is_last=true`。Server 端收集所有批次后组装完整结果。

### 4.2 Workspace API 扩展

**创建远程 Workspace**：在现有 `CreateWorkspace` 请求中增加 `storage_type` 字段。当 `storage_type=remote` 时，Server 创建 Workspace 记录但不创建本地目录，等待 Client 连接后通过 Server 端 FUSE 挂载创建目录。

**注册 NFS 通道**：新增 API，允许将一个 remote Workspace 的传输通道从 gRPC 切换到 NFS。需要提供 Client 的 NFS URL（如 `nfs://192.168.1.100:2049/my-project`）。Server 收到后执行 mount 操作，成功后切换该 Workspace 的存储后端为 `LocalStorageBackend`（指向 mount 点），同时卸载 Server 端 FUSE 挂载。

**NFS URL 安全校验**（防止 SSRF 攻击）：

Server 收到 NFS 注册请求后，在执行 mount 前必须通过以下校验：

1. **白名单校验（默认拒绝）**：Server 配置项 `nfs_allowed_cidrs`（类型：CIDR 列表），未配置时**拒绝所有 NFS 注册请求**。管理员必须显式配置允许的网段才能使用 NFS 通道。示例配置：`nfs_allowed_cidrs = ["192.168.1.0/24", "10.0.0.0/8"]`
2. **DNS 解析校验**：如果 NFS URL 使用主机名而非 IP，先解析 DNS，检查解析结果是否在白名单内（防止 DNS rebinding 攻击）
3. **端口限制**：仅允许标准 NFS 端口（2049）或管理员配置的端口范围，防止利用 mount 操作探测内网其他服务
4. **mount 超时**：mount 操作设置 30 秒超时（`-o timeo=300,retry=0`），防止对不可达地址的 mount 请求长时间挂起
5. **mount 后验证**：mount 成功后执行 stat 检查，验证目标确实是 NFS 文件系统（而非被劫持的本地路径）

**注销 NFS 通道**：umount NFS，恢复 Server 端 FUSE 挂载，切回 gRPC 通道。

### 4.3 错误码映射

Client 在本地执行文件操作时可能遇到各种 OS 错误。需要在协议层定义标准错误码，映射到 `StorageError`：

| 协议错误码 | StorageError |
|-----------|-------------|
| NOT_FOUND | NotFound |
| ALREADY_EXISTS | AlreadyExists |
| IS_A_DIRECTORY | IsADirectory |
| NOT_A_DIRECTORY | NotADirectory |
| NOT_A_FILE | NotAFile |
| DIRECTORY_NOT_EMPTY | DirectoryNotEmpty |
| PERMISSION_DENIED | PermissionDenied |
| PATH_TRAVERSAL_DENIED | PathTraversalDenied |
| NOT_SUPPORTED | NotSupported |
| IO_ERROR | Io |

覆盖 `StorageError` 的全部变体。

---

## 5. 数据模型变更

### 5.1 workspaces 表

新增两个字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| storage_type | TEXT NOT NULL DEFAULT 'managed' | 存储类型：managed / remote |
| storage_config | TEXT NOT NULL DEFAULT '{}' | JSON，远程存储配置 |

`storage_type` 仅区分两种场景：
- `managed`：Server 管理的存储后端（由环境变量 `WORKSPACE_STORAGE_TYPE` 决定是 local 还是 S3）。命名为 `managed` 而非 `default`，语义更明确——表示存储由 Server 管理
- `remote`：Client 提供的远程存储后端

`storage_config` 仅在 `storage_type=remote` 时有意义，采用带版本的 JSON schema：
- gRPC 通道（默认）：`{"v": 1, "transport": "grpc"}`
- NFS 通道：`{"v": 1, "transport": "nfs", "nfs_url": "nfs://192.168.1.100:2049/my-project"}`
- 通道切换中：`{"v": 1, "transport": "grpc", "switching_to": "nfs", "switch_phase": "pending", "nfs_url": "..."}`（见 3.5 节状态机）

`v` 字段为 schema 版本号，当前为 1。后续扩展（如增加缓存配置、认证信息等）时通过递增版本号管理兼容性。Server 读取时根据版本号选择对应的反序列化逻辑，遇到未知版本号时拒绝操作并输出错误日志。

这样避免 per-workspace `storage_type` 与全局存储配置产生语义冲突。

### 5.2 Workspace 生命周期变化

**managed Workspace（不变）**：
1. 创建 → 分配本地目录 → 导出 NFS → 可用

**remote Workspace**：
1. 创建（storage_type=remote）→ 记录写入 DB → 状态为 `pending`
2. Client 通过 `ClientStorageService.Connect` 连接 → Server 创建 FUSE 挂载 → 导出 NFS → 状态变为 `connected`
3. 可选：通过 API 注册 NFS 通道 → Server umount FUSE、mount NFS → 切换为 NFS 传输
4. Client 断线 → 状态变为 `disconnected`，FUSE 挂载保留，文件操作超时后报错
5. Client 重连 → 状态恢复为 `connected`，FUSE 操作自动恢复
6. Server 重启 → FUSE 挂载丢失 → Client 重连后重建 FUSE → 自动重启关联 Sandbox 容器（详见 8.3 节）
7. 删除 → 先停止关联 Sandbox → umount（FUSE 或 NFS）→ 清理 DB 记录

### 5.3 连接状态

remote Workspace 有一个运行时状态（不持久化到 DB，存在内存中）：

| 状态 | 说明 |
|------|------|
| pending | 已创建，等待 Client 首次连接 |
| connected | Client 已连接，文件操作正常 |
| disconnected | Client 断线，FUSE 挂载保留但操作会超时报错 |

---

## 6. Client 端设计（Go SDK）

### 6.1 核心能力

Go SDK 新增 `StorageProvider`，Client 调用后将本地目录通过 gRPC 反向流共享给 Server。

使用流程：
1. Client 创建一个 remote 类型的 Workspace
2. Client 调用 SDK 的 share 方法，传入 workspace_id 和本地目录路径
3. SDK 内部建立 gRPC 双向流连接（控制流），在本地目录上执行 Server 发来的文件操作请求
4. 大文件读写时，SDK 根据控制流的 `StartDataTransfer` 通知发起独立的数据流 RPC
5. SDK 启动 fsnotify watcher 监听共享目录，推送文件变更事件到 Server（详见 3.6 节）
6. SDK 处理心跳、断线重连等

### 6.2 Client 端并发模型

Server 可能并发发送大量文件操作请求（例如消费方执行 `find`、`tree` 或 IDE 打开项目时的批量 stat），Client SDK 需要高效处理并发：

- **控制流处理**：一个 goroutine 负责从控制流读取请求，解码后分发到工作池
- **工作池**：固定大小的 goroutine 池（默认 64 个）处理文件操作请求。每个请求在独立 goroutine 中执行本地文件操作，完成后将响应写回控制流
- **控制流写入串行化**：响应写入控制流需要串行化（gRPC stream 不支持并发写入），使用带缓冲的 channel（容量 256）汇总响应，由单独的 goroutine 负责写入
- **数据流并发**：大文件操作的数据流 RPC 独立于控制流，可以并行（每个数据流一个 goroutine），最大并发数限制为 8（防止过多并行传输导致带宽争用）
- **写操作串行化**：对同一文件的并发写操作需要在 Client 端通过 per-file 锁串行化，避免数据损坏。读操作不受此限制。锁策略细节：
  - **锁的 key**：使用文件的**规范化相对路径**（`filepath.Clean` 后的路径字符串）作为 key，存储在 `sync.Map` 中。不使用 inode 的原因是：Server 发来的请求只包含路径，获取 inode 需要额外的 stat 系统调用
  - **锁粒度**：每个路径对应一把 `sync.Mutex`。锁在首次写操作时惰性创建，存入 `sync.Map`
  - **rename 处理**：`rename(old, new)` 操作需要同时获取 old 和 new 两个路径的锁（按路径字典序获取，避免死锁），操作完成后释放。这保证了 rename 过程中 old 和 new 路径都不会有并发写入
  - **delete 处理**：`remove_file(path)` 获取 path 的锁后执行删除，完成后从 `sync.Map` 中移除该锁条目（惰性清理，允许极低概率的竞态——新创建的同名文件会重新创建锁条目）
  - **锁超时**：单个锁等待超时 10 秒，超时后返回 `IO_ERROR`，防止某个卡住的写操作阻塞后续所有同文件写入

### 6.3 NFS 通道注册

Go SDK 也提供注册 NFS 通道的方法：
1. Client 先自行配置好本地 NFS server（导出目标目录）
2. 调用 SDK 方法传入 NFS URL
3. SDK 调用 Server API 完成注册
4. Server 执行 mount 后，该 Workspace 切换为 NFS 传输

### 6.4 安全考虑

**gRPC 通道**：
- 复用现有 Token 认证机制
- Client 只能连接自己有权限的 Workspace
- **路径穿越防护（关键安全要求）**：Client SDK 必须对 Server 发来的每个路径执行安全校验，确保操作始终限制在共享目录内。不能仅依赖 OS 权限——Client 进程通常有权访问远超共享目录的文件系统范围（如 `/etc/passwd`、`~/.ssh/`）。Server 的 bug 或被攻击后可能发送恶意路径。

  **防护采用两层机制**：

  **第一层：路径预校验（快速拒绝）**：
  1. 对收到的相对路径执行 `filepath.Clean`，拒绝包含 `..` 的路径
  2. 将清理后的路径与共享目录根路径拼接，检查结果是否以共享目录路径为前缀
  3. 不满足则立即返回 `PATH_TRAVERSAL_DENIED` 错误

  **第二层：基于 openat 的安全文件操作（消除 TOCTOU 竞态）**：

  仅做路径预校验存在 TOCTOU（Time-of-Check-Time-of-Use）竞态——在检查通过后、实际操作前，攻击者可以替换路径中的某个组件为符号链接，指向共享目录外。因此实际文件操作使用 `openat` 系列系统调用逐级打开：
  1. 以 `O_NOFOLLOW | O_DIRECTORY` 打开共享目录根，获得 root fd
  2. 逐级以 `O_NOFOLLOW | O_DIRECTORY` 打开路径中的每个目录组件，每一级都基于上一级的 fd（通过 `openat`）
  3. 最终在目标目录的 fd 上执行实际操作（`openat` / `mkdirat` / `unlinkat` / `renameat` 等）
  4. 任何一级遇到符号链接（`O_NOFOLLOW` 导致 `ELOOP`）时返回 `PATH_TRAVERSAL_DENIED`

  **符号链接的特殊处理**：`Symlink` 和 `ReadLink` 操作本身涉及符号链接，这两个操作仍使用 `openat` 逐级打开到目标的父目录，仅在最后一级允许符号链接。`ReadLink` 读取到的目标路径不做穿越检查（返回给 Server 的是符号链接的原始内容）

  Go 标准库中 `openat` 等系统调用通过 `golang.org/x/sys/unix` 包提供

**NFS 通道**：
- 建议仅在可信局域网使用
- NFS URL 需做白名单校验（可配置允许的 IP 段或主机名），防止恶意 Client 注册指向攻击者的地址
- Server mount 时使用 `nosuid,nodev` 选项，防止特殊文件攻击
- Server 端路径穿越检查由 `LocalStorageBackend` 已有的逻辑保障

---

## 7. 对现有功能的影响

### 7.1 Sandbox

两种通道下 Sandbox 都**正常工作，零改动**：

- **gRPC 通道**：Server 端 FUSE 挂载让 `/var/lib/workspace/{workspace_id}/` 表现为普通目录，Docker bind mount 正常工作
- **NFS 通道**：NFS mount 点同样是普通目录，Docker bind mount 正常工作

Sandbox 创建流程中的目录存在性检查、bind mount 构造、Agent 通信等环节均无需改动。

**Server 重启场景**：Sandbox 容器的 bind mount 在 Server 重启后可能失效（FUSE 挂载点消失导致 stale mount）。恢复策略见 8.3 节——优先使用 `rshared` mount propagation 自动恢复，不支持时自动重启 Sandbox 容器。

**Workspace 删除**：删除 remote workspace 前，必须先停止关联的 Sandbox 容器（释放对挂载点的引用），否则 umount 会失败（`device is busy`）。删除流程：停止 Sandbox → umount → 清理 DB。

### 7.2 NFS Export

嵌入式 NFS server 通过 `StorageBackend` trait 访问文件。对于 remote workspace：
- **gRPC 通道**：NFS server 操作 FUSE 挂载点上的文件，和操作本地文件一致
- **NFS 通道**：NFS server 操作 NFS mount 点上的文件。这是用户态 NFS re-export（nfsserve crate 在用户态实现，不受内核 NFS re-export 限制）

### 7.3 FUSE Client

FUSE Client 通过 gRPC `FileSystemService` 访问 Server，Server 内部路由到对应的存储后端。FUSE Client 不感知底层是本地、S3 还是 remote。

### 7.4 MCP

MCP 工具调用最终也通过 WorkspaceService 访问文件，透明兼容。

---

## 8. 可靠性设计

### 8.1 gRPC 通道断线处理

**Server 端**：
- Client 断线后，Server 端 FUSE 挂载保留，RemoteStorageBackend 标记为断线状态
- 断线时**主动清理所有 pending 请求**（遍历 DashMap，向所有 oneshot::Sender 发送错误），避免资源泄漏
- 后续新的文件操作进入等待队列，有独立超时（默认 30 秒）
- 超时后返回 `StorageError::Io`，上层 FUSE 转换为 EIO 返回给调用方

**Client 端**：
- SDK 内置指数退避重连逻辑（1s → 2s → 4s → ... → 最大 30s）
- 重连成功后，Server 重新绑定流到 Workspace，FUSE 挂载自动恢复工作
- 重连期间，Client 本地目录上的修改会在重连后自然生效（下次读取时拿到最新数据）

### 8.2 NFS 通道断线处理

- Server 端定期检查 `/proc/mounts`（每 30 秒，复用现有 S3fsMountMonitor 的模式）
- mount 丢失时自动尝试 remount
- remount 失败时标记 Workspace 状态为 disconnected
- NFS 内核客户端自身有重试机制（soft mount 模式，超时后返回 EIO）

### 8.3 Server 重启

- Server 重启后，remote Workspace 运行时状态重置为 `pending`

**NFS 通道恢复**：
- Server 启动时读取 DB 中 `storage_config`，对所有 `transport=nfs` 的 workspace 自动 remount
- remount 成功后状态恢复为 `connected`

**gRPC 通道恢复**：
- FUSE 挂载不存在，等待 Client 重新连接后重建
- Client SDK 的指数退避重连逻辑会自动重连

**Sandbox 自动恢复**：

Server 重启后，原有 Sandbox 容器的 bind mount 会因 FUSE 挂载点消失而变为 stale（`Transport endpoint is not connected`）。恢复流程：

1. Server 启动时，查询所有 `storage_type=remote` 的 workspace，记录其关联的 Sandbox 容器列表
2. 当 remote workspace 的存储恢复（gRPC 通道：Client 重连且 FUSE 挂载重建；NFS 通道：remount 成功）后，检查是否有关联的 Sandbox 容器
3. 如果有，自动重启这些 Sandbox 容器（`docker restart`），使其重新获得有效的 bind mount
4. 重启完成后，通过现有的 Sandbox 事件通知机制告知上层

**部署要求**：为使 FUSE 重新挂载后容器内的 bind mount 能自动生效（避免必须重启容器），Docker daemon 应配置 mount propagation 为 `rshared`。具体做法：
- Server 进程挂载 FUSE 时，对 `/var/lib/workspace/` 设置 `mount --make-rshared`
- Docker 创建 Sandbox 时，bind mount 使用 `:rshared` propagation 选项（如 `-v /var/lib/workspace/{id}:/workspace:rshared`）
- 如果部署环境支持此配置，FUSE 重新挂载后容器内可自动恢复，无需重启容器。如果不支持，则回退到自动重启容器的方式

### 8.4 Lease 管理

remote Workspace 复用现有 Lease 机制，无特殊处理：
- 通过 WorkspaceService 的写操作正常走 `ensure_lease_held` 检查
- 通过 NFS 和 gRPC FileSystemService 直接访问 StorageBackend 的路径不经过 Lease 检查——这是现有系统的已知限制，不在本设计范围内解决

**remote workspace 的额外风险**：对于 remote workspace，Client 本地进程可以直接修改共享目录中的文件，完全绕过 Server 的 Lease 检查。这意味着可能存在三方并发写入同一文件：(1) Client 本地进程直接写入、(2) 消费方通过 WorkspaceService 写入、(3) 消费方通过 NFS/FUSE 绕过 Lease 写入。此风险在已知限制中列出（见第 9 节）

### 8.5 可观测性

remote storage 涉及跨网络的分布式文件操作，生产环境必须有完善的可观测性支撑排障和性能调优。

**Metrics（Prometheus 格式）**：

| 指标名 | 类型 | 标签 | 说明 |
|--------|------|------|------|
| `remote_storage_operation_duration_seconds` | Histogram | `workspace_id`, `operation`, `transport` | 文件操作端到端延迟（P50/P95/P99） |
| `remote_storage_operation_total` | Counter | `workspace_id`, `operation`, `transport`, `status` | 文件操作计数（status: success/error/timeout） |
| `remote_storage_cache_hit_total` | Counter | `workspace_id`, `cache_type` | 缓存命中计数（cache_type: inode_attr/dir_content/kernel_entry） |
| `remote_storage_cache_miss_total` | Counter | `workspace_id`, `cache_type` | 缓存未命中计数 |
| `remote_storage_pending_requests` | Gauge | `workspace_id` | 当前等待 Client 响应的请求数 |
| `remote_storage_data_transfer_bytes_total` | Counter | `workspace_id`, `direction` | 数据流传输字节数（direction: read/write） |
| `remote_storage_client_connection_state` | Gauge | `workspace_id` | Client 连接状态（0=pending, 1=connected, 2=disconnected） |
| `remote_storage_fuse_mount_healthy` | Gauge | `workspace_id` | FUSE 挂载健康状态（0=unhealthy, 1=healthy） |
| `remote_storage_file_change_notifications_total` | Counter | `workspace_id` | 收到的文件变更通知数 |

**Logging（结构化日志）**：

- **INFO 级别**：Client 连接/断线/重连、通道切换成功、FUSE 挂载创建/销毁、NFS mount/umount
- **WARN 级别**：操作超时、Client 反压触发（channel 满载）、inotify watch 数量接近上限、FUSE 健康检查失败并触发重建、缓存命中率低于阈值（< 50%）
- **ERROR 级别**：FUSE 挂载重建失败、通道切换失败并回滚、路径穿越被拒绝（含请求路径，用于安全审计）、数据流关联超时

所有日志携带 `workspace_id` 和 `transport` 字段，便于按 workspace 过滤。

**Alerting（建议规则）**：

| 告警 | 条件 | 严重级别 |
|------|------|----------|
| Client 长时间断线 | `remote_storage_client_connection_state == 2` 持续 > 5 分钟 | Warning |
| 操作延迟异常 | `remote_storage_operation_duration_seconds` P95 > 5 秒，持续 > 2 分钟 | Warning |
| FUSE 挂载异常 | `remote_storage_fuse_mount_healthy == 0` 持续 > 1 分钟 | Critical |
| 高错误率 | 操作错误率 > 10%，持续 > 3 分钟 | Warning |
| 请求积压 | `remote_storage_pending_requests` > 100，持续 > 1 分钟 | Warning |

### 8.6 容量规划

**单个 remote workspace 的资源占用估算**：

| 资源 | gRPC 通道 | NFS 通道 |
|------|-----------|----------|
| FUSE 挂载 | 1 个 `/dev/fuse` fd + 内核 inode/dentry 缓存 | 无 |
| gRPC 流 | 1 条控制流（持久双向流）+ 0~8 条数据流（按需） | 无 |
| 内存（应用层缓存） | ~2-10 MB（取决于目录规模和缓存 TTL） | ~0（LocalStorageBackend 无额外缓存） |
| 内存（pending 请求 DashMap） | ~1 KB/请求 × 最大 128 并发 ≈ 128 KB | 无 |
| 线程/goroutine | 1 个 tokio task（FUSE 事件循环） | 独立阻塞线程池（每 workspace 最大 4 线程） |
| 文件描述符 | 1（/dev/fuse）+ 临时打开的文件 fd | NFS mount 的内核 fd |

**单台 Server 容量上限**：

- **FUSE 挂载数量**：受 `/dev/fuse` 设备和内核资源限制。Linux 默认无硬上限，但每个 FUSE 挂载消耗内核内存（约 100-200 KB）。建议单台 Server 的 gRPC 通道 remote workspace 不超过 **200 个**
- **gRPC 流数量**：每个 remote workspace 至少 1 条持久流 + 最多 8 条数据流。200 个 workspace 最坏情况 1800 条流，在 gRPC/HTTP2 的能力范围内
- **NFS 通道**：NFS mount 资源消耗较低，上限主要受内核 NFS 客户端并发连接限制，建议不超过 **500 个**
- **混合部署**：建议 gRPC + NFS 的 remote workspace 总数不超过 **300 个**

**配额控制**：Server 配置项 `max_remote_workspaces`（默认 200），超出时创建 remote workspace 返回 `RESOURCE_EXHAUSTED` 错误。此限制与 managed workspace 独立计算。

---

## 9. 已知限制

| 限制 | 说明 | 影响范围 | 缓解措施 |
|------|------|----------|----------|
| gRPC 通道延迟 | 每次文件操作经过 gRPC RTT + FUSE 内核开销，比本地文件系统慢 | 密集 stat 调用场景（如 git status）体感明显 | FUSE 内核缓存（TTL 可配置）+ 文件变更通知主动失效（3.2/3.6 节） |
| FUSE Client 双重穿越 | FUSE Client 消费方访问 remote workspace 时经过两次 FUSE 内核穿越和两次 gRPC 往返 | FUSE Client 访问 remote workspace 的所有操作 | 已预留直连优化接口（3.2 节），后续可实现 Server 代理转发绕过 Server 端 FUSE |
| gRPC 通道无文件锁 | flock/fcntl 无法通过 gRPC 代理 | 依赖文件锁的工具（部分数据库、编辑器并发编辑） | 切换 NFS 通道（支持 NLM 文件锁） |
| gRPC 通道无 mmap | 内存映射文件无法透传 | 依赖 mmap 的工具（git 部分操作、某些编译器） | 切换 NFS 通道 |
| 大文件性能 | gRPC 通道受限于 protobuf 序列化和网络带宽 | 超大文件传输场景 | 切换 NFS 通道；数据流分离减轻对元数据操作的影响 |
| 单 Client 限制 | 同一 remote Workspace 同一时间只能有一个 Client 提供存储 | 如需多 Client 写入，应使用独立 Workspace | - |
| 多方写入冲突 | Client 本地进程可直接修改文件，绕过 Server 的 Lease 检查，可能与消费方的写入冲突 | 多方同时写同一文件的场景 | 使用者需自行协调写入；建议单一写入者模式 |
| 文件变更通知尽力而为 | Client 断线期间的文件变更无法通知 Server，缓存可能短暂 stale | Client 断线重连后的短暂窗口 | 重连时全量清除缓存；TTL 自然过期 |
| inotify 资源限制 | 大型项目可能超出 inotify watch 数量限制 | 目录/文件数量极多的项目 | `.elevoignore` 排除构建产物目录；降级为定期全量缓存清除（3.6 节） |
| FUSE 部署依赖 | Server 端需要 FUSE 支持、allow_other 配置 | 容器化部署可能需要额外权限（`--device /dev/fuse`） | NFS 通道不依赖 FUSE |
| 容量不受 Server 控制 | remote workspace 的存储容量完全取决于 Client 本地磁盘，Server 无法限制 | 磁盘空间管理 | Client 通过 StatFs 汇报磁盘使用情况，仅用于展示 |
| Server 端 remote workspace 数量 | 受 FUSE 挂载数、gRPC 流数和内存限制，单台 Server 建议不超过 200 个 gRPC 通道 workspace | 大规模部署场景 | 配额控制（8.6 节）；NFS 通道资源消耗更低，上限更高 |

---

## 10. 整体数据流总结

### 10.1 gRPC 通道

```
消费方 (FUSE Client / NFS Client / HTTP API / Sandbox)
  │
  │  正常的文件操作（read / write / ls / ...）
  ▼
/var/lib/workspace/{workspace_id}/      ← Server 端 FUSE 挂载点
  │                                        对消费方来说就是普通目录
  │  FUSE 内核回调
  ▼
Server 端 FUSE Filesystem
  │
  │  调用 StorageBackend trait 方法
  ▼
RemoteStorageBackend
  │
  │  元数据操作 → 控制流（双向流）
  │  大文件读写 → 数据流（独立 streaming RPC）
  ▼
gRPC (ClientStorageService)
  │
  ▼
Client (Go SDK)
  │  在本地目录执行文件操作
  ▼
本地文件系统 (/my/project)
```

### 10.2 NFS 通道

```
消费方 (FUSE Client / NFS Client / HTTP API / Sandbox)
  │
  │  正常的文件操作
  ▼
/var/lib/workspace/{workspace_id}/      ← NFS mount 点
  │                                        对消费方来说就是普通目录
  │  内核 VFS → NFS Client
  ▼
Client (NFS Server)
  │
  ▼
本地文件系统 (/my/project)
```

### 10.3 两种通道的统一视图

无论使用哪种通道，`/var/lib/workspace/{workspace_id}/` 始终是一个可访问的本地目录。区别仅在于这个目录的底层实现：

| 通道 | 目录底层 | Sandbox | NFS Export | FUSE Client | HTTP API |
|------|----------|---------|------------|-------------|----------|
| gRPC | Server 端 FUSE 挂载 | 正常 | 正常 | 正常 | 正常 |
| NFS | 内核 NFS mount | 正常 | 正常 | 正常 | 正常 |
| local（对比） | 真实本地目录 | 正常 | 正常 | 正常 | 正常 |

上层功能**全部零改动**。
