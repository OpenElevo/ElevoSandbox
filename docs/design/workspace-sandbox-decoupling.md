# Workspace 与 Sandbox 解耦设计方案

## 一、背景

### 现状问题

当前架构中 Workspace 和 Sandbox 是 1:1 强绑定：

- Workspace 不是独立实体，只是 Sandbox 内的挂载目录 `/workspace`
- 目录路径为 `/var/lib/workspace/{sandbox_id}/`，以 sandbox_id 命名
- 删除 Sandbox 时会同时删除 Workspace 目录，导致数据丢失
- 无法复用 Workspace，每次创建 Sandbox 都是全新的工作空间

### 目标

将 Workspace 和 Sandbox 解耦为独立实体：

- **Workspace**：长期持久的工作目录，独立管理生命周期
- **Sandbox**：临时的执行环境，创建时指定绑定的 Workspace
- **关系**：一个 Workspace 可被多个 Sandbox 同时使用（1:N）

## 二、数据模型

### 2.1 新增 Workspace 实体

```rust
pub struct Workspace {
    pub id: String,                           // UUID
    pub name: Option<String>,                 // 可选名称
    pub nfs_url: Option<String>,              // NFS 挂载地址
    pub metadata: HashMap<String, String>,    // 自定义元数据
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct CreateWorkspaceParams {
    pub name: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
}
```

### 2.2 修改 Sandbox 实体

```rust
pub struct Sandbox {
    pub id: String,
    pub workspace_id: String,                 // 【新增】关联的 Workspace ID（必填）
    pub name: Option<String>,
    pub template: String,
    pub state: SandboxState,
    pub container_id: Option<String>,
    pub env: HashMap<String, String>,
    pub metadata: HashMap<String, String>,
    // pub nfs_url: Option<String>,           // 【移除】改由 Workspace 管理
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub timeout: u64,
    pub error_message: Option<String>,
}

pub struct CreateSandboxParams {
    pub workspace_id: String,                 // 【新增】必填
    pub template: Option<String>,
    pub name: Option<String>,
    pub env: Option<HashMap<String, String>>,
    pub metadata: Option<HashMap<String, String>>,
    pub timeout: Option<u64>,
}
```

### 2.3 实体关系

```
┌─────────────────┐         ┌─────────────────┐
│   Workspace     │ 1     N │    Sandbox      │
│─────────────────│─────────│─────────────────│
│ id (PK)         │         │ id (PK)         │
│ name            │         │ workspace_id(FK)│◄── 外键引用
│ nfs_url         │         │ template        │
│ metadata        │         │ state           │
│ created_at      │         │ container_id    │
│ updated_at      │         │ env             │
│                 │         │ metadata        │
│                 │         │ created_at      │
│                 │         │ updated_at      │
│                 │         │ timeout         │
│                 │         │ error_message   │
└─────────────────┘         └─────────────────┘
```

## 三、API 设计

### 3.1 新增 Workspace HTTP API

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/workspaces` | 创建工作空间 |
| GET | `/api/v1/workspaces` | 列出所有工作空间 |
| GET | `/api/v1/workspaces/{id}` | 获取工作空间详情 |
| DELETE | `/api/v1/workspaces/{id}` | 删除工作空间 |

**文件操作 API（从 Sandbox 迁移）：**

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/workspaces/{id}/files` | 读取/列出文件 |
| PUT | `/api/v1/workspaces/{id}/files` | 写入文件 |
| DELETE | `/api/v1/workspaces/{id}/files` | 删除文件 |
| POST | `/api/v1/workspaces/{id}/files/mkdir` | 创建目录 |
| POST | `/api/v1/workspaces/{id}/files/move` | 移动文件 |
| POST | `/api/v1/workspaces/{id}/files/copy` | 复制文件 |
| GET | `/api/v1/workspaces/{id}/files/info` | 获取文件信息 |

### 3.2 修改 Sandbox HTTP API

**创建 Sandbox 请求变更：**

```json
{
    "workspace_id": "xxx",  // 【必填】指定工作空间
    "template": "ubuntu:22.04",
    "name": "my-sandbox",
    "env": {},
    "metadata": {},
    "timeout": 3600
}
```

**删除以下文件相关 API：**

