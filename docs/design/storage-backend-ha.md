# Storage Backend 抽象与 S3 存储集成

> **范围说明**：本文档聚焦于存储后端抽象层设计和 S3 存储集成方案。高可用相关内容（S3 集群部署、数据备份策略、Server 实例健康检查与自动摘除等）将在单独的 HA 方案文档中阐述。

## 1. 背景与现状

### 1.1 当前架构

当前 Workspace Server 的文件存储完全依赖本地文件系统：

```
WorkspaceService (service/workspace.rs)
    └── tokio::fs 直接操作 /var/lib/workspace/{workspace_id}/
        （create/delete workspace 使用 std::fs 同步操作，在 async 函数中阻塞）

NfsManager (infra/nfs.rs)
    └── std::fs 直接操作本地磁盘（在 async 上下文中阻塞 tokio 工作线程）
        inode 映射使用双向 HashMap：path_to_id (PathBuf → fileid3) + id_to_path (fileid3 → PathBuf)
        两个 HashMap 均使用 std::sync::RwLock 保护

SandboxService (service/sandbox.rs)
    └── Docker bind mount: host_path → /workspace
```

### 1.2 存在的问题

1. **单点故障**：存储绑定在单台机器的本地磁盘，磁盘损坏或节点宕机即数据丢失
2. **无法水平扩展**：多个 Server 实例无法共享同一份 workspace 数据
3. **无存储抽象**：`WorkspaceService` 中 10 个文件操作方法（`read_file`, `write_file`, `list_files`, `mkdir`, `delete_file`, `move_file`, `copy_file`, `get_file_info`, `exists`, `read_file_string`）直接调用 `tokio::fs`，整体无法替换后端
4. **WorkspaceService 混用同步/异步**：`create()` 和 `delete()` 方法中使用 `std::fs::create_dir_all()` / `std::fs::remove_dir_all()` 同步操作，在 async 函数中阻塞 tokio 工作线程
5. **NFS 层紧耦合**：`WorkspaceNfs` 实现了 `NFSFileSystem` trait，内部全部使用 `std::fs` 同步操作（约 20+ 处阻塞调用），在 async 上下文中阻塞 tokio 工作线程

## 2. 目标

1. 引入 `StorageBackend` trait，将文件操作从具体实现中解耦
2. 实现 `LocalStorageBackend` 作为唯一后端实现，同时服务 local 和 S3 两种模式
3. `WorkspaceService` 和 `WorkspaceNfs` 统一通过 trait 访问存储
4. 修复 `WorkspaceService` 和 NFS 层在 async 上下文中使用阻塞 `std::fs` 的问题
5. S3 模式下通过 s3fs-fuse 将 S3 bucket 挂载为本地目录，所有组件统一通过本地文件系统访问

## 3. 技术选型

### 3.1 RustFS

