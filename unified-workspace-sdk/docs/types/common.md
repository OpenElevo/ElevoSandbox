# 通用类型定义

本文档定义了 Unified Workspace SDK 中使用的所有通用类型、枚举和数据结构。

---

## 目录

- [1. 基础类型](#1-基础类型)
- [2. 枚举类型](#2-枚举类型)
- [3. 配置类型](#3-配置类型)
- [4. 资源类型](#4-资源类型)
- [5. 分页类型](#5-分页类型)
- [6. 错误类型](#6-错误类型)
- [7. 回调类型](#7-回调类型)

---

## 1. 基础类型

### 1.1 Timestamp

时间戳类型，用于表示时间点。

```typescript
/**
 * ISO 8601 格式的时间字符串或 Date 对象
 * @example "2024-01-15T10:30:00Z"
 */
type Timestamp = string | Date
```

```python
from datetime import datetime
from typing import Union

Timestamp = Union[str, datetime]
```

### 1.2 Duration

持续时间类型。

```typescript
/**
 * 持续时间（毫秒）
 * @minimum 0
 * @maximum 86400000 (24小时)
 */
type DurationMs = number

/**
 * 持续时间（秒）
 * @minimum 0
 * @maximum 86400 (24小时)
 */
type DurationSec = number
```

### 1.3 Labels 和 Metadata

标签和元数据类型。

```typescript
/**
 * 标签键值对
 * - 键：1-63 个字符，字母数字和 -_.
 * - 值：0-255 个字符
 */
type Labels = Record<string, string>

/**
 * 元数据键值对
 * - 键：1-255 个字符
 * - 值：任意字符串
 */
type Metadata = Record<string, string>
```

### 1.4 Username

用户名类型。

```typescript
/**
 * 操作系统用户名
 * @pattern ^[a-z_][a-z0-9_-]*[$]?$
 * @minLength 1
 * @maxLength 32
 * @default "user"
 */
type Username = string
```

---

## 2. 枚举类型

### 2.1 SandboxState

Sandbox 状态枚举。

```typescript
/**
 * Sandbox 运行状态
 */
enum SandboxState {
  /** 等待创建/调度 */
  PENDING = 'pending',
  /** 正在启动 */
  STARTING = 'starting',
  /** 运行中 */
  RUNNING = 'running',
  /** 正在停止 */
  STOPPING = 'stopping',
  /** 已停止 */
  STOPPED = 'stopped',
  /** 已暂停（可恢复） */
  PAUSED = 'paused',
  /** 错误状态 */
  ERROR = 'error',
  /** 已归档 */
  ARCHIVED = 'archived'
}
```

```python
from enum import Enum

class SandboxState(str, Enum):
    PENDING = "pending"
    STARTING = "starting"
    RUNNING = "running"
    STOPPING = "stopping"
    STOPPED = "stopped"
    PAUSED = "paused"
    ERROR = "error"
    ARCHIVED = "archived"
```

**状态转换图**:

```
                    ┌─────────┐
                    │ PENDING │
                    └────┬────┘
                         │ create
                         ▼
                    ┌─────────┐
         ┌─────────│STARTING │─────────┐
         │ error   └────┬────┘         │ timeout
         ▼              │ success      ▼
    ┌─────────┐         ▼         ┌─────────┐
    │  ERROR  │←───┌─────────┐───→│ STOPPED │
    └─────────┘    │ RUNNING │    └────┬────┘
                   └────┬────┘         │
                        │              │ start
              pause │   │ stop         │
                    ▼   ▼              ▼
               ┌─────────┐        ┌─────────┐
               │ PAUSED  │───────→│STARTING │
               └─────────┘ resume └─────────┘
```

### 2.2 FileType

文件类型枚举。

```typescript
/**
 * 文件系统条目类型
 */
enum FileType {
  /** 普通文件 */
  FILE = 'file',
  /** 目录 */
  DIRECTORY = 'dir',
  /** 符号链接 */
  SYMLINK = 'symlink',
  /** 块设备 */
  BLOCK_DEVICE = 'block',
  /** 字符设备 */
  CHAR_DEVICE = 'char',
  /** 命名管道 */
  FIFO = 'fifo',
  /** Unix 套接字 */
  SOCKET = 'socket'
}
```

```python
class FileType(str, Enum):
    FILE = "file"
    DIRECTORY = "dir"
    SYMLINK = "symlink"
    BLOCK_DEVICE = "block"
    CHAR_DEVICE = "char"
    FIFO = "fifo"
    SOCKET = "socket"
```

### 2.3 CodeLanguage

编程语言枚举。

```typescript
/**
 * 支持的编程语言
 */
enum CodeLanguage {
  PYTHON = 'python',
  JAVASCRIPT = 'javascript',
  TYPESCRIPT = 'typescript',
  BASH = 'bash',
  GO = 'go',
  RUST = 'rust',
  JAVA = 'java',
  CPP = 'cpp',
  C = 'c',
  RUBY = 'ruby',
  PHP = 'php'
}
```

### 2.4 LSPLanguageId

LSP 语言标识枚举。

```typescript
/**
 * LSP 语言服务器支持的语言
 */
enum LSPLanguageId {
  PYTHON = 'python',
  TYPESCRIPT = 'typescript',
  JAVASCRIPT = 'javascript',
  GO = 'go',
  RUST = 'rust',
  JAVA = 'java',
  C = 'c',
  CPP = 'cpp',
  CSHARP = 'csharp'
}
```

### 2.5 DiagnosticSeverity

诊断严重性枚举。

```typescript
/**
 * LSP 诊断消息严重性
 */
enum DiagnosticSeverity {
  /** 错误 */
  ERROR = 1,
  /** 警告 */
  WARNING = 2,
  /** 信息 */
  INFORMATION = 3,
  /** 提示 */
  HINT = 4
}
```

### 2.6 SymbolKind

符号类型枚举。

```typescript
/**
 * LSP 符号类型
 */
enum SymbolKind {
  FILE = 1,
  MODULE = 2,
  NAMESPACE = 3,
  PACKAGE = 4,
  CLASS = 5,
  METHOD = 6,
  PROPERTY = 7,
  FIELD = 8,
  CONSTRUCTOR = 9,
  ENUM = 10,
  INTERFACE = 11,
  FUNCTION = 12,
  VARIABLE = 13,
  CONSTANT = 14,
  STRING = 15,
  NUMBER = 16,
  BOOLEAN = 17,
  ARRAY = 18,
  OBJECT = 19,
  KEY = 20,
  NULL = 21,
  ENUM_MEMBER = 22,
  STRUCT = 23,
  EVENT = 24,
  OPERATOR = 25,
  TYPE_PARAMETER = 26
}
```

### 2.7 CompletionItemKind

补全项类型枚举。

```typescript
/**
 * LSP 补全项类型
 */
enum CompletionItemKind {
  TEXT = 1,
  METHOD = 2,
  FUNCTION = 3,
  CONSTRUCTOR = 4,
  FIELD = 5,
  VARIABLE = 6,
  CLASS = 7,
  INTERFACE = 8,
  MODULE = 9,
  PROPERTY = 10,
  UNIT = 11,
  VALUE = 12,
  ENUM = 13,
  KEYWORD = 14,
  SNIPPET = 15,
  COLOR = 16,
  FILE = 17,
  REFERENCE = 18,
  FOLDER = 19,
  ENUM_MEMBER = 20,
  CONSTANT = 21,
  STRUCT = 22,
  EVENT = 23,
  OPERATOR = 24,
  TYPE_PARAMETER = 25
}
```

### 2.8 FileEventType

文件事件类型枚举。

```typescript
/**
 * 文件系统监视事件类型
 */
enum FileEventType {
  /** 文件/目录创建 */
  CREATED = 'created',
  /** 文件/目录修改 */
  MODIFIED = 'modified',
  /** 文件/目录删除 */
  DELETED = 'deleted',
  /** 文件/目录重命名 */
  RENAMED = 'renamed'
}
```

### 2.9 ProcessSignal

进程信号枚举。

```typescript
/**
 * Unix 进程信号
 */
enum ProcessSignal {
  SIGHUP = 'SIGHUP',
  SIGINT = 'SIGINT',
  SIGQUIT = 'SIGQUIT',
  SIGKILL = 'SIGKILL',
  SIGTERM = 'SIGTERM',
  SIGSTOP = 'SIGSTOP',
  SIGCONT = 'SIGCONT',
  SIGUSR1 = 'SIGUSR1',
  SIGUSR2 = 'SIGUSR2'
}
```

---

## 3. 配置类型

### 3.1 SDKConfig

SDK 客户端配置。

```typescript
/**
 * SDK 客户端配置
 */
interface SDKConfig {
  /**
   * API 服务器地址
   * @format uri
   * @example "https://api.workspace.example.com"
   */
  apiUrl: string

  /**
   * API 密钥（与 token 二选一）
   * @pattern ^[a-zA-Z0-9_-]+$
   */
  apiKey?: string

  /**
   * JWT 访问令牌（与 apiKey 二选一）
   */
  token?: string

  /**
   * 组织 ID（多租户场景）
   */
  organizationId?: string

  /**
   * 默认请求超时时间（毫秒）
   * @default 30000
   * @minimum 1000
   * @maximum 300000
   */
  timeout?: number

  /**
   * 请求重试次数
   * @default 3
   * @minimum 0
   * @maximum 10
   */
  retries?: number

  /**
   * 重试延迟基数（毫秒）
   * @default 1000
   */
  retryDelay?: number

  /**
   * 是否启用调试日志
   * @default false
   */
  debug?: boolean

  /**
   * 自定义 HTTP 头
   */
  headers?: Record<string, string>

  /**
   * 代理服务器地址
   * @format uri
   */
  proxy?: string
}
```

```python
from dataclasses import dataclass, field
from typing import Optional, Dict

@dataclass
class SDKConfig:
    """SDK 客户端配置"""

    api_url: str
    """API 服务器地址"""

    api_key: Optional[str] = None
    """API 密钥"""

    token: Optional[str] = None
    """JWT 访问令牌"""

    organization_id: Optional[str] = None
    """组织 ID"""

    timeout: int = 30000
    """默认请求超时时间（毫秒）"""

    retries: int = 3
    """请求重试次数"""

    retry_delay: int = 1000
    """重试延迟基数（毫秒）"""

    debug: bool = False
    """是否启用调试日志"""

    headers: Dict[str, str] = field(default_factory=dict)
    """自定义 HTTP 头"""

    proxy: Optional[str] = None
    """代理服务器地址"""
```

### 3.2 RequestOptions

请求选项。

```typescript
/**
 * 单次请求的选项
 */
interface RequestOptions {
  /**
   * 请求超时时间（毫秒），覆盖默认值
   * @minimum 1000
   * @maximum 600000
   */
  timeout?: number

  /**
   * 取消信号
   */
  signal?: AbortSignal

  /**
   * 额外的 HTTP 头
   */
  headers?: Record<string, string>

  /**
   * 是否禁用重试
   * @default false
   */
  noRetry?: boolean
}
```

---

## 4. 资源类型

### 4.1 Resources

资源配置。

```typescript
/**
 * Sandbox 资源配置
 */
interface Resources {
  /**
   * CPU 核心数
   * @minimum 0.5
   * @maximum 96
   * @default 1
   */
  cpu?: number

  /**
   * 内存大小（MB）
   * @minimum 256
   * @maximum 393216 (384GB)
   * @default 1024
   */
  memory?: number

  /**
   * 磁盘大小（GB）
   * @minimum 1
   * @maximum 2048
   * @default 10
   */
  disk?: number

  /**
   * GPU 数量
   * @minimum 0
   * @maximum 8
   * @default 0
   */
  gpu?: number

  /**
   * GPU 类型（如果 gpu > 0）
   * @example "nvidia-tesla-t4"
   */
  gpuType?: string
}
```

```python
@dataclass
class Resources:
    """Sandbox 资源配置"""

    cpu: Optional[float] = None
    """CPU 核心数 (0.5-96)"""

    memory: Optional[int] = None
    """内存大小 MB (256-393216)"""

    disk: Optional[int] = None
    """磁盘大小 GB (1-2048)"""

    gpu: Optional[int] = None
    """GPU 数量 (0-8)"""

    gpu_type: Optional[str] = None
    """GPU 类型"""
```

### 4.2 NetworkConfig

网络配置。

```typescript
/**
 * 网络配置
 */
interface NetworkConfig {
  /**
   * 是否允许访问公网
   * @default true
   */
  allowInternet?: boolean

  /**
   * 允许访问的出站 CIDR 列表
   * 如果设置，只允许访问这些地址
   * @example ["10.0.0.0/8", "192.168.0.0/16"]
   */
  allowedCIDRs?: string[]

  /**
   * 禁止访问的出站 CIDR 列表
   * @example ["169.254.169.254/32"]
   */
  deniedCIDRs?: string[]

  /**
   * 需要暴露的端口列表
   * @example [8080, 3000, 5432]
   */
  exposedPorts?: number[]

  /**
   * 是否允许入站公网流量
   * @default true
   */
  allowPublicInbound?: boolean

  /**
   * DNS 服务器列表
   * @example ["8.8.8.8", "8.8.4.4"]
   */
  dnsServers?: string[]

  /**
   * 自定义 hosts 映射
   * @example {"internal.api": "10.0.0.100"}
   */
  extraHosts?: Record<string, string>
}
```

### 4.3 VolumeMount

卷挂载配置。

```typescript
/**
 * 卷挂载配置
 */
interface VolumeMount {
  /**
   * 卷 ID 或名称
   */
  volumeId: string

  /**
   * 容器内挂载路径
   * @pattern ^/[a-zA-Z0-9_/-]+$
   * @example "/data"
   */
  mountPath: string

  /**
   * 卷内子路径（可选）
   * @example "subdir/path"
   */
  subPath?: string

  /**
   * 是否只读挂载
   * @default false
   */
  readOnly?: boolean
}
```

---

## 5. 分页类型

### 5.1 PaginationParams

分页请求参数。

```typescript
/**
 * 分页请求参数
 */
interface PaginationParams {
  /**
   * 页码（从 1 开始）
   * @minimum 1
   * @default 1
   */
  page?: number

  /**
   * 每页数量
   * @minimum 1
   * @maximum 100
   * @default 20
   */
  limit?: number

  /**
   * 排序字段
   * @example "createdAt"
   */
  sortBy?: string

  /**
   * 排序方向
   * @default "desc"
   */
  sortOrder?: 'asc' | 'desc'
}
```

### 5.2 PaginatedResult

分页响应结果。

```typescript
/**
 * 分页响应结果
 */
interface PaginatedResult<T> {
  /**
   * 数据列表
   */
  items: T[]

  /**
   * 总记录数
   */
  total: number

  /**
   * 当前页码
   */
  page: number

  /**
   * 每页数量
   */
  limit: number

  /**
   * 总页数
   */
  totalPages: number

  /**
   * 是否有下一页
   */
  hasNext: boolean

  /**
   * 是否有上一页
   */
  hasPrev: boolean
}
```

```python
from dataclasses import dataclass
from typing import TypeVar, Generic, List

T = TypeVar('T')

@dataclass
class PaginatedResult(Generic[T]):
    """分页响应结果"""

    items: List[T]
    """数据列表"""

    total: int
    """总记录数"""

    page: int
    """当前页码"""

    limit: int
    """每页数量"""

    total_pages: int
    """总页数"""

    has_next: bool
    """是否有下一页"""

    has_prev: bool
    """是否有上一页"""
```

### 5.3 CursorPaginationParams

游标分页参数（用于大数据集）。

```typescript
/**
 * 游标分页参数
 */
interface CursorPaginationParams {
  /**
   * 游标位置
   */
  cursor?: string

  /**
   * 返回数量
   * @minimum 1
   * @maximum 100
   * @default 20
   */
  limit?: number

  /**
   * 分页方向
   * @default "forward"
   */
  direction?: 'forward' | 'backward'
}

/**
 * 游标分页结果
 */
interface CursorPaginatedResult<T> {
  items: T[]
  nextCursor?: string
  prevCursor?: string
  hasMore: boolean
}
```

---

## 6. 错误类型

### 6.1 ErrorCode

错误码枚举。

```typescript
/**
 * 错误码枚举
 */
enum ErrorCode {
  // 通用错误 (1xxx)
  UNKNOWN_ERROR = 1000,
  INVALID_REQUEST = 1001,
  UNAUTHORIZED = 1002,
  FORBIDDEN = 1003,
  NOT_FOUND = 1004,
  CONFLICT = 1005,
  RATE_LIMITED = 1006,
  INTERNAL_ERROR = 1007,
  SERVICE_UNAVAILABLE = 1008,
  TIMEOUT = 1009,

  // Sandbox 错误 (2xxx)
  SANDBOX_NOT_FOUND = 2000,
  SANDBOX_ALREADY_EXISTS = 2001,
  SANDBOX_NOT_RUNNING = 2002,
  SANDBOX_ALREADY_RUNNING = 2003,
  SANDBOX_START_FAILED = 2004,
  SANDBOX_STOP_FAILED = 2005,
  SANDBOX_LIMIT_EXCEEDED = 2006,
  SANDBOX_RESOURCE_INSUFFICIENT = 2007,
  SANDBOX_TIMEOUT = 2008,

  // 文件系统错误 (3xxx)
  FILE_NOT_FOUND = 3000,
  FILE_ALREADY_EXISTS = 3001,
  FILE_PERMISSION_DENIED = 3002,
  FILE_IS_DIRECTORY = 3003,
  FILE_NOT_DIRECTORY = 3004,
  FILE_TOO_LARGE = 3005,
  FILE_PATH_INVALID = 3006,
  FILE_OPERATION_FAILED = 3007,

  // 进程错误 (4xxx)
  PROCESS_NOT_FOUND = 4000,
  PROCESS_ALREADY_RUNNING = 4001,
  PROCESS_TIMEOUT = 4002,
  PROCESS_FAILED = 4003,
  PROCESS_KILLED = 4004,

  // Git 错误 (5xxx)
  GIT_ERROR = 5000,
  GIT_AUTH_FAILED = 5001,
  GIT_CLONE_FAILED = 5002,
  GIT_PUSH_FAILED = 5003,
  GIT_MERGE_CONFLICT = 5004,
  GIT_REPO_NOT_FOUND = 5005,

  // LSP 错误 (6xxx)
  LSP_NOT_FOUND = 6000,
  LSP_START_FAILED = 6001,
  LSP_NOT_SUPPORTED = 6002,

  // 快照/卷错误 (7xxx)
  SNAPSHOT_NOT_FOUND = 7000,
  SNAPSHOT_CREATE_FAILED = 7001,
  VOLUME_NOT_FOUND = 7100,
  VOLUME_IN_USE = 7101,
  VOLUME_CREATE_FAILED = 7102
}
```

### 6.2 APIError

API 错误响应。

```typescript
/**
 * API 错误响应
 */
interface APIError {
  /**
   * 错误码
   */
  code: ErrorCode

  /**
   * 错误消息
   */
  message: string

  /**
   * 详细错误信息
   */
  details?: Record<string, unknown>

  /**
   * 请求 ID（用于追踪）
   */
  requestId?: string

  /**
   * 文档链接
   */
  docUrl?: string
}
```

### 6.3 SDK 异常类

```typescript
/**
 * SDK 基础异常
 */
class WorkspaceError extends Error {
  code: ErrorCode
  details?: Record<string, unknown>
  requestId?: string

  constructor(message: string, code: ErrorCode, details?: Record<string, unknown>) {
    super(message)
    this.name = 'WorkspaceError'
    this.code = code
    this.details = details
  }
}

/**
 * 资源未找到异常
 */
class NotFoundError extends WorkspaceError {
  constructor(resource: string, id: string) {
    super(`${resource} not found: ${id}`, ErrorCode.NOT_FOUND)
    this.name = 'NotFoundError'
  }
}

/**
 * 认证失败异常
 */
class UnauthorizedError extends WorkspaceError {
  constructor(message = 'Unauthorized') {
    super(message, ErrorCode.UNAUTHORIZED)
    this.name = 'UnauthorizedError'
  }
}

/**
 * 请求超时异常
 */
class TimeoutError extends WorkspaceError {
  constructor(operation: string, timeout: number) {
    super(`Operation '${operation}' timed out after ${timeout}ms`, ErrorCode.TIMEOUT)
    this.name = 'TimeoutError'
  }
}

/**
 * 命令执行失败异常
 */
class CommandError extends WorkspaceError {
  exitCode: number
  stdout: string
  stderr: string

  constructor(exitCode: number, stdout: string, stderr: string) {
    super(`Command failed with exit code ${exitCode}`, ErrorCode.PROCESS_FAILED)
    this.name = 'CommandError'
    this.exitCode = exitCode
    this.stdout = stdout
    this.stderr = stderr
  }
}
```

```python
class WorkspaceError(Exception):
    """SDK 基础异常"""

    def __init__(self, message: str, code: int, details: dict = None):
        super().__init__(message)
        self.code = code
        self.details = details or {}
        self.request_id: Optional[str] = None


class NotFoundError(WorkspaceError):
    """资源未找到异常"""

    def __init__(self, resource: str, id: str):
        super().__init__(f"{resource} not found: {id}", 1004)


class UnauthorizedError(WorkspaceError):
    """认证失败异常"""

    def __init__(self, message: str = "Unauthorized"):
        super().__init__(message, 1002)


class TimeoutError(WorkspaceError):
    """请求超时异常"""

    def __init__(self, operation: str, timeout: int):
        super().__init__(f"Operation '{operation}' timed out after {timeout}ms", 1009)


class CommandError(WorkspaceError):
    """命令执行失败异常"""

    def __init__(self, exit_code: int, stdout: str, stderr: str):
        super().__init__(f"Command failed with exit code {exit_code}", 4003)
        self.exit_code = exit_code
        self.stdout = stdout
        self.stderr = stderr
```

---

## 7. 回调类型

### 7.1 事件回调

```typescript
/**
 * 标准输出回调
 */
type StdoutCallback = (data: string) => void | Promise<void>

/**
 * 标准错误回调
 */
type StderrCallback = (data: string) => void | Promise<void>

/**
 * 文件事件回调
 */
type FileEventCallback = (event: FileEvent) => void | Promise<void>

/**
 * PTY 数据回调
 */
type PTYDataCallback = (data: Uint8Array) => void | Promise<void>

/**
 * 进程退出回调
 */
type ProcessExitCallback = (exitCode: number, signal?: string) => void | Promise<void>
```

### 7.2 进度回调

```typescript
/**
 * 进度信息
 */
interface ProgressInfo {
  /** 当前进度 (0-100) */
  percent: number
  /** 已处理字节数 */
  loaded: number
  /** 总字节数 */
  total: number
  /** 传输速度 (bytes/sec) */
  speed?: number
  /** 预计剩余时间 (秒) */
  eta?: number
}

/**
 * 进度回调
 */
type ProgressCallback = (progress: ProgressInfo) => void
```

---

## 8. JSON Schema

以下是用于验证的 JSON Schema 定义：

### 8.1 SDKConfig Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://workspace-sdk.example.com/schemas/sdk-config.json",
  "title": "SDKConfig",
  "type": "object",
  "required": ["apiUrl"],
  "properties": {
    "apiUrl": {
      "type": "string",
      "format": "uri",
      "description": "API 服务器地址"
    },
    "apiKey": {
      "type": "string",
      "pattern": "^[a-zA-Z0-9_-]+$",
      "description": "API 密钥"
    },
    "token": {
      "type": "string",
      "description": "JWT 访问令牌"
    },
    "organizationId": {
      "type": "string",
      "description": "组织 ID"
    },
    "timeout": {
      "type": "integer",
      "minimum": 1000,
      "maximum": 300000,
      "default": 30000,
      "description": "默认请求超时时间（毫秒）"
    },
    "retries": {
      "type": "integer",
      "minimum": 0,
      "maximum": 10,
      "default": 3,
      "description": "请求重试次数"
    },
    "debug": {
      "type": "boolean",
      "default": false,
      "description": "是否启用调试日志"
    }
  },
  "additionalProperties": false
}
```

### 8.2 Resources Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://workspace-sdk.example.com/schemas/resources.json",
  "title": "Resources",
  "type": "object",
  "properties": {
    "cpu": {
      "type": "number",
      "minimum": 0.5,
      "maximum": 96,
      "default": 1,
      "description": "CPU 核心数"
    },
    "memory": {
      "type": "integer",
      "minimum": 256,
      "maximum": 393216,
      "default": 1024,
      "description": "内存大小（MB）"
    },
    "disk": {
      "type": "integer",
      "minimum": 1,
      "maximum": 2048,
      "default": 10,
      "description": "磁盘大小（GB）"
    },
    "gpu": {
      "type": "integer",
      "minimum": 0,
      "maximum": 8,
      "default": 0,
      "description": "GPU 数量"
    },
    "gpuType": {
      "type": "string",
      "description": "GPU 类型"
    }
  },
  "additionalProperties": false
}
```

---

## 附录：类型映射表

| TypeScript | Python | Go | JSON Schema |
|------------|--------|-----|-------------|
| `string` | `str` | `string` | `"type": "string"` |
| `number` | `float` | `float64` | `"type": "number"` |
| `boolean` | `bool` | `bool` | `"type": "boolean"` |
| `string[]` | `List[str]` | `[]string` | `"type": "array", "items": {"type": "string"}` |
| `Record<string, string>` | `Dict[str, str]` | `map[string]string` | `"type": "object", "additionalProperties": {"type": "string"}` |
| `Date` | `datetime` | `time.Time` | `"type": "string", "format": "date-time"` |
| `Uint8Array` | `bytes` | `[]byte` | `"type": "string", "contentEncoding": "base64"` |
| `T \| null` | `Optional[T]` | `*T` | `"anyOf": [{"type": "..."}, {"type": "null"}]` |
