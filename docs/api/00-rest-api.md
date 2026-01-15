# REST API 规范文档

本文档定义 Elevo Workspace 的 REST API 规范，包括认证、请求格式、响应格式、错误处理等。

---

## 目录

- [1. 概述](#1-概述)
- [2. 认证](#2-认证)
- [3. 请求格式](#3-请求格式)
- [4. 响应格式](#4-响应格式)
- [5. 分页](#5-分页)
- [6. 错误处理](#6-错误处理)
- [7. WebSocket 协议](#7-websocket-协议)
- [8. 速率限制](#8-速率限制)
- [9. API 端点汇总](#9-api-端点汇总)
- [10. OpenAPI 规范](#10-openapi-规范)

---

## 1. 概述

### 1.1 基础信息

| 项目 | 值 |
|-----|---|
| API 版本 | v1 |
| 基础路径 | `/api/v1` |
| 协议 | HTTPS |
| 数据格式 | JSON |
| 字符编码 | UTF-8 |

### 1.2 设计原则

- **RESTful**: 遵循 REST 设计原则
- **一致性**: 所有 API 使用统一的请求/响应格式
- **可预测**: URL 和参数命名规范统一
- **版本化**: API 版本通过 URL 路径指定
- **幂等性**: GET/PUT/DELETE 操作幂等

### 1.3 URL 结构

```
https://{host}/api/v1/{resource}[/{id}][/{sub-resource}][/{sub-id}]
```

示例：
- `GET /api/v1/sandboxes` - 列出所有 Sandbox
- `GET /api/v1/sandboxes/sbx-abc123` - 获取指定 Sandbox
- `POST /api/v1/sandboxes/sbx-abc123/files` - 在 Sandbox 中创建文件
- `GET /api/v1/sandboxes/sbx-abc123/process/1234` - 获取进程信息

---

## 2. 认证

### 2.1 认证方式

支持以下认证方式：

#### Bearer Token

```http
Authorization: Bearer <token>
```

#### API Key (Header)

```http
X-API-Key: <api-key>
```

#### API Key (Query Parameter)

```
GET /api/v1/sandboxes?api_key=<api-key>
```

### 2.2 Token 格式

```typescript
interface AuthToken {
  /**
   * Token 值
   */
  token: string

  /**
   * Token 类型
   */
  type: 'bearer' | 'api_key'

  /**
   * 过期时间
   */
  expiresAt?: Date

  /**
   * 权限范围
   */
  scopes?: string[]
}
```

### 2.3 权限范围 (Scopes)

| Scope | 描述 |
|-------|------|
| `sandbox:read` | 读取 Sandbox 信息 |
| `sandbox:write` | 创建/修改/删除 Sandbox |
| `sandbox:exec` | 执行命令 |
| `files:read` | 读取文件 |
| `files:write` | 写入文件 |
| `git:read` | 读取 Git 信息 |
| `git:write` | Git 写操作 |
| `snapshot:read` | 读取快照 |
| `snapshot:write` | 创建/删除快照 |
| `volume:read` | 读取卷信息 |
| `volume:write` | 创建/删除卷 |

### 2.4 认证错误

```json
{
  "error": {
    "code": 1001,
    "name": "UNAUTHORIZED",
    "message": "Invalid or expired token",
    "details": {
      "reason": "token_expired"
    }
  }
}
```

---

## 3. 请求格式

### 3.1 HTTP 方法

| 方法 | 用途 | 幂等 |
|------|-----|------|
| GET | 获取资源 | 是 |
| POST | 创建资源/执行操作 | 否 |
| PUT | 替换资源 | 是 |
| PATCH | 部分更新资源 | 否 |
| DELETE | 删除资源 | 是 |

### 3.2 请求头

| 头部 | 必需 | 描述 |
|-----|------|------|
| `Authorization` | 是 | 认证信息 |
| `Content-Type` | 条件 | 请求体类型 (POST/PUT/PATCH 必需) |
| `Accept` | 否 | 期望的响应类型 |
| `X-Request-ID` | 否 | 请求追踪 ID |
| `X-Idempotency-Key` | 否 | 幂等键 (用于 POST 请求) |

### 3.3 Content-Type

| 类型 | 用途 |
|-----|------|
| `application/json` | JSON 数据 (默认) |
| `application/octet-stream` | 二进制数据 |
| `multipart/form-data` | 文件上传 |
| `text/plain` | 纯文本 |

### 3.4 查询参数规范

```typescript
// 通用查询参数
interface CommonQueryParams {
  // 分页
  page?: number      // 页码，从 1 开始
  limit?: number     // 每页数量，默认 20，最大 100

  // 排序
  sort_by?: string   // 排序字段
  sort_order?: 'asc' | 'desc'  // 排序方向

  // 筛选
  labels?: string    // 标签筛选，格式: key1=value1,key2=value2
  state?: string     // 状态筛选，多个用逗号分隔
  search?: string    // 搜索关键词

  // 字段选择
  fields?: string    // 返回字段，逗号分隔
  expand?: string    // 展开关联资源
}
```

### 3.5 请求示例

```http
POST /api/v1/sandboxes HTTP/1.1
Host: api.example.com
Authorization: Bearer eyJhbGciOiJIUzI1NiIs...
Content-Type: application/json
X-Request-ID: req-abc123
X-Idempotency-Key: idem-xyz789

{
  "name": "my-sandbox",
  "template": "python:3.11",
  "resources": {
    "cpu": 2,
    "memoryMB": 4096
  },
  "labels": {
    "project": "demo"
  }
}
```

---

## 4. 响应格式

### 4.1 成功响应

#### 单个资源

```json
{
  "id": "sbx-abc123",
  "name": "my-sandbox",
  "state": "running",
  "createdAt": "2024-01-15T10:30:00Z"
}
```

#### 资源列表

```json
{
  "data": [
    { "id": "sbx-abc123", "name": "sandbox-1" },
    { "id": "sbx-def456", "name": "sandbox-2" }
  ],
  "pagination": {
    "page": 1,
    "limit": 20,
    "total": 42,
    "totalPages": 3,
    "hasMore": true
  }
}
```

#### 操作结果

```json
{
  "success": true,
  "message": "Sandbox deleted successfully"
}
```

### 4.2 响应头

| 头部 | 描述 |
|-----|------|
| `Content-Type` | 响应内容类型 |
| `X-Request-ID` | 请求追踪 ID (与请求相同或自动生成) |
| `X-RateLimit-Limit` | 速率限制上限 |
| `X-RateLimit-Remaining` | 剩余请求数 |
| `X-RateLimit-Reset` | 重置时间 (Unix 时间戳) |

### 4.3 HTTP 状态码

| 状态码 | 描述 | 使用场景 |
|-------|------|---------|
| 200 | OK | 请求成功 |
| 201 | Created | 资源创建成功 |
| 202 | Accepted | 请求已接受，异步处理 |
| 204 | No Content | 成功但无返回内容 |
| 400 | Bad Request | 请求参数错误 |
| 401 | Unauthorized | 未认证 |
| 403 | Forbidden | 无权限 |
| 404 | Not Found | 资源不存在 |
| 409 | Conflict | 资源冲突 |
| 422 | Unprocessable Entity | 验证失败 |
| 429 | Too Many Requests | 超过速率限制 |
| 500 | Internal Server Error | 服务器内部错误 |
| 502 | Bad Gateway | 网关错误 |
| 503 | Service Unavailable | 服务不可用 |
| 507 | Insufficient Storage | 存储空间不足 |

### 4.4 日期时间格式

所有日期时间使用 ISO 8601 格式：

```
2024-01-15T10:30:00Z
2024-01-15T10:30:00.123Z
2024-01-15T10:30:00+08:00
```

---

## 5. 分页

### 5.1 请求参数

| 参数 | 类型 | 默认值 | 描述 |
|-----|------|-------|------|
| `page` | integer | 1 | 页码，从 1 开始 |
| `limit` | integer | 20 | 每页数量 (1-100) |

### 5.2 响应格式

```typescript
interface PaginatedResponse<T> {
  /**
   * 数据列表
   */
  data: T[]

  /**
   * 分页信息
   */
  pagination: {
    /**
     * 当前页码
     */
    page: number

    /**
     * 每页数量
     */
    limit: number

    /**
     * 总记录数
     */
    total: number

    /**
     * 总页数
     */
    totalPages: number

    /**
     * 是否有更多
     */
    hasMore: boolean
  }
}
```

### 5.3 示例

**请求**:
```http
GET /api/v1/sandboxes?page=2&limit=10
```

**响应**:
```json
{
  "data": [...],
  "pagination": {
    "page": 2,
    "limit": 10,
    "total": 42,
    "totalPages": 5,
    "hasMore": true
  }
}
```

### 5.4 游标分页 (可选)

对于大数据集，支持游标分页：

**请求**:
```http
GET /api/v1/sandboxes?cursor=eyJpZCI6InNieC1hYmMxMjMifQ&limit=20
```

**响应**:
```json
{
  "data": [...],
  "pagination": {
    "nextCursor": "eyJpZCI6InNieC14eXo3ODkifQ",
    "prevCursor": "eyJpZCI6InNieC1kZWY0NTYifQ",
    "hasMore": true
  }
}
```

---

## 6. 错误处理

### 6.1 错误响应格式

```typescript
interface ErrorResponse {
  error: {
    /**
     * 错误码
     */
    code: number

    /**
     * 错误名称
     */
    name: string

    /**
     * 错误消息
     */
    message: string

    /**
     * 详细信息
     */
    details?: Record<string, any>

    /**
     * 请求 ID
     */
    requestId?: string

    /**
     * 文档链接
     */
    docUrl?: string
  }
}
```

### 6.2 错误码分类

| 范围 | 类别 |
|-----|------|
| 1000-1999 | 认证和授权错误 |
| 2000-2999 | Sandbox 错误 |
| 3000-3999 | FileSystem 错误 |
| 4000-4999 | Process/PTY 错误 |
| 5000-5999 | Git 错误 |
| 6000-6999 | LSP 错误 |
| 7000-7999 | Snapshot/Volume 错误 |
| 9000-9999 | 系统错误 |

### 6.3 完整错误码列表

```typescript
enum ErrorCode {
  // ========== 认证错误 (1000-1999) ==========
  UNAUTHORIZED = 1001,
  FORBIDDEN = 1002,
  TOKEN_EXPIRED = 1003,
  INVALID_TOKEN = 1004,
  INSUFFICIENT_SCOPE = 1005,

  // ========== Sandbox 错误 (2000-2999) ==========
  SANDBOX_NOT_FOUND = 2001,
  SANDBOX_ALREADY_EXISTS = 2002,
  SANDBOX_NOT_RUNNING = 2003,
  SANDBOX_LIMIT_EXCEEDED = 2004,
  SANDBOX_CREATION_FAILED = 2005,
  SANDBOX_TIMEOUT = 2006,
  TEMPLATE_NOT_FOUND = 2007,
  INVALID_SANDBOX_STATE = 2008,

  // ========== FileSystem 错误 (3000-3999) ==========
  FILE_NOT_FOUND = 3001,
  FILE_ALREADY_EXISTS = 3002,
  DIRECTORY_NOT_EMPTY = 3003,
  PERMISSION_DENIED = 3004,
  DISK_QUOTA_EXCEEDED = 3005,
  INVALID_PATH = 3006,
  FILE_TOO_LARGE = 3007,
  NOT_A_DIRECTORY = 3008,
  NOT_A_FILE = 3009,

  // ========== Process 错误 (4000-4099) ==========
  PROCESS_NOT_FOUND = 4001,
  PROCESS_TIMEOUT = 4002,
  PROCESS_KILLED = 4003,
  COMMAND_FAILED = 4004,
  STDIN_CLOSED = 4005,
  SESSION_NOT_FOUND = 4006,
  SESSION_EXPIRED = 4007,

  // ========== PTY 错误 (4100-4199) ==========
  PTY_NOT_FOUND = 4100,
  PTY_ALREADY_EXISTS = 4101,
  PTY_LIMIT_EXCEEDED = 4102,
  PTY_CONNECTION_FAILED = 4103,
  PTY_WRITE_FAILED = 4104,
  PTY_RESIZE_FAILED = 4105,

  // ========== Git 错误 (5000-5999) ==========
  GIT_NOT_INITIALIZED = 5001,
  GIT_CLONE_FAILED = 5002,
  GIT_COMMIT_FAILED = 5003,
  GIT_PUSH_FAILED = 5004,
  GIT_PULL_FAILED = 5005,
  GIT_MERGE_CONFLICT = 5006,
  GIT_AUTH_FAILED = 5007,
  GIT_BRANCH_NOT_FOUND = 5008,
  GIT_REMOTE_NOT_FOUND = 5009,
  GIT_DIRTY_WORKTREE = 5010,

  // ========== LSP 错误 (6000-6999) ==========
  LSP_SERVER_NOT_FOUND = 6001,
  LSP_SERVER_CRASHED = 6002,
  LSP_INITIALIZATION_FAILED = 6003,
  LSP_REQUEST_FAILED = 6004,
  LSP_LANGUAGE_NOT_SUPPORTED = 6005,
  LSP_TIMEOUT = 6006,

  // ========== Snapshot 错误 (7000-7099) ==========
  SNAPSHOT_NOT_FOUND = 7000,
  SNAPSHOT_CREATE_FAILED = 7001,
  SNAPSHOT_IN_USE = 7002,
  SNAPSHOT_LIMIT_EXCEEDED = 7003,

  // ========== Volume 错误 (7100-7199) ==========
  VOLUME_NOT_FOUND = 7100,
  VOLUME_IN_USE = 7101,
  VOLUME_CREATE_FAILED = 7102,
  VOLUME_EXISTS = 7103,
  VOLUME_LIMIT_EXCEEDED = 7104,
  INVALID_SIZE = 7105,
  STORAGE_INSUFFICIENT = 7106,

  // ========== 系统错误 (9000-9999) ==========
  INTERNAL_ERROR = 9001,
  SERVICE_UNAVAILABLE = 9002,
  RATE_LIMITED = 9003,
  VALIDATION_ERROR = 9004,
  INVALID_REQUEST = 9005,
  RESOURCE_EXHAUSTED = 9006,
  NOT_IMPLEMENTED = 9007
}
```

### 6.4 错误示例

```json
{
  "error": {
    "code": 2001,
    "name": "SANDBOX_NOT_FOUND",
    "message": "Sandbox with ID 'sbx-abc123' not found",
    "details": {
      "sandboxId": "sbx-abc123"
    },
    "requestId": "req-xyz789",
    "docUrl": "https://docs.example.com/errors/2001"
  }
}
```

### 6.5 验证错误

验证错误返回详细的字段错误信息：

```json
{
  "error": {
    "code": 9004,
    "name": "VALIDATION_ERROR",
    "message": "Request validation failed",
    "details": {
      "fields": [
        {
          "field": "name",
          "message": "Name must be between 3 and 63 characters",
          "value": "ab"
        },
        {
          "field": "resources.cpu",
          "message": "CPU must be at least 1",
          "value": 0
        }
      ]
    }
  }
}
```

---

## 7. WebSocket 协议

### 7.1 连接

```
wss://{host}/api/v1/sandboxes/{id}/{service}/connect
```

**认证**:
```
wss://api.example.com/api/v1/sandboxes/sbx-abc123/pty/1234/connect?token=<token>
```

或通过 header (如果客户端支持):
```
Sec-WebSocket-Protocol: bearer, <token>
```

### 7.2 消息格式

所有 WebSocket 消息使用 JSON 格式：

```typescript
interface WebSocketMessage {
  /**
   * 消息类型
   */
  type: string

  /**
   * 消息数据
   */
  data?: any

  /**
   * 消息 ID (用于请求-响应模式)
   */
  id?: string

  /**
   * 时间戳
   */
  timestamp?: string
}
```

### 7.3 消息类型

#### 客户端 -> 服务器

| 类型 | 描述 |
|-----|------|
| `input` | 输入数据 |
| `resize` | 调整大小 |
| `ping` | 心跳 |
| `subscribe` | 订阅事件 |
| `unsubscribe` | 取消订阅 |

#### 服务器 -> 客户端

| 类型 | 描述 |
|-----|------|
| `data` | 输出数据 |
| `exit` | 进程退出 |
| `pong` | 心跳响应 |
| `event` | 事件通知 |
| `error` | 错误消息 |

### 7.4 心跳

客户端应定期发送心跳保持连接：

**客户端**:
```json
{ "type": "ping", "timestamp": "2024-01-15T10:30:00Z" }
```

**服务器**:
```json
{ "type": "pong", "timestamp": "2024-01-15T10:30:00Z" }
```

建议心跳间隔: 30 秒

### 7.5 PTY WebSocket 示例

**连接**:
```
wss://api.example.com/api/v1/sandboxes/sbx-abc123/pty/1234/connect
```

**输入**:
```json
{
  "type": "input",
  "data": "bHMgLWxhDQ=="
}
```

**输出**:
```json
{
  "type": "data",
  "data": "dXNlckBzYW5kYm94Oi9hcHAkIA==",
  "timestamp": "2024-01-15T10:30:00Z"
}
```

**调整大小**:
```json
{
  "type": "resize",
  "data": { "cols": 120, "rows": 40 }
}
```

**退出**:
```json
{
  "type": "exit",
  "data": { "exitCode": 0 }
}
```

### 7.6 文件监听 WebSocket

**连接**:
```
wss://api.example.com/api/v1/sandboxes/sbx-abc123/files/watch
```

**订阅**:
```json
{
  "type": "subscribe",
  "data": {
    "path": "/app/src",
    "recursive": true
  }
}
```

**事件**:
```json
{
  "type": "event",
  "data": {
    "event": "modified",
    "path": "/app/src/main.py",
    "timestamp": "2024-01-15T10:30:00Z"
  }
}
```

---

## 8. 速率限制

### 8.1 限制规则

| 端点类型 | 限制 | 时间窗口 |
|---------|------|---------|
| 读取操作 | 1000 | 1 分钟 |
| 写入操作 | 100 | 1 分钟 |
| Sandbox 创建 | 10 | 1 分钟 |
| 命令执行 | 60 | 1 分钟 |
| 文件上传 | 30 | 1 分钟 |

### 8.2 响应头

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1705312200
```

### 8.3 超限响应

```http
HTTP/1.1 429 Too Many Requests
Content-Type: application/json
Retry-After: 60
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 1705312200

{
  "error": {
    "code": 9003,
    "name": "RATE_LIMITED",
    "message": "Rate limit exceeded. Please retry after 60 seconds",
    "details": {
      "limit": 1000,
      "remaining": 0,
      "resetAt": "2024-01-15T10:30:00Z",
      "retryAfter": 60
    }
  }
}
```

---

## 9. API 端点汇总

### 9.1 Sandbox 服务

| 方法 | 端点 | 描述 |
|-----|------|------|
| POST | `/sandboxes` | 创建 Sandbox |
| GET | `/sandboxes` | 列出 Sandbox |
| GET | `/sandboxes/{id}` | 获取 Sandbox |
| DELETE | `/sandboxes/{id}` | 删除 Sandbox |
| POST | `/sandboxes/{id}/start` | 启动 Sandbox |
| POST | `/sandboxes/{id}/stop` | 停止 Sandbox |
| POST | `/sandboxes/{id}/pause` | 暂停 Sandbox |
| POST | `/sandboxes/{id}/resume` | 恢复 Sandbox |
| GET | `/sandboxes/{id}/metrics` | 获取指标 |

### 9.2 FileSystem 服务

| 方法 | 端点 | 描述 |
|-----|------|------|
| GET | `/sandboxes/{id}/files` | 列出文件 |
| GET | `/sandboxes/{id}/files/read` | 读取文件 |
| POST | `/sandboxes/{id}/files/write` | 写入文件 |
| POST | `/sandboxes/{id}/files/mkdir` | 创建目录 |
| POST | `/sandboxes/{id}/files/copy` | 复制文件 |
| POST | `/sandboxes/{id}/files/move` | 移动文件 |
| DELETE | `/sandboxes/{id}/files` | 删除文件 |
| POST | `/sandboxes/{id}/files/find` | 查找文件 |
| POST | `/sandboxes/{id}/files/grep` | 搜索内容 |
| GET | `/sandboxes/{id}/files/download` | 下载文件 |
| POST | `/sandboxes/{id}/files/upload` | 上传文件 |
| WS | `/sandboxes/{id}/files/watch` | 监听变化 |

### 9.3 Process 服务

| 方法 | 端点 | 描述 |
|-----|------|------|
| POST | `/sandboxes/{id}/process/run` | 执行命令 |
| POST | `/sandboxes/{id}/process/spawn` | 启动进程 |
| GET | `/sandboxes/{id}/process` | 列出进程 |
| GET | `/sandboxes/{id}/process/{pid}` | 获取进程 |
| DELETE | `/sandboxes/{id}/process/{pid}` | 终止进程 |
| POST | `/sandboxes/{id}/process/{pid}/write` | 写入 stdin |
| WS | `/sandboxes/{id}/process/{pid}/stream` | 流式输出 |
| POST | `/sandboxes/{id}/code/run` | 执行代码 |
| POST | `/sandboxes/{id}/sessions` | 创建会话 |
| DELETE | `/sandboxes/{id}/sessions/{sid}` | 关闭会话 |

### 9.4 PTY 服务

| 方法 | 端点 | 描述 |
|-----|------|------|
| POST | `/sandboxes/{id}/pty` | 创建 PTY |
| GET | `/sandboxes/{id}/pty` | 列出 PTY |
| GET | `/sandboxes/{id}/pty/{pid}` | 获取 PTY |
| DELETE | `/sandboxes/{id}/pty/{pid}` | 关闭 PTY |
| POST | `/sandboxes/{id}/pty/{pid}/resize` | 调整大小 |
| POST | `/sandboxes/{id}/pty/{pid}/write` | 写入数据 |
| WS | `/sandboxes/{id}/pty/{pid}/connect` | 连接 PTY |

### 9.5 Git 服务

| 方法 | 端点 | 描述 |
|-----|------|------|
| POST | `/sandboxes/{id}/git/clone` | 克隆仓库 |
| POST | `/sandboxes/{id}/git/init` | 初始化仓库 |
| GET | `/sandboxes/{id}/git/status` | 获取状态 |
| GET | `/sandboxes/{id}/git/log` | 获取日志 |
| GET | `/sandboxes/{id}/git/diff` | 获取差异 |
| GET | `/sandboxes/{id}/git/branches` | 列出分支 |
| POST | `/sandboxes/{id}/git/branches` | 创建分支 |
| DELETE | `/sandboxes/{id}/git/branches/{name}` | 删除分支 |
| POST | `/sandboxes/{id}/git/checkout` | 切换分支 |
| POST | `/sandboxes/{id}/git/add` | 暂存文件 |
| POST | `/sandboxes/{id}/git/commit` | 提交更改 |
| POST | `/sandboxes/{id}/git/push` | 推送更改 |
| POST | `/sandboxes/{id}/git/pull` | 拉取更改 |
| POST | `/sandboxes/{id}/git/merge` | 合并分支 |
| POST | `/sandboxes/{id}/git/reset` | 重置更改 |
| POST | `/sandboxes/{id}/git/stash` | 储藏更改 |

### 9.6 LSP 服务

| 方法 | 端点 | 描述 |
|-----|------|------|
| POST | `/sandboxes/{id}/lsp/servers` | 启动 LSP 服务器 |
| GET | `/sandboxes/{id}/lsp/servers` | 列出 LSP 服务器 |
| DELETE | `/sandboxes/{id}/lsp/servers/{sid}` | 停止 LSP 服务器 |
| POST | `/sandboxes/{id}/lsp/symbols/document` | 文档符号 |
| POST | `/sandboxes/{id}/lsp/symbols/workspace` | 工作区符号 |
| POST | `/sandboxes/{id}/lsp/completion` | 自动补全 |
| POST | `/sandboxes/{id}/lsp/hover` | 悬停信息 |
| POST | `/sandboxes/{id}/lsp/definition` | 转到定义 |
| POST | `/sandboxes/{id}/lsp/references` | 查找引用 |
| POST | `/sandboxes/{id}/lsp/diagnostics` | 获取诊断 |
| POST | `/sandboxes/{id}/lsp/rename` | 重命名 |
| POST | `/sandboxes/{id}/lsp/codeAction` | 代码操作 |
| POST | `/sandboxes/{id}/lsp/format` | 格式化代码 |

### 9.7 Snapshot 服务

| 方法 | 端点 | 描述 |
|-----|------|------|
| POST | `/snapshots` | 创建快照 |
| GET | `/snapshots` | 列出快照 |
| GET | `/snapshots/{id}` | 获取快照 |
| DELETE | `/snapshots/{id}` | 删除快照 |

### 9.8 Volume 服务

| 方法 | 端点 | 描述 |
|-----|------|------|
| POST | `/volumes` | 创建卷 |
| GET | `/volumes` | 列出卷 |
| GET | `/volumes/{id}` | 获取卷 |
| DELETE | `/volumes/{id}` | 删除卷 |
| POST | `/volumes/{id}/resize` | 调整大小 |

---

## 10. OpenAPI 规范

完整的 OpenAPI 3.0 规范文件请参考 `openapi.yaml`。

### 10.1 基础信息

```yaml
openapi: 3.0.3
info:
  title: Elevo Workspace API
  description: 统一工作空间 SDK REST API
  version: 1.0.0
  contact:
    name: API Support
    email: api-support@example.com
  license:
    name: Apache 2.0
    url: https://www.apache.org/licenses/LICENSE-2.0

servers:
  - url: https://api.example.com/api/v1
    description: Production
  - url: https://staging-api.example.com/api/v1
    description: Staging
  - url: http://localhost:8080/api/v1
    description: Local Development

tags:
  - name: Sandbox
    description: Sandbox 生命周期管理
  - name: FileSystem
    description: 文件系统操作
  - name: Process
    description: 进程管理
  - name: PTY
    description: 伪终端
  - name: Git
    description: Git 版本控制
  - name: LSP
    description: 语言服务协议
  - name: Snapshot
    description: 快照管理
  - name: Volume
    description: 卷管理
```

### 10.2 安全定义

```yaml
components:
  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
      bearerFormat: JWT
    apiKeyAuth:
      type: apiKey
      in: header
      name: X-API-Key

security:
  - bearerAuth: []
  - apiKeyAuth: []
```

### 10.3 通用响应

```yaml
components:
  responses:
    BadRequest:
      description: 请求参数错误
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    Unauthorized:
      description: 未认证
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    NotFound:
      description: 资源不存在
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    TooManyRequests:
      description: 超过速率限制
      headers:
        Retry-After:
          schema:
            type: integer
          description: 重试等待时间（秒）
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'

  schemas:
    ErrorResponse:
      type: object
      required:
        - error
      properties:
        error:
          type: object
          required:
            - code
            - name
            - message
          properties:
            code:
              type: integer
              description: 错误码
            name:
              type: string
              description: 错误名称
            message:
              type: string
              description: 错误消息
            details:
              type: object
              additionalProperties: true
            requestId:
              type: string
            docUrl:
              type: string
              format: uri
```

### 10.4 Sandbox 端点示例

```yaml
paths:
  /sandboxes:
    post:
      tags:
        - Sandbox
      summary: 创建 Sandbox
      operationId: createSandbox
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/CreateSandboxRequest'
      responses:
        '201':
          description: 创建成功
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/SandboxInfo'
        '400':
          $ref: '#/components/responses/BadRequest'
        '401':
          $ref: '#/components/responses/Unauthorized'
        '429':
          $ref: '#/components/responses/TooManyRequests'

    get:
      tags:
        - Sandbox
      summary: 列出 Sandbox
      operationId: listSandboxes
      parameters:
        - name: page
          in: query
          schema:
            type: integer
            default: 1
        - name: limit
          in: query
          schema:
            type: integer
            default: 20
            maximum: 100
        - name: state
          in: query
          schema:
            type: string
            enum: [creating, running, stopped, paused, failed, deleted]
        - name: labels
          in: query
          schema:
            type: string
          description: 标签筛选，格式 key1=value1,key2=value2
      responses:
        '200':
          description: 成功
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/SandboxList'

  /sandboxes/{id}:
    get:
      tags:
        - Sandbox
      summary: 获取 Sandbox
      operationId: getSandbox
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
      responses:
        '200':
          description: 成功
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/SandboxInfo'
        '404':
          $ref: '#/components/responses/NotFound'

    delete:
      tags:
        - Sandbox
      summary: 删除 Sandbox
      operationId: deleteSandbox
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
        - name: force
          in: query
          schema:
            type: boolean
            default: false
      responses:
        '204':
          description: 删除成功
        '404':
          $ref: '#/components/responses/NotFound'
```

---

## 附录

### A. SDK 代码生成

可以使用 OpenAPI Generator 从规范生成 SDK：

```bash
# TypeScript
openapi-generator-cli generate \
  -i openapi.yaml \
  -g typescript-fetch \
  -o ./sdk/typescript

# Python
openapi-generator-cli generate \
  -i openapi.yaml \
  -g python \
  -o ./sdk/python
```

### B. 客户端库推荐配置

#### TypeScript

```typescript
const client = new WorkspaceClient({
  apiUrl: 'https://api.example.com',
  apiKey: process.env.WORKSPACE_API_KEY,
  timeout: 30000,
  retries: 3,
  retryDelay: 1000
})
```

#### Python

```python
client = WorkspaceClient(
    api_url="https://api.example.com",
    api_key=os.environ["WORKSPACE_API_KEY"],
    timeout=30.0,
    retries=3,
    retry_delay=1.0
)
```

### C. 环境变量

| 变量 | 描述 |
|-----|------|
| `WORKSPACE_API_URL` | API 基础 URL |
| `WORKSPACE_API_KEY` | API 密钥 |
| `WORKSPACE_TIMEOUT` | 默认超时（秒）|
| `WORKSPACE_DEBUG` | 启用调试日志 |