- ~~GET /api/v1/sandboxes/{id}/files~~
- ~~PUT /api/v1/sandboxes/{id}/files~~
- ~~DELETE /api/v1/sandboxes/{id}/files~~
- ~~POST /api/v1/sandboxes/{id}/files/mkdir~~
- ~~POST /api/v1/sandboxes/{id}/files/move~~
- ~~POST /api/v1/sandboxes/{id}/files/copy~~
- ~~GET /api/v1/sandboxes/{id}/files/info~~

**保留的 Sandbox API：**

- CRUD: 创建、获取、列出、删除
- 进程执行: `POST /api/v1/sandboxes/{id}/process/run`
- 进程终止: `POST /api/v1/sandboxes/{id}/process/{pid}/kill`
- PTY 操作: 创建、输入、调整大小、关闭

### 3.3 gRPC Proto 变更

**新增 `workspace.proto`：**

```protobuf
syntax = "proto3";
package workspace.v1;

service WorkspaceService {
  rpc CreateWorkspace(CreateWorkspaceRequest) returns (CreateWorkspaceResponse);
  rpc GetWorkspace(GetWorkspaceRequest) returns (GetWorkspaceResponse);
  rpc ListWorkspaces(ListWorkspacesRequest) returns (ListWorkspacesResponse);
  rpc DeleteWorkspace(DeleteWorkspaceRequest) returns (DeleteWorkspaceResponse);
}

message Workspace {
  string id = 1;
  optional string name = 2;
  optional string nfs_url = 3;
  map<string, string> metadata = 4;
  google.protobuf.Timestamp created_at = 5;
  google.protobuf.Timestamp updated_at = 6;
}

message CreateWorkspaceRequest {
  optional string name = 1;
  map<string, string> metadata = 2;
}

message CreateWorkspaceResponse {
  Workspace workspace = 1;
}

message GetWorkspaceRequest {
  string id = 1;
}

message GetWorkspaceResponse {
  Workspace workspace = 1;
}

message ListWorkspacesRequest {}

message ListWorkspacesResponse {
  repeated Workspace workspaces = 1;
}

message DeleteWorkspaceRequest {
  string id = 1;
}

message DeleteWorkspaceResponse {}
```

**修改 `sandbox.proto`：**

```protobuf
message Sandbox {
  string id = 1;
  string workspace_id = 2;              // 【新增】
  optional string name = 3;             // 原 2 → 3
  string template = 4;                  // 原 3 → 4
  SandboxState state = 5;               // 原 4 → 5
  map<string, string> env = 6;          // 原 5 → 6
  map<string, string> metadata = 7;     // 原 6 → 7
  // optional string nfs_url = 7;       // 【移除】
  google.protobuf.Timestamp created_at = 8;
  google.protobuf.Timestamp updated_at = 9;
  uint64 timeout = 10;
  optional string error_message = 11;
}

message CreateSandboxRequest {
  string workspace_id = 1;              // 【新增】必填
  optional string template = 2;
  optional string name = 3;
  map<string, string> env = 4;
  map<string, string> metadata = 5;
  optional uint64 timeout = 6;
}
```

## 四、生命周期管理

### 4.1 Workspace 生命周期

**创建 Workspace：**

```
1. 生成 UUID 作为 workspace_id
2. 创建目录 /var/lib/workspace/{workspace_id}/
3. 启用 NFS 导出，生成 nfs_url
4. 写入数据库
5. 返回 Workspace 对象
```

**删除 Workspace：**

```
1. 检查是否有 Sandbox 正在使用此 Workspace
   - 如有，拒绝删除，返回错误
2. 取消 NFS 导出
3. 删除目录 /var/lib/workspace/{workspace_id}/
4. 从数据库删除记录
```

### 4.2 Sandbox 生命周期

**创建 Sandbox：**

```
1. 校验 workspace_id 对应的 Workspace 存在
   - 不存在则返回错误
2. 创建数据库记录（包含 workspace_id）
3. 获取 Workspace 目录路径
4. 创建 Docker 容器，挂载 Workspace 目录到 /workspace
5. 启动容器
6. 等待 Agent 连接
7. 更新状态为 Running
```

**删除 Sandbox：**

```
1. 检查状态，如果 Running 且非 force 则拒绝
2. 更新状态为 Stopping
3. 停止 Docker 容器
4. 移除 Docker 容器
5. 注销 Agent 连接
6. 从数据库删除记录
7. 【不删除 Workspace 目录】
```