[RustFS](https://github.com/rustfs/rustfs) 是 Rust 实现的 S3 兼容高性能对象存储系统。

- **协议**：完全兼容 S3 API（V2/V4 签名）
- **性能**：4KB 小对象场景下 2.3x 于 MinIO
- **一致性**：strict read-after-write 强一致
- **许可证**：Apache 2.0
- **当前状态**：alpha (v1.0.0-alpha.82)，不建议直接用于生产关键路径
- **运行时**：Tokio async，与项目技术栈一致

> 注意：由于 RustFS 尚处 alpha 阶段，架构设计上应保证 local 模式作为默认后端，S3 模式作为可选升级路径。底层对接的是标准 S3 协议，可随时切换到 MinIO、AWS S3 等任何 S3 兼容存储。

### 3.2 s3fs-fuse

[s3fs-fuse](https://github.com/s3fs-fuse/s3fs-fuse) 将 S3 bucket 挂载为本地 POSIX 文件系统。

- **POSIX 兼容性**：支持大部分 POSIX 语义（随机读写、rename、symlink、chmod、uid/gid），但存在以下限制：
  - `rename` 非原子操作：S3 不支持原生 rename，s3fs 内部实现为 copy + delete，大文件 rename 延迟可能达秒级
  - `hard link` 不支持
  - `flock` / `fcntl` 文件锁不支持
  - `mmap` 不支持
  - `chmod` / `chown` 通过 S3 object metadata 模拟，性能较低
- **兼容性**：支持 S3 兼容存储（RustFS/MinIO），通过 `url` + `use_path_request_style` 参数指定自定义 endpoint
- **缓存**：内置 `use_cache` 选项，支持本地文件缓存加速读取；内置 stat 缓存减少元数据请求
- **许可证**：GPL-2.0（仅作为外部工具调用，不链接到项目代码中；项目仅做服务端部署不做分发，无许可证兼容性问题）
- **成熟度**：生产就绪，广泛使用

宿主机需安装 `s3fs`（Debian/Ubuntu: `apt install s3fs`，RHEL/CentOS: `yum install s3fs-fuse`）及 `fuse` 内核模块。

### 3.3 架构决策：统一走 s3fs-fuse

S3 模式下，**所有组件**（WorkspaceService、NFS、Docker）统一通过 s3fs-fuse 挂载的本地目录访问文件：

```
                    s3fs-fuse 挂载
S3 Bucket ──────────────────────────── /var/lib/workspace/
                                            │
                    ┌───────────────────────┼───────────────────────┐
                    │                       │                       │
            WorkspaceService          WorkspaceNfs            Docker bind mount
            (StorageBackend)          (StorageBackend)        (host_path → /workspace)
```

**为什么不直接调用 S3 API**：

1. **一致性**：如果 WorkspaceService/NFS 走 S3 API 而 Docker 走 s3fs-fuse，两条路径各自有独立缓存，会导致数据不一致（API 写入后容器内 30 秒内看不到更新，反之亦然）
2. **复杂度**：S3 不支持原生随机写入（seek + write），需要读-改-写整文件 + 乐观锁，NFS 高频小块写入场景下性能极差
3. **语义差异**：S3 无目录概念、无 symlink、无 POSIX 权限，需要大量模拟逻辑
4. **维护成本**：s3fs-fuse 已经解决了上述所有问题，且经过生产验证

统一走 s3fs-fuse 后，`LocalStorageBackend` 是唯一的后端实现，S3 模式只是挂载点不同（从本地磁盘变为 s3fs-fuse 挂载点），代码路径完全一致。

**s3fs-fuse 的已知限制及影响**：

| 限制 | 对本项目的影响 | 应对策略 |
|------|---------------|---------|
| `rename` 非原子（copy + delete） | NFS rename 操作在大文件场景下延迟增大 | workspace 中文件通常较小（代码文件）；大文件场景需在文档中提醒用户 |
| 无文件锁（flock/fcntl） | 多进程并发写同一文件无法通过文件锁协调 | 通过应用层的 workspace 级别并发控制解决（见第 10.5 节） |
| `chmod`/`chown` 性能差 | NFS setattr 操作中的权限修改延迟增大 | 可接受，权限修改不是高频操作 |
| 无 `mmap` 支持 | 不影响，StorageBackend 不使用 mmap | 无需处理 |

## 4. 详细设计

### 4.1 StorageBackend Trait

新增文件：`server/src/infra/storage/mod.rs`

```rust
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// 文件类型
#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    File,
    Directory,
    Symlink,
}

/// 文件元信息
#[derive(Debug, Clone)]
pub struct FileStat {
    pub name: String,
    pub path: String,
    pub file_type: FileType,
    pub size: u64,
    /// Unix 权限模式 (如 0o644)
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub modified_at: Option<DateTime<Utc>>,
    pub accessed_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}

/// 存储后端错误类型
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("is a directory: {0}")]
    IsADirectory(String),
    #[error("not a directory: {0}")]
    NotADirectory(String),
    #[error("not a file: {0}")]
    NotAFile(String),
    #[error("directory not empty: {0}")]
    DirectoryNotEmpty(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("path traversal denied: {0}")]
    PathTraversalDenied(String),
    #[error("operation not supported: {0}")]
    NotSupported(String),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("storage backend error: {0}")]
    Internal(String),
}

pub type StorageResult<T> = std::result::Result<T, StorageError>;

/// 存储后端抽象 trait
///
/// 所有路径参数均为 workspace 内部的相对路径，不包含 workspace_id 前缀。
/// 实现者负责将相对路径映射到实际存储位置。
///
/// 例如：path = "src/main.rs"
///   - LocalBackend → /var/lib/workspace/{workspace_id}/src/main.rs
#[async_trait]
pub trait StorageBackend: Send + Sync + 'static {
    // ── 文件读写 ──

    /// 读取文件全部内容
    async fn read_file(&self, workspace_id: &str, path: &str) -> StorageResult<Vec<u8>>;

    /// 读取文件指定范围（用于 NFS 的 offset+count 读取）
    async fn read_file_range(
        &self, workspace_id: &str, path: &str, offset: u64, length: u32,
    ) -> StorageResult<Vec<u8>>;

    /// 写入文件（全量覆盖，如果文件已存在则覆盖，不存在则创建）
    async fn write_file(
        &self, workspace_id: &str, path: &str, content: &[u8],
    ) -> StorageResult<()>;

    /// 写入文件指定位置（用于 NFS 的 offset 写入）
    async fn write_file_at(
        &self, workspace_id: &str, path: &str, offset: u64, data: &[u8],
    ) -> StorageResult<()>;

    // ── 文件创建 ──

    /// 创建文件
    ///
    /// - `exclusive = true`：文件必须不存在，否则返回 `StorageError::AlreadyExists`
    ///   对应 NFS CREATE 的 GUARDED/EXCLUSIVE 模式
    /// - `exclusive = false`：如果文件已存在则截断为空，不存在则创建
    ///   对应 NFS CREATE 的 UNCHECKED 模式
    async fn create_file(
        &self, workspace_id: &str, path: &str, exclusive: bool,
    ) -> StorageResult<()>;

    // ── 元数据 ──

    /// 获取文件/目录元信息
    async fn stat(&self, workspace_id: &str, path: &str) -> StorageResult<FileStat>;

    /// 列出目录下的直接子项
    async fn list_dir(&self, workspace_id: &str, path: &str) -> StorageResult<Vec<FileStat>>;

    /// 检查文件或目录是否存在
    async fn exists(&self, workspace_id: &str, path: &str) -> StorageResult<bool>;

    // ── 目录操作 ──

    /// 创建目录
    ///
    /// - `recursive = true`：递归创建父目录（类似 `mkdir -p`），适用于 WorkspaceService
    /// - `recursive = false`：只创建单级目录，父目录不存在时返回 `StorageError::NotFound`，
    ///   适用于 NFS mkdir 操作
    async fn mkdir(
        &self, workspace_id: &str, path: &str, recursive: bool,
    ) -> StorageResult<()>;

    // ── 删除操作 ──

    /// 删除文件
    ///
    /// 如果路径指向目录，返回 `StorageError::IsADirectory`。
    /// 对应 NFS REMOVE 操作。
    async fn remove_file(&self, workspace_id: &str, path: &str) -> StorageResult<()>;

    /// 删除目录
    ///
    /// - `recursive = true`：递归删除目录及所有内容（类似 `rm -rf`），适用于 WorkspaceService
    /// - `recursive = false`：目录必须为空，否则返回 `StorageError::DirectoryNotEmpty`，
    ///   对应 NFS RMDIR 操作
    ///
    /// 如果路径指向文件，返回 `StorageError::NotADirectory`。
    async fn remove_dir(
        &self, workspace_id: &str, path: &str, recursive: bool,
    ) -> StorageResult<()>;

    // ── 移动/复制 ──

    /// 重命名/移动
    async fn rename(
        &self, workspace_id: &str, src: &str, dst: &str,
    ) -> StorageResult<()>;

    /// 复制文件或目录
    async fn copy(
        &self, workspace_id: &str, src: &str, dst: &str,
    ) -> StorageResult<()>;

    // ── Workspace 生命周期 ──

    /// 创建 workspace 根目录
    async fn create_workspace_root(&self, workspace_id: &str) -> StorageResult<()>;

    /// 删除 workspace 根目录及全部内容
    async fn delete_workspace_root(&self, workspace_id: &str) -> StorageResult<()>;

    // ── NFS 扩展操作 ──

    /// 设置文件大小（truncate，用于 NFS setattr）
    async fn set_file_size(
        &self, workspace_id: &str, path: &str, size: u64,
    ) -> StorageResult<()>;

    /// 创建符号链接
    ///
    /// `target` 为符号链接指向的目标路径。NFS 协议允许 symlink target 为任意字符串
    /// （可以是相对路径、绝对路径、甚至不存在的路径），此方法不对 target 做路径校验。
    ///
    /// 安全边界：
    /// - `link_path` 受 workspace 路径校验约束，必须位于 workspace 内部
    /// - `target` 不做校验，由 NFS 客户端负责解释。实际访问 symlink 目标时，
    ///   StorageBackend 的其他方法（read_file、stat 等）会对解析后的路径做安全校验
    async fn symlink(
        &self, workspace_id: &str, link_path: &str, target: &str,
    ) -> StorageResult<()>;

    /// 读取符号链接目标
    async fn readlink(&self, workspace_id: &str, path: &str) -> StorageResult<String>;
}
```

### 4.2 设计说明

#### 4.2.1 为什么需要 `read_file_range` 和 `write_file_at`

NFS 协议的读写操作基于 offset + length 的随机访问模式。如果只有全量 `read_file`/`write_file`，NFS 层每次 read 都需要读取整个文件再截取，性能不可接受。

- `read_file_range`：对应 `seek()` + `read_exact()`
- `write_file_at`：对应 `OpenOptions::write(true)` + `seek()` + `write_all()`

#### 4.2.2 为什么不用泛型而用 trait object

`WorkspaceService` 和 `NfsManager` 使用 `Arc<dyn StorageBackend>`，避免在 `AppState`、`NfsManager` 等结构体上传播泛型参数。虽然当前只有 `LocalStorageBackend` 一个实现，trait object 仍然有价值：

1. 单元测试中可以注入 mock 实现
2. 解耦 `tokio::fs` 硬编码，统一路径安全校验
3. 为 NFS 层提供 async 接口，解决阻塞问题

#### 4.2.3 workspace_id 由谁传入

所有方法的第一个参数都是 `workspace_id`，由 `WorkspaceService` 或 `NfsManager` 传入。`StorageBackend` 内部负责将 `(workspace_id, path)` 映射到实际存储路径。

#### 4.2.4 FileStat 与 FileInfo 的关系

当前 `WorkspaceService` 对外暴露 `FileInfo`（name, path, file_type, size, modified_at），这是面向 HTTP API 的 DTO。新增的 `FileStat` 是面向存储层的完整元数据结构，包含 POSIX 属性（uid/gid/mode 等），主要服务于 NFS 层。

```rust
impl From<FileStat> for FileInfo {
    fn from(stat: FileStat) -> Self {
        FileInfo {
            name: stat.name,
            path: stat.path,
            file_type: match stat.file_type {
                FileType::Directory => "directory".to_string(),
                FileType::File => "file".to_string(),
                FileType::Symlink => "symlink".to_string(),
            },
            size: stat.size,
            modified_at: stat.modified_at,
        }
    }
}
```

#### 4.2.5 read_file_string 的处理

`WorkspaceService::read_file_string()` 是 `read_file()` + UTF-8 转换的组合方法。这是应用层逻辑，不属于存储抽象，继续作为 `WorkspaceService` 的辅助方法保留。

#### 4.2.6 create_file 与 write_file 的区分

`create_file` 和 `write_file` 的职责不同：

- `write_file`：全量写入内容，文件不存在时创建、存在时覆盖。用于 WorkspaceService 的文件写入场景。
- `create_file`：只创建文件（不写入内容），支持 `exclusive` 模式控制是否允许已存在。用于 NFS CREATE 操作的三种模式（UNCHECKED/GUARDED/EXCLUSIVE）。

NFS 的 `create` 操作如果统一映射到 `write_file(空内容)`，会丢失 GUARDED/EXCLUSIVE 语义——NFS 客户端期望在 GUARDED 模式下，如果文件已存在则返回 `NFS3ERR_EXIST` 而非静默覆盖。

#### 4.2.7 mkdir 的 recursive 参数

NFS 的 `mkdir` 操作只创建单级目录（父目录不存在时应返回错误），而 `WorkspaceService` 的 `mkdir` 操作需要递归创建（类似 `mkdir -p`，面向用户的友好行为）。通过 `recursive` 参数统一两种语义，避免拆分为两个方法。

#### 4.2.8 remove_file 与 remove_dir 的拆分

NFS 协议中 `REMOVE`（删除文件）和 `RMDIR`（删除目录）是不同操作，错误语义不同：

- 对目录调用 REMOVE 应返回 `NFS3ERR_ISDIR`
- 对文件调用 RMDIR 应返回 `NFS3ERR_NOTDIR`

将 trait 拆分为 `remove_file` 和 `remove_dir` 两个方法，使错误处理更精确，避免在 NFS 层额外做类型判断。

#### 4.2.9 WorkspaceNfs inode 映射变更

当前 `WorkspaceNfs` 使用双向 HashMap 做 inode 映射：`path_to_id: HashMap<PathBuf, fileid3>` + `id_to_path: HashMap<fileid3, PathBuf>`，直接存储完整本地路径。重构后改为：

- `path_to_id: HashMap<(String, String), fileid3>`，key 为 `(workspace_id, relative_path)` 元组
- `id_to_path: HashMap<fileid3, (String, String)>`，value 为 `(workspace_id, relative_path)` 元组

变更原因：
- `StorageBackend` 的所有方法都以 `(workspace_id, path)` 为参数，inode 映射需要存储这两个信息才能调用 backend
- 逻辑路径与物理路径解耦，backend 内部负责路径映射

> 注意：两个 HashMap 应使用 `tokio::sync::RwLock` 替代当前的 `std::sync::RwLock`，与全 async 架构保持一致，避免在 async 上下文中持有同步锁跨越 await 点。

### 4.3 LocalStorageBackend

新增文件：`server/src/infra/storage/local.rs`

```rust
pub struct LocalStorageBackend {
    /// workspace 根目录，如 /var/lib/workspace
    /// local 模式下指向本地磁盘，S3 模式下指向 s3fs-fuse 挂载点（同一路径）
    base_dir: PathBuf,
}

impl LocalStorageBackend {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// 解析路径：base_dir / workspace_id / path
    /// 包含路径安全校验，防止目录穿越
    ///
    /// 安全策略：检测到非法路径组件时立即报错（而非静默丢弃），
    /// 确保调用方明确知道路径被拒绝，避免操作了非预期的文件。
    fn resolve_path(&self, workspace_id: &str, path: &str) -> StorageResult<PathBuf> {
        // 第一道防线：检测非法路径组件，发现即报错
        for component in Path::new(path).components() {
            match component {
                std::path::Component::ParentDir => {
                    return Err(StorageError::PathTraversalDenied(
                        format!("path contains '..': {}", path)
                    ));
                }
                std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                    return Err(StorageError::PathTraversalDenied(
                        format!("path contains absolute component: {}", path)
                    ));
                }
                _ => {}
            }
        }

        let workspace_dir = self.base_dir.join(workspace_id);
        let full_path = workspace_dir.join(path);

        // 第二道防线：规范化后的路径必须仍在 workspace 目录内
        // 防止 symlink 或其他方式绕过组件检查
        let canonical_workspace = workspace_dir.canonicalize().unwrap_or(workspace_dir.clone());
        let canonical_full = full_path.canonicalize().unwrap_or(full_path.clone());
        if !canonical_full.starts_with(&canonical_workspace) {
            return Err(StorageError::PathTraversalDenied(
                format!("resolved path escapes workspace: {}", path)
            ));
        }

        Ok(full_path)
    }
}
```

实现说明：

| trait 方法 | 实现方式 |
|-----------|---------|
| `read_file` | `tokio::fs::read()` |
| `read_file_range` | `tokio::fs::File::open()` + `seek()` + `read_exact()` |
| `write_file` | 自动创建父目录 + `tokio::fs::write()` |
| `write_file_at` | `OpenOptions::write(true).open()` + `seek()` + `write_all()` |
| `create_file` | `exclusive=true`: `OpenOptions::create_new(true)`, `exclusive=false`: `OpenOptions::create(true).truncate(true)` |
| `stat` | `tokio::fs::symlink_metadata()` + `MetadataExt` 获取 uid/gid/mode |
| `list_dir` | `tokio::fs::read_dir()` + 收集所有 entry 的 metadata |
| `mkdir` | `recursive=true`: `tokio::fs::create_dir_all()`, `recursive=false`: `tokio::fs::create_dir()` |
| `remove_file` | `tokio::fs::remove_file()`，如果目标是目录则返回 `IsADirectory` |
| `remove_dir` | `recursive=true`: `tokio::fs::remove_dir_all()`, `recursive=false`: `tokio::fs::remove_dir()` |
| `rename` | `tokio::fs::rename()` |
| `copy` | 文件用 `tokio::fs::copy()`，目录递归复制 |
| `set_file_size` | `tokio::fs::File::open()` + `set_len()` |
| `symlink` | `tokio::fs::symlink()` (tokio::fs::os::unix) |
| `readlink` | `tokio::fs::read_link()` |
| `create_workspace_root` | `tokio::fs::create_dir_all()` |
| `delete_workspace_root` | `tokio::fs::remove_dir_all()` |
| `exists` | `tokio::fs::symlink_metadata().is_ok()` |

> **路径安全说明**：`resolve_path` 采用"检测即拒绝"策略——遇到 `..`、`/` 或 Windows 盘符组件时立即返回 `PathTraversalDenied` 错误。这与之前"静默过滤"的设计不同：静默丢弃路径组件可能导致用户以为在操作 `../file` 但实际操作了 `file`，行为不透明。显式拒绝让调用方明确知道路径被拒绝。第二道防线通过 `canonicalize` 检查处理 symlink 绕过的情况。

### 4.4 StorageError 与 std::io::Error 的映射

`LocalStorageBackend` 需要将 `std::io::Error` 转换为语义化的 `StorageError`，以便上层（WorkspaceService、NFS）做细粒度错误处理：

```rust
impl StorageError {
    /// 从 std::io::Error 转换，附带路径上下文
    pub fn from_io(err: std::io::Error, path: impl Into<String>) -> Self {
        let path = path.into();
        match err.kind() {
            std::io::ErrorKind::NotFound => StorageError::NotFound(path),
            std::io::ErrorKind::AlreadyExists => StorageError::AlreadyExists(path),
            std::io::ErrorKind::PermissionDenied => StorageError::PermissionDenied(path),
            // nightly feature: std::io::ErrorKind::IsADirectory
            // 在 stable Rust 中通过 raw_os_error 判断
            _ if err.raw_os_error() == Some(libc::EISDIR) => StorageError::IsADirectory(path),
            _ if err.raw_os_error() == Some(libc::ENOTDIR) => StorageError::NotADirectory(path),
            _ if err.raw_os_error() == Some(libc::ENOTEMPTY) => StorageError::DirectoryNotEmpty(path),
            _ => StorageError::Io { path, source: err },
        }
    }
}
```

通过 `libc` crate 提供的常量进行 errno 判断，避免硬编码平台相关的数值。需要在 `Cargo.toml` 中新增 `libc` 依赖。

### 4.5 s3fs-fuse 挂载管理

新增文件：`server/src/infra/storage/s3fs_mount.rs`

```rust
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// s3fs-fuse 挂载管理器
pub struct S3fsMountManager {
    /// 挂载点路径（即 workspace 根目录）
    mount_point: PathBuf,
    /// S3 bucket 名称
    bucket: String,
    /// S3 endpoint URL
    endpoint: String,
    /// S3 凭证（可选，未提供时依赖环境变量或 IAM role）
    credentials: Option<S3Credentials>,
    /// s3fs-fuse 本地文件缓存目录
    cache_dir: Option<PathBuf>,
}

pub struct S3Credentials {
    pub access_key: String,
    pub secret_key: String,
}

impl S3fsMountManager {
    /// 执行 s3fs 挂载
    ///
    /// 挂载前检查：
    /// 1. 挂载点目录是否存在（不存在则创建）
    /// 2. 是否已经挂载（已挂载则跳过）
    /// 3. s3fs 命令是否可用
    ///
    /// 注意：is_mounted + mount 之间存在 TOCTOU 竞态，但实际场景中
    /// 不会并发调用 mount()（仅在 Server 启动时调用一次），风险可接受。
    pub async fn mount(&self) -> Result<(), S3fsMountError> {
        // 检查是否已挂载
        if self.is_mounted().await {
            tracing::info!(mount_point = %self.mount_point.display(), "s3fs already mounted");
            return Ok(());
        }

        // 确保挂载点目录存在
        tokio::fs::create_dir_all(&self.mount_point).await?;

        // 如果配置了凭证，写入临时密码文件供 s3fs 使用
        let passwd_file = self.prepare_credentials().await?;

        // 构建 s3fs 命令
        let mut cmd = Command::new("s3fs");
        cmd.arg(&self.bucket)
            .arg(&self.mount_point)
            .arg("-o").arg(format!("url={}", self.endpoint))
            .arg("-o").arg("use_path_request_style")
            .arg("-o").arg("allow_other");

        // 指定凭证文件
        if let Some(ref passwd_path) = passwd_file {
            cmd.arg("-o").arg(format!("passwd_file={}", passwd_path.display()));
        }

        if let Some(ref cache_dir) = self.cache_dir {
            tokio::fs::create_dir_all(cache_dir).await?;
            cmd.arg("-o").arg(format!("use_cache={}", cache_dir.display()));
        }

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(S3fsMountError::MountFailed {
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        // 验证挂载成功
        if !self.is_mounted().await {
            return Err(S3fsMountError::MountVerificationFailed);
        }

        tracing::info!(mount_point = %self.mount_point.display(), "s3fs mounted successfully");
        Ok(())
    }

    /// 准备凭证文件
    ///
    /// 如果配置了 S3 凭证，写入临时文件并设置 600 权限。
    /// s3fs 要求密码文件权限不能高于 600。
    async fn prepare_credentials(&self) -> Result<Option<PathBuf>, S3fsMountError> {
        let Some(ref creds) = self.credentials else {
            return Ok(None);
        };

        let passwd_path = self.mount_point.parent()
            .unwrap_or(Path::new("/tmp"))
            .join(".s3fs_passwd");

        let content = format!("{}:{}", creds.access_key, creds.secret_key);
        tokio::fs::write(&passwd_path, content.as_bytes()).await?;

        // 设置 600 权限
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(&passwd_path, perms).await?;
        }

        Ok(Some(passwd_path))
    }

    /// 卸载 s3fs
    pub async fn unmount(&self) -> Result<(), S3fsMountError> {
        if !self.is_mounted().await {
            return Ok(());
        }

        let output = Command::new("fusermount")
            .arg("-u")
            .arg(&self.mount_point)
            .output()
            .await?;

        if !output.status.success() {
            return Err(S3fsMountError::UnmountFailed {
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        Ok(())
    }

    /// 检查挂载点是否已挂载
    ///
    /// 通过读取 /proc/mounts 检查，而非简单判断目录是否存在
    pub async fn is_mounted(&self) -> bool {
        let mount_point_str = self.mount_point.to_string_lossy();
        match tokio::fs::read_to_string("/proc/mounts").await {
            Ok(mounts) => mounts.lines().any(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.len() >= 2 && parts[1] == mount_point_str.as_ref()
            }),
            Err(_) => false,
        }
    }

    /// 检查并清理 stale mount（进程异常退出后残留的挂载点）
    ///
    /// 启动时调用。如果挂载点存在但不可访问（stale mount），
    /// 先执行 fusermount -u 清理，再重新挂载。
    pub async fn cleanup_stale_mount(&self) -> Result<(), S3fsMountError> {
        if !self.mount_point.exists() {
            return Ok(());
        }

        // 尝试访问挂载点，如果失败说明是 stale mount
        match tokio::fs::read_dir(&self.mount_point).await {
            Ok(_) => Ok(()), // 可访问，不是 stale
            Err(e) if e.kind() == std::io::ErrorKind::Other
                || e.raw_os_error() == Some(libc::ENOTCONN)  // Transport endpoint is not connected
                || e.raw_os_error() == Some(libc::EACCES)    // Permission denied (另一种 stale 表现)
                => {
                tracing::warn!(
                    mount_point = %self.mount_point.display(),
                    "detected stale mount, cleaning up"
                );
                // 强制卸载
                let _ = Command::new("fusermount")
                    .arg("-uz") // lazy unmount
                    .arg(&self.mount_point)
                    .output()
                    .await;
                Ok(())
            }
            Err(_) => Ok(()),
        }
    }
}
```

#### 4.5.1 S3 凭证传递

s3fs-fuse 支持多种凭证获取方式（按优先级）：

1. 命令行参数 `-o passwd_file=<path>`（文件格式：`ACCESS_KEY:SECRET_KEY`，权限必须为 600）
2. 环境变量 `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY`
3. 凭证文件 `~/.passwd-s3fs`
4. IAM role（EC2/ECS 环境）

`S3fsMountManager` 的凭证处理策略：
- 如果 `StorageConfig::S3` 中配置了 `access_key` + `secret_key`，则写入临时密码文件（设置 600 权限），通过 `-o passwd_file` 传递给 s3fs
- 如果未配置，依赖外部环境变量或 IAM role
- 推荐容器化部署使用环境变量方式

#### 4.5.2 Graceful Shutdown

Server 退出时需要卸载 s3fs-fuse，避免残留挂载点。需要同时处理 `SIGINT`（Ctrl+C）和 `SIGTERM`（容器化环境下 Kubernetes 的默认终止信号）：

```rust
// main.rs 中注册 shutdown hook
let mount_manager = s3fs_mount_manager.clone();
tokio::spawn(async move {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(
        tokio::signal::unix::SignalKind::terminate()
    ).expect("failed to register SIGTERM handler");

    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => {},
        _ = sigterm.recv() => {},
    }

    #[cfg(not(unix))]
    ctrl_c.await.ok();

    tracing::info!("shutting down, unmounting s3fs...");
    if let Err(e) = mount_manager.unmount().await {
        tracing::error!("failed to unmount s3fs: {}", e);
    }
});
```

启动时调用 `cleanup_stale_mount()` 处理上次异常退出残留的 stale mount（如 SIGKILL 导致的未清理挂载点）。

### 4.6 S3 连接健康检查

Server 启动时，在 s3fs 挂载完成后执行健康检查，快速失败：

```rust
impl S3fsMountManager {
    /// 启动时健康检查：验证挂载点可读写
    pub async fn health_check(&self) -> Result<(), S3fsMountError> {
        let test_file = self.mount_point.join(".workspace_health_check");

        // 写入测试文件
        tokio::fs::write(&test_file, b"ok").await.map_err(|e| {
            S3fsMountError::HealthCheckFailed(format!("write failed: {}", e))
        })?;

        // 读回验证
        let content = tokio::fs::read(&test_file).await.map_err(|e| {
            S3fsMountError::HealthCheckFailed(format!("read failed: {}", e))
        })?;

        if content != b"ok" {
            return Err(S3fsMountError::HealthCheckFailed(
                "read-back content mismatch".to_string()
            ));
        }

        // 清理测试文件
        let _ = tokio::fs::remove_file(&test_file).await;

        tracing::info!("s3fs health check passed");
        Ok(())
    }
}
```

## 5. 配置设计

### 5.1 配置结构

```rust
pub struct Config {
    // ...existing fields...

    /// 存储配置
    pub storage: StorageConfig,
}

/// 存储配置（枚举，互斥选择）
pub enum StorageConfig {
    /// 本地磁盘存储（默认）
    Local {
        /// workspace 根目录
        workspace_dir: PathBuf,
    },
    /// S3 存储（通过 s3fs-fuse 挂载）
    S3 {
        /// workspace 根目录（同时作为 s3fs-fuse 挂载点）
        workspace_dir: PathBuf,
        /// S3 连接配置
        s3: S3Config,
    },
}

/// S3 连接配置
pub struct S3Config {
    /// S3 endpoint URL
    pub endpoint: String,
    /// S3 bucket 名称
    pub bucket: String,
    /// S3 access key（可选，也可通过环境变量传入）
    pub access_key: Option<String>,
    /// S3 secret key（可选，也可通过环境变量传入）
    pub secret_key: Option<String>,
    /// S3 region
    pub region: Option<String>,
    /// s3fs-fuse 本地缓存目录（可选，启用后加速读取）
    pub cache_dir: Option<PathBuf>,
}

impl StorageConfig {
    /// 获取 workspace 根目录（两种模式都有）
    pub fn workspace_dir(&self) -> &Path {
        match self {
            StorageConfig::Local { workspace_dir } => workspace_dir,
            StorageConfig::S3 { workspace_dir, .. } => workspace_dir,
        }
    }
}
```

### 5.2 环境变量映射

遵循项目现有的 `WORKSPACE_` 前缀约定：

| 环境变量 | 说明 | 默认值 |
|---------|------|--------|
| `WORKSPACE_STORAGE_TYPE` | 存储类型：`local` 或 `s3` | `local` |
| `WORKSPACE_WORKSPACE_DIR` | workspace 根目录（复用现有变量） | `/var/lib/workspace` |
| `WORKSPACE_S3_ENDPOINT` | S3 endpoint URL | - |
| `WORKSPACE_S3_BUCKET` | S3 bucket 名称 | - |
| `WORKSPACE_S3_ACCESS_KEY` | S3 access key | - |
| `WORKSPACE_S3_SECRET_KEY` | S3 secret key | - |
| `WORKSPACE_S3_REGION` | S3 region | `us-east-1` |
| `WORKSPACE_S3_CACHE_DIR` | s3fs-fuse 本地缓存目录 | - |

> 注意：`WORKSPACE_WORKSPACE_DIR` 是已有的环境变量，两种模式共用。S3 模式下它同时作为 s3fs-fuse 的挂载点路径。

### 5.3 配置校验

启动时校验：

- `WORKSPACE_STORAGE_TYPE=s3` 时，`WORKSPACE_S3_ENDPOINT` 和 `WORKSPACE_S3_BUCKET` 必填
- S3 凭证至少通过以下方式之一提供：`WORKSPACE_S3_ACCESS_KEY` + `WORKSPACE_S3_SECRET_KEY` 环境变量、`~/.passwd-s3fs` 文件、或 IAM role
- `WORKSPACE_WORKSPACE_DIR` 路径必须是绝对路径
- `WORKSPACE_S3_CACHE_DIR` 如果指定，必须是绝对路径且有写权限

## 6. 初始化流程

### 6.1 启动时序

```
Server 启动
    │
    ├── 解析配置 (Config)
    │
    ├── 根据 StorageConfig 初始化
    │   ├── Local 模式:
    │   │   └── LocalStorageBackend::new(workspace_dir)
    │   │
    │   └── S3 模式:
    │       ├── S3fsMountManager::cleanup_stale_mount()  // 清理残留挂载
    │       ├── S3fsMountManager::mount()                // 挂载 s3fs-fuse
    │       ├── S3fsMountManager::health_check()         // 健康检查
    │       └── LocalStorageBackend::new(workspace_dir)  // 同一个 backend
    │
    ├── Arc<dyn StorageBackend> 注入到:
    │   ├── WorkspaceService
    │   └── NfsManager
    │
    └── 启动 HTTP/NFS 服务
```

### 6.2 初始化代码

```rust
async fn init_storage(config: &Config) -> Result<(
    Arc<dyn StorageBackend>,
    Option<S3fsMountManager>,
)> {
    match &config.storage {
        StorageConfig::Local { workspace_dir } => {
            tokio::fs::create_dir_all(workspace_dir).await?;
            let backend = Arc::new(LocalStorageBackend::new(workspace_dir.clone()));
            Ok((backend as Arc<dyn StorageBackend>, None))
        }
        StorageConfig::S3 { workspace_dir, s3 } => {
            let credentials = match (&s3.access_key, &s3.secret_key) {
                (Some(ak), Some(sk)) => Some(S3Credentials {
                    access_key: ak.clone(),
                    secret_key: sk.clone(),
                }),
                _ => None,
            };

            let mount_manager = S3fsMountManager::new(
                workspace_dir.clone(),
                s3.bucket.clone(),
                s3.endpoint.clone(),
                credentials,
                s3.cache_dir.clone(),
            );

            // 清理可能的 stale mount
            mount_manager.cleanup_stale_mount().await?;

            // 挂载 s3fs-fuse
            mount_manager.mount().await?;

            // 健康检查
            mount_manager.health_check().await?;

            // 复用同一个 LocalStorageBackend
            let backend = Arc::new(LocalStorageBackend::new(workspace_dir.clone()));
            Ok((backend as Arc<dyn StorageBackend>, Some(mount_manager)))
        }
    }
}
```

关键点：**S3 模式和 Local 模式使用完全相同的 `LocalStorageBackend`**，区别仅在于 `workspace_dir` 指向的是 s3fs-fuse 挂载点还是本地磁盘。

## 7. WorkspaceService 迁移

### 7.1 结构变更

```rust
pub struct WorkspaceService {
    // 移除: workspace_dir: PathBuf
    storage: Arc<dyn StorageBackend>,
    // ...其他字段保持不变
}
```

### 7.2 方法迁移对照

| 原方法 | 迁移后 |
|--------|--------|
| `read_file()` → `tokio::fs::read()` | `self.storage.read_file(workspace_id, path)` |
| `read_file_string()` → `tokio::fs::read()` + `String::from_utf8` | `self.storage.read_file()` + `String::from_utf8`（组合逻辑保留在 WorkspaceService） |
| `write_file()` → `tokio::fs::write()` | `self.storage.write_file(workspace_id, path, content)` |
| `list_files()` → `tokio::fs::read_dir()` | `self.storage.list_dir(workspace_id, path)` + `FileStat → FileInfo` 转换 |
| `mkdir()` → `tokio::fs::create_dir_all()` | `self.storage.mkdir(workspace_id, path, true)` |
| `delete_file()` → `tokio::fs::remove_file/dir_all()` | 根据类型调用 `self.storage.remove_file()` 或 `self.storage.remove_dir(recursive)` |
| `move_file()` → `tokio::fs::rename()` | `self.storage.rename(workspace_id, src, dst)` |
| `copy_file()` → `tokio::fs::copy()` | `self.storage.copy(workspace_id, src, dst)` |
| `get_file_info()` → `tokio::fs::metadata()` | `self.storage.stat(workspace_id, path)` + `FileStat → FileInfo` |
| `exists()` → `tokio::fs::metadata().is_ok()` | `self.storage.exists(workspace_id, path)` |
| `create()` → `std::fs::create_dir_all()` | `self.storage.create_workspace_root(workspace_id)` |
| `delete()` → `std::fs::remove_dir_all()` | `self.storage.delete_workspace_root(workspace_id)` |

### 7.3 路径解析变更

当前 `WorkspaceService` 内部有 `resolve_path` 方法做路径安全校验。迁移后：

- **路径安全校验下沉到 `LocalStorageBackend::resolve_path`**
- `WorkspaceService` 不再直接拼接文件路径，只传递 `(workspace_id, relative_path)` 给 backend
- `WorkspaceService` 仍然负责校验 workspace_id 的合法性（是否存在、权限等业务逻辑）

## 8. NFS 层迁移

### 8.1 当前问题

`WorkspaceNfs` 实现 `NFSFileSystem` trait 时，内部全部使用 `std::fs` 同步操作。`nfsserve` crate 的 `NFSFileSystem` trait 方法是 async 的，但当前实现在 async 函数体内调用阻塞的 `std::fs`，会阻塞 tokio 工作线程。

### 8.2 迁移方案

将 `WorkspaceNfs` 内部的所有 `std::fs` 调用替换为 `StorageBackend` 的 async 方法：

```rust
pub struct WorkspaceNfs {
    // 移除: base_dir: PathBuf
    storage: Arc<dyn StorageBackend>,
    workspace_id: String,
    // inode 映射改为逻辑路径（双向映射）
    path_to_id: tokio::sync::RwLock<HashMap<(String, String), fileid3>>,
    id_to_path: tokio::sync::RwLock<HashMap<fileid3, (String, String)>>,
    next_fileid: std::sync::atomic::AtomicU64,
    // ...
}
```

NFS 操作到 StorageBackend 方法的映射：

| NFS 操作 | 当前实现 (std::fs) | 迁移后 (StorageBackend) |
|----------|-------------------|------------------------|
| `read` | `File::open()` + `seek()` + `read()` | `storage.read_file_range()` |
| `write` | `OpenOptions::write()` + `seek()` + `write()` | `storage.write_file_at()` |
| `getattr` | `fs::metadata()` | `storage.stat()` |
| `setattr` (size) | `File::set_len()` | `storage.set_file_size()` |
| `lookup` | `fs::metadata()` | `storage.stat()` |
| `readdir` | `fs::read_dir()` | `storage.list_dir()` |
| `create` | `File::create()` | `storage.create_file(exclusive)` |
| `remove` | `fs::remove_file()` | `storage.remove_file()` |
| `rmdir` | `fs::remove_dir()` | `storage.remove_dir(recursive=false)` |
| `rename` | `fs::rename()` | `storage.rename()` |
| `mkdir` | `fs::create_dir()` | `storage.mkdir(recursive=false)` |
| `symlink` | `os::unix::fs::symlink()` | `storage.symlink()` |
| `readlink` | `fs::read_link()` | `storage.readlink()` |

### 8.3 FileStat 到 NFS fattr3 的转换

```rust
impl From<FileStat> for fattr3 {
    fn from(stat: FileStat) -> Self {
        fattr3 {
            ftype: match stat.file_type {
                FileType::File => ftype3::NF3REG,
                FileType::Directory => ftype3::NF3DIR,
                FileType::Symlink => ftype3::NF3LNK,
            },
            mode: stat.mode,
            nlink: 1,
            uid: stat.uid,
            gid: stat.gid,
            size: stat.size,
            used: stat.size,
            rdev: specdata3 { specdata1: 0, specdata2: 0 },
            fsid: 0,
            fileid: 0, // 由 inode_map 填充
            atime: to_nfstime3(stat.accessed_at),
            mtime: to_nfstime3(stat.modified_at),
            ctime: to_nfstime3(stat.modified_at),
        }
    }
}
```

### 8.4 NFS 错误码映射

`StorageError` 到 NFS `nfsstat3` 的映射：

| StorageError | nfsstat3 |
|-------------|----------|
| `NotFound` | `NFS3ERR_NOENT` |
| `AlreadyExists` | `NFS3ERR_EXIST` |
| `IsADirectory` | `NFS3ERR_ISDIR` |
| `NotADirectory` | `NFS3ERR_NOTDIR` |
| `DirectoryNotEmpty` | `NFS3ERR_NOTEMPTY` |
| `PermissionDenied` | `NFS3ERR_ACCES` |
| `PathTraversalDenied` | `NFS3ERR_ACCES` |
| `NotSupported` | `NFS3ERR_NOTSUPP` |
| `NotAFile` | `NFS3ERR_INVAL` |
| `Io` / `Internal` | `NFS3ERR_IO` |

## 9. Docker Bind Mount

### 9.1 无需代码变更

`SandboxService` 创建 Docker 容器时使用 bind mount 将 workspace 目录挂载到容器内。由于 s3fs-fuse 挂载后的目录对宿主机来说就是普通目录，Docker bind mount 无需任何代码变更。

```rust
// SandboxService 中的 bind mount 逻辑保持不变
let host_path = format!("{}/{}", workspace_dir, workspace_id);
// Docker bind mount: host_path → /workspace
```

### 9.2 s3fs-fuse 与 Docker 的兼容性

s3fs-fuse 挂载时需要 `-o allow_other` 选项，确保 Docker daemon（通常以 root 运行）可以访问挂载点。同时需要确保 `/etc/fuse.conf` 中启用了 `user_allow_other`。

## 10. 风险与缓解

### 10.1 s3fs-fuse 性能

| 场景 | 风险 | 缓解措施 |
|------|------|---------|
| 小文件频繁读写 | s3fs 每次操作都有 HTTP 往返开销 | 启用 `use_cache` 本地缓存；s3fs 内置 stat 缓存 |
| 大文件随机写入 | s3fs 需要下载整文件、修改、重新上传 | 对于大文件场景，s3fs 的 `enable_content_md5` 和 multipart upload 可缓解 |
| 元数据操作（ls, stat） | 每次 ls 都需要 ListObjects API 调用 | s3fs 的 `stat_cache_expire` 参数控制缓存时间 |
| 文件/目录 rename | S3 不支持原子 rename，s3fs 内部实现为 copy + delete；大文件 rename 延迟达秒级 | workspace 场景下文件通常较小（代码文件），可接受；大文件场景需在部署文档中说明 |

### 10.2 s3fs-fuse 可用性

| 场景 | 风险 | 缓解措施 |
|------|------|---------|
| S3 服务不可用 | s3fs 挂载点变为不可访问 | 启动时健康检查；运行时周期性挂载点监控（见第 11 节） |
| s3fs 进程崩溃 | 挂载点变为 stale | 启动时 `cleanup_stale_mount()`；运行时定期检查 `/proc/mounts` |
| SIGKILL 导致未卸载 | 残留 stale mount | 启动时 `cleanup_stale_mount()` 自动清理 |
| FUSE 内核模块不可用 | 无法挂载 | 启动时检查 `/dev/fuse` 是否存在，快速失败并给出明确错误信息 |

### 10.3 数据一致性

统一走 s3fs-fuse 后，**单个 Server 实例内**的所有组件（WorkspaceService、NFS、Docker）通过同一个挂载点访问数据，消除了双路径不一致问题。s3fs-fuse 内部的缓存策略对所有本地访问者一致。

**跨 Server 实例的一致性**需要区分两种场景：

1. **不同实例操作不同 workspace**：S3 的 read-after-write 强一致性保证充分。s3fs 的 stat 缓存可能导致短暂的跨实例元数据不一致（默认缓存时间内），可通过调整 `stat_cache_expire` 参数控制。

2. **多个实例并发操作同一 workspace**：S3 的 read-after-write 一致性仅保证单个对象的最终一致，**不保证并发写入的正确性**。两个实例同时写入同一文件时会出现 last-writer-wins，数据可能丢失。此场景需要通过应用层的并发控制机制解决（见第 10.5 节）。

### 10.4 RustFS alpha 风险

| 风险 | 缓解措施 |
|------|---------|
| RustFS 存在未知 bug | 架构对接标准 S3 协议，可随时切换到 MinIO 或 AWS S3 |
| RustFS 性能不达预期 | s3fs-fuse 的本地缓存可缓解部分性能问题；必要时切换到 MinIO |
| RustFS 项目停止维护 | 不依赖 RustFS 特有功能，仅使用标准 S3 API |

### 10.5 多实例并发 workspace 访问控制

多个 Server 实例共享同一个 S3 bucket 时，需要防止同一个 workspace 被多个实例并发修改导致数据不一致。

#### 方案：基于数据库的 workspace 级别租约（Lease）

```
Server A                     Database                     Server B
   │                            │                            │
   ├── acquire_lease(ws_1) ───→ │                            │
   │ ←── lease granted ─────────│                            │
   │                            │ ←── acquire_lease(ws_1) ───┤
   │                            │ ──── lease denied ────────→│
   │                            │                            │
   │  (操作 workspace)          │                            │
   │                            │                            │
   ├── renew_lease(ws_1) ─────→│                            │
   │                            │                            │
   ├── release_lease(ws_1) ───→│                            │
   │                            │ ←── acquire_lease(ws_1) ───┤
   │                            │ ──── lease granted ────────→│
```

核心设计：

```rust
/// workspace_leases 表
/// 存储在现有的 SQLite 数据库中（S3 模式下需要替换为共享数据库，如 PostgreSQL）
CREATE TABLE workspace_leases (
    workspace_id TEXT PRIMARY KEY,
    holder_id    TEXT NOT NULL,       -- Server 实例 ID
    acquired_at  TIMESTAMP NOT NULL,
    expires_at   TIMESTAMP NOT NULL,  -- 租约过期时间
    renewed_at   TIMESTAMP NOT NULL
);

pub struct WorkspaceLease {
    /// 当前 Server 实例的唯一 ID（启动时生成 UUID）
    holder_id: String,
    /// 租约持续时间（默认 60 秒）
    lease_duration: Duration,
    /// 续约间隔（默认 20 秒，小于 lease_duration 的 1/3）
    renew_interval: Duration,
}
```

租约规则：
- Server 在操作 workspace 之前必须获取该 workspace 的租约
- 租约有有效期（默认 60 秒），持有者需定期续约
- 如果持有者崩溃未续约，租约自动过期，其他实例可获取
- 获取租约失败时返回错误，由上层（调度层或客户端）决定重试或路由到持有者

> **注意**：S3 模式下的多实例部署，需要将租约存储从 SQLite 迁移到共享数据库（如 PostgreSQL），这部分将在 HA 方案文档中详细设计。当前文档定义租约接口，具体存储实现留给 HA 方案。

## 11. 运行时监控与可观测性

### 11.1 s3fs-fuse 挂载状态监控

S3 模式下，需要定期检查 s3fs-fuse 挂载点的健康状态：

```rust
pub struct S3fsMountMonitor {
    mount_manager: Arc<S3fsMountManager>,
    /// 检查间隔（默认 30 秒）
    check_interval: Duration,
}

impl S3fsMountMonitor {
    /// 启动后台监控任务
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.check_interval);
            loop {
                interval.tick().await;

                // 检查 /proc/mounts 中是否存在挂载记录
                if !self.mount_manager.is_mounted().await {
                    tracing::error!("s3fs mount lost! attempting remount...");
                    // 尝试重新挂载
                    if let Err(e) = self.mount_manager.cleanup_stale_mount().await {
                        tracing::error!("stale mount cleanup failed: {}", e);
                    }
                    if let Err(e) = self.mount_manager.mount().await {
                        tracing::error!("remount failed: {}", e);
                        // TODO: 触发告警，标记实例为不健康
                    }
                    continue;
                }

                // 验证挂载点可访问（不只是存在于 /proc/mounts）
                if tokio::fs::read_dir(self.mount_manager.mount_point()).await.is_err() {
                    tracing::error!("s3fs mount point not accessible!");
                    // 清理 stale mount 后重新挂载
                    let _ = self.mount_manager.cleanup_stale_mount().await;
                    if let Err(e) = self.mount_manager.mount().await {
                        tracing::error!("remount after stale cleanup failed: {}", e);
                    }
                }
            }
        })
    }
}
```

### 11.2 Prometheus 指标

建议暴露以下指标：

| 指标名称 | 类型 | 说明 |
|----------|------|------|
| `workspace_storage_operation_duration_seconds` | Histogram | 各存储操作的延迟分布 |
| `workspace_storage_operation_errors_total` | Counter | 各存储操作的错误次数 |
| `workspace_s3fs_mount_status` | Gauge | s3fs 挂载状态（1=正常, 0=异常） |
| `workspace_s3fs_remount_total` | Counter | 重新挂载次数 |
| `workspace_lease_active` | Gauge | 当前持有的 workspace 租约数 |

### 11.3 日志

s3fs-fuse 支持通过参数控制日志级别：
- `-o dbglevel=info`：输出基本操作日志
- `-o curldbg`：输出 HTTP 请求详情（调试用）
- `-f`：前台运行，日志输出到 stderr

建议在 S3 模式启动时，将 s3fs 配置为将日志写入文件，便于故障排查：
```
-o logfile=/var/log/s3fs.log -o dbglevel=info
```

## 12. 数据迁移

### 12.1 Local → S3 迁移

从 Local 模式切换到 S3 模式时，已有的 workspace 数据需要迁移到 S3 bucket：

```
迁移步骤：
1. 停止 Server（或设为只读模式）
2. 创建 S3 bucket
3. 使用 aws cli / rclone / s3cmd 将本地 workspace_dir 内容同步到 S3 bucket
   $ rclone sync /var/lib/workspace s3:workspace-bucket --s3-endpoint=<endpoint>
4. 配置 S3 环境变量
5. 启动 Server（S3 模式）
6. 验证 workspace 数据完整性
```

### 12.2 S3 → Local 回退

如果 S3 模式出现严重问题，需要回退到 Local 模式：

```
回退步骤：
1. 停止 Server
2. 使用 s3fs-fuse 临时挂载 S3 bucket 到临时目录
3. 使用 rsync / cp -a 将数据从 S3 复制到本地 workspace_dir
4. 卸载 s3fs-fuse
5. 修改配置为 Local 模式
6. 启动 Server
```

### 12.3 数据完整性校验

迁移后建议进行校验：

- 对比源和目标的文件数量
- 对比关键文件的 checksum（可通过遍历 workspace 目录计算 MD5/SHA256）
- 通过 HTTP API 和 NFS 分别验证文件读写功能

## 13. 文件结构

```
server/src/infra/storage/
├── mod.rs          // StorageBackend trait, FileType, FileStat, StorageError 定义
├── local.rs        // LocalStorageBackend 实现
└── s3fs_mount.rs   // S3fsMountManager + S3fsMountMonitor（s3fs-fuse 挂载管理与监控）
```

## 14. 实施计划

### P1: StorageBackend trait 定义

- 新增 `server/src/infra/storage/mod.rs`
- 定义 `StorageBackend` trait（包含 `create_file`、带 `recursive` 参数的 `mkdir`、拆分的 `remove_file`/`remove_dir`）
- 定义 `FileStat`、`FileType`、`StorageError`（含 `IsADirectory`、`PathTraversalDenied`）
- 定义 `StorageResult` 类型别名
- 实现 `StorageError::from_io` 转换方法（使用 `libc` 常量）
- 实现 `From<FileStat> for FileInfo` 转换
- 新增 `libc` 依赖到 `Cargo.toml`

### P2: LocalStorageBackend 实现

- 新增 `server/src/infra/storage/local.rs`
- 实现所有 trait 方法（全部使用 `tokio::fs` async 操作）
- 实现 `resolve_path` 路径安全校验（检测即拒绝策略 + canonicalize 二次校验）
- 实现 `create_file` 的 exclusive/non-exclusive 模式
- 实现 `mkdir` 的 recursive/non-recursive 模式
- 实现 `remove_file`（目标为目录时返回 IsADirectory）和 `remove_dir`（目标为文件时返回 NotADirectory）
- 单元测试：覆盖所有方法的正常路径和错误路径（NotFound、PermissionDenied、路径穿越、IsADirectory 等）

### P3: WorkspaceService 迁移

- `WorkspaceService` 接收 `Arc<dyn StorageBackend>` 替代 `workspace_dir: PathBuf`
- 逐个替换文件操作方法（见 7.2 节对照表）
- 移除 `WorkspaceService` 内部的 `resolve_path`（下沉到 backend）
- 保留 `read_file_string` 作为组合方法
- **修复 `create()` 和 `delete()` 中的 `std::fs` 阻塞调用**
- 单元测试：使用 mock StorageBackend 测试业务逻辑

### P4: NFS 层迁移

- `WorkspaceNfs` 接收 `Arc<dyn StorageBackend>` 替代直接文件操作
- 替换所有 `std::fs` 调用为 `StorageBackend` async 方法（见 8.2 节对照表）
- inode 映射改为 `(workspace_id, relative_path)` 双向映射元组
- 将 `std::sync::RwLock` 替换为 `tokio::sync::RwLock`
- 实现 `FileStat → fattr3` 转换
- 实现 `StorageError → nfsstat3` 映射（见 8.4 节）
- NFS `create` 操作使用 `storage.create_file(exclusive)` 替代 `write_file(空内容)`
- **同时修复 async 上下文中的阻塞问题**
- 单元测试：验证 NFS 操作正确调用 backend 方法

### P5: 配置与 s3fs-fuse 集成

- 重构 `Config`，引入 `StorageConfig` 枚举（Local/S3）和 `S3Config` 结构体
- 环境变量使用 `WORKSPACE_` 前缀（`WORKSPACE_STORAGE_TYPE`、`WORKSPACE_S3_*`）
- 新增 `server/src/infra/storage/s3fs_mount.rs`
- 实现 `S3fsMountManager`（mount/unmount/is_mounted/cleanup_stale_mount/health_check/prepare_credentials）
- 凭证处理：配置了 access_key/secret_key 时写入临时密码文件
- stale mount 检测使用 `libc::ENOTCONN` / `libc::EACCES` 常量
- 实现启动时序（见 6.1 节）
- 实现 graceful shutdown hook（同时监听 SIGINT 和 SIGTERM）
- 实现 `S3fsMountMonitor` 运行时监控
- 集成测试：使用 MinIO + s3fs-fuse 验证端到端流程

### P6: 并发控制

- 定义 workspace 租约接口（`WorkspaceLease` trait）
- 实现基于 SQLite 的本地租约（单实例场景）
- 在 `WorkspaceService` 的写操作前集成租约检查
- 单元测试：验证租约获取/续约/过期/释放逻辑

> 注意：基于共享数据库的分布式租约实现将在 HA 方案文档中设计。

### P7: 集成测试与文档

- 端到端测试：Local 模式下完整 workspace 生命周期
- 端到端测试：S3 模式下完整 workspace 生命周期（需要 MinIO + s3fs-fuse 环境）
- NFS 集成测试：通过 NFS 客户端验证文件操作（含 NFS 错误码验证）
- Docker 集成测试：验证容器内文件访问
- 部署文档：s3fs-fuse 安装、配置、故障排查
- 迁移文档：Local ↔ S3 数据迁移步骤

### 依赖关系

```
P1 → P2 → P3 ──→ P5 → P7
         ↘ P4 ──↗
           P6 ──↗
```

P3 和 P4 可以并行（都依赖 P2），但建议先完成 P3（WorkspaceService 更简单，可以先验证 trait 设计是否合理），再做 P4（NFS 层更复杂）。P6（并发控制）可与 P5 并行开发。