## 五、文件操作实现

### 5.1 实现方式

Workspace 文件操作由 Server 直接操作文件系统，不依赖 Sandbox/Agent。

原因：
- Workspace 目录 `/var/lib/workspace/{workspace_id}/` 对 Server 进程可直接访问
- 不需要有运行中的 Sandbox 就能操作文件
- 实现更简单，性能更好

### 5.2 文件服务设计

```rust
pub struct FileService {
    config: Arc<Config>,
    workspace_repository: Arc<WorkspaceRepository>,
}

impl FileService {
    // 获取 Workspace 目录路径
    fn get_workspace_path(&self, workspace_id: &str) -> Result<PathBuf>;

    // 读取文件内容
    pub async fn read_file(&self, workspace_id: &str, path: &str) -> Result<Vec<u8>>;

    // 列出目录内容
    pub async fn list_dir(&self, workspace_id: &str, path: &str) -> Result<Vec<FileInfo>>;

    // 写入文件
    pub async fn write_file(&self, workspace_id: &str, path: &str, content: &[u8]) -> Result<()>;

    // 删除文件/目录
    pub async fn delete(&self, workspace_id: &str, path: &str) -> Result<()>;

    // 创建目录
    pub async fn mkdir(&self, workspace_id: &str, path: &str) -> Result<()>;

    // 移动文件/目录
    pub async fn move_file(&self, workspace_id: &str, src: &str, dst: &str) -> Result<()>;

    // 复制文件/目录
    pub async fn copy_file(&self, workspace_id: &str, src: &str, dst: &str) -> Result<()>;

    // 获取文件信息
    pub async fn file_info(&self, workspace_id: &str, path: &str) -> Result<FileInfo>;
}
```

### 5.3 安全考虑

- 路径校验：防止路径穿越攻击（如 `../`）
- 权限控制：确保只能操作 Workspace 目录内的文件
- 符号链接：谨慎处理，防止跳出 Workspace 目录

## 六、目录结构

```
/var/lib/workspace/
├── {workspace_id_1}/           ← Workspace 1
│   ├── src/
│   ├── config.json
│   └── data/
├── {workspace_id_2}/           ← Workspace 2
│   └── project/
└── {workspace_id_3}/           ← Workspace 3
    └── ...
```

## 七、数据库 Schema

### 7.1 新增 workspaces 表

```sql
CREATE TABLE workspaces (
    id TEXT PRIMARY KEY,
    name TEXT,
    nfs_url TEXT,
    metadata TEXT,                -- JSON 格式
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

### 7.2 修改 sandboxes 表

```sql
-- 新增 workspace_id 列
ALTER TABLE sandboxes ADD COLUMN workspace_id TEXT NOT NULL REFERENCES workspaces(id);

-- 移除 nfs_url 列（如果有）
-- SQLite 不支持 DROP COLUMN，需要重建表

-- 新的表结构
CREATE TABLE sandboxes (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    name TEXT,
    template TEXT NOT NULL,
    state TEXT NOT NULL,
    container_id TEXT,
    env TEXT,                     -- JSON 格式
    metadata TEXT,                -- JSON 格式
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    timeout INTEGER NOT NULL,
    error_message TEXT
);

-- 创建索引
CREATE INDEX idx_sandboxes_workspace_id ON sandboxes(workspace_id);
```

## 八、SDK 变更

### 8.1 TypeScript SDK

```typescript
// 新增 WorkspaceClient
class WorkspaceClient {
  create(params?: CreateWorkspaceParams): Promise<Workspace>;
  get(id: string): Promise<Workspace>;
  list(): Promise<Workspace[]>;
  delete(id: string): Promise<void>;

  // 文件操作
  readFile(id: string, path: string): Promise<Buffer>;
  writeFile(id: string, path: string, content: Buffer): Promise<void>;
  listDir(id: string, path: string): Promise<FileInfo[]>;
  deleteFile(id: string, path: string): Promise<void>;
  mkdir(id: string, path: string): Promise<void>;
  moveFile(id: string, src: string, dst: string): Promise<void>;
  copyFile(id: string, src: string, dst: string): Promise<void>;
  fileInfo(id: string, path: string): Promise<FileInfo>;
}

// 修改 SandboxClient
interface CreateSandboxParams {
  workspaceId: string;  // 必填
  template?: string;
  name?: string;
  env?: Record<string, string>;
  metadata?: Record<string, string>;
  timeout?: number;
}

// 移除 SandboxClient 的文件操作方法
```

### 8.2 Python SDK

```python
# 新增 WorkspaceClient
class WorkspaceClient:
    def create(self, params: CreateWorkspaceParams = None) -> Workspace: ...
    def get(self, id: str) -> Workspace: ...
    def list(self) -> List[Workspace]: ...
    def delete(self, id: str) -> None: ...

    # 文件操作
    def read_file(self, id: str, path: str) -> bytes: ...
    def write_file(self, id: str, path: str, content: bytes) -> None: ...
    def list_dir(self, id: str, path: str) -> List[FileInfo]: ...
    def delete_file(self, id: str, path: str) -> None: ...
    def mkdir(self, id: str, path: str) -> None: ...
    def move_file(self, id: str, src: str, dst: str) -> None: ...
    def copy_file(self, id: str, src: str, dst: str) -> None: ...
    def file_info(self, id: str, path: str) -> FileInfo: ...

# 修改 CreateSandboxParams
@dataclass
class CreateSandboxParams:
    workspace_id: str  # 必填
    template: Optional[str] = None
    name: Optional[str] = None
    env: Optional[Dict[str, str]] = None
    metadata: Optional[Dict[str, str]] = None
    timeout: Optional[int] = None
```

### 8.3 Go SDK

```go
// 新增 WorkspaceClient
type WorkspaceClient interface {
    Create(ctx context.Context, params *CreateWorkspaceParams) (*Workspace, error)
    Get(ctx context.Context, id string) (*Workspace, error)
    List(ctx context.Context) ([]*Workspace, error)
    Delete(ctx context.Context, id string) error

    // 文件操作
    ReadFile(ctx context.Context, id, path string) ([]byte, error)
    WriteFile(ctx context.Context, id, path string, content []byte) error
    ListDir(ctx context.Context, id, path string) ([]*FileInfo, error)
    DeleteFile(ctx context.Context, id, path string) error
    Mkdir(ctx context.Context, id, path string) error
    MoveFile(ctx context.Context, id, src, dst string) error
    CopyFile(ctx context.Context, id, src, dst string) error
    FileInfo(ctx context.Context, id, path string) (*FileInfo, error)
}

// 修改 CreateSandboxParams
type CreateSandboxParams struct {
    WorkspaceID string            // 必填
    Template    *string
    Name        *string
    Env         map[string]string
    Metadata    map[string]string
    Timeout     *uint64
}
```

## 九、实现步骤

### Phase 1: 基础设施

1. 新增 `workspace.proto` 定义
2. 修改 `sandbox.proto` 定义
3. 运行 proto 生成代码
4. 新增数据库 migration

### Phase 2: Server 核心

1. 新增 `domain/workspace.rs` 定义 Workspace 实体
2. 新增 `repository/workspace.rs` 实现数据库操作
3. 新增 `service/workspace.rs` 实现业务逻辑
4. 新增 `service/file.rs` 实现文件操作（直接文件系统）
5. 修改 `service/sandbox.rs` 添加 workspace_id 关联逻辑
6. 修改 NFS 管理，将导出绑定到 Workspace

### Phase 3: API 层

1. 新增 HTTP Workspace API 路由和处理器
2. 新增 gRPC Workspace Service 实现
3. 新增 HTTP 文件操作 API（Workspace 下）
4. 移除 Sandbox 下的文件操作 API
5. 修改 Sandbox 创建 API，添加 workspace_id 参数

### Phase 4: SDK 更新

1. 更新 TypeScript SDK
2. 更新 Python SDK
3. 更新 Go SDK

### Phase 5: 测试与文档

1. 更新单元测试
2. 更新集成测试
3. 更新 API 文档

## 十、约束与限制

| 约束 | 说明 |
|------|------|
| Workspace : Sandbox = 1 : N | 一个 Workspace 可被多个 Sandbox 同时使用 |
| 创建 Sandbox 必须指定 workspace_id | Workspace 不存在则返回错误 |
| 删除 Workspace 需检查引用 | 有 Sandbox 使用时拒绝删除 |
| 文件操作归属 Workspace | Server 直接操作文件系统，不依赖 Sandbox |
| NFS 归属 Workspace | NFS 导出绑定到 Workspace，不再绑定 Sandbox |
