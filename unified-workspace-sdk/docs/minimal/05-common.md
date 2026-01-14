# 通用类型和错误码

本文档定义 SDK 的通用类型、错误码和配置。

---

## 目录

- [1. SDK 配置](#1-sdk-配置)
- [2. 通用类型](#2-通用类型)
- [3. 错误码](#3-错误码)
- [4. 错误类](#4-错误类)

---

## 1. SDK 配置

### 1.1 SDKConfig

SDK 客户端配置。

```typescript
interface SDKConfig {
  /**
   * API 基础地址
   * @required
   * @example "https://api.example.com"
   */
  apiUrl: string

  /**
   * API 密钥
   * @required
   */
  apiKey: string

  /**
   * 请求超时时间 (毫秒)
   * @optional
   * @default 30000
   */
  timeout?: number
}
```

**Python 定义**:

```python
@dataclass
class SDKConfig:
    api_url: str
    """API 基础地址"""

    api_key: str
    """API 密钥"""

    timeout: int = 30000
    """请求超时时间 (毫秒)"""
```

### 1.2 客户端初始化

```typescript
// TypeScript
import { WorkspaceClient } from '@workspace-sdk/typescript'

const client = new WorkspaceClient({
  apiUrl: 'https://api.example.com',
  apiKey: process.env.WORKSPACE_API_KEY!,
  timeout: 30000
})
```

```python
# Python
from workspace_sdk import WorkspaceClient, SDKConfig

client = WorkspaceClient(SDKConfig(
    api_url="https://api.example.com",
    api_key=os.environ["WORKSPACE_API_KEY"],
    timeout=30000
))
```

### 1.3 环境变量

SDK 支持通过环境变量配置：

| 变量 | 描述 |
|-----|------|
| `WORKSPACE_API_URL` | API 基础地址 |
| `WORKSPACE_API_KEY` | API 密钥 |
| `WORKSPACE_TIMEOUT` | 请求超时时间 (毫秒) |

```typescript
// TypeScript - 使用环境变量
const client = new WorkspaceClient()  // 自动读取环境变量
```

```python
# Python - 使用环境变量
client = WorkspaceClient()  # 自动读取环境变量
```

---

## 2. 通用类型

### 2.1 SandboxState

Sandbox 状态枚举。

```typescript
type SandboxState = 'running' | 'stopped'
```

```python
class SandboxState(str, Enum):
    RUNNING = "running"
    STOPPED = "stopped"
```

### 2.2 FileType

文件类型枚举。

```typescript
type FileType = 'file' | 'directory'
```

```python
class FileType(str, Enum):
    FILE = "file"
    DIRECTORY = "directory"
```

### 2.3 FileInfo

文件信息。

```typescript
interface FileInfo {
  name: string
  path: string
  type: FileType
  size: number
}
```

```python
@dataclass
class FileInfo:
    name: str
    path: str
    type: FileType
    size: int
```

### 2.4 CommandResult

命令执行结果。

```typescript
interface CommandResult {
  exitCode: number
  stdout: string
  stderr: string
}
```

```python
@dataclass
class CommandResult:
    exit_code: int
    stdout: str
    stderr: str
```

---

## 3. 错误码

### 3.1 错误码分类

| 范围 | 类别 |
|-----|------|
| 1000-1999 | 认证错误 |
| 2000-2999 | Sandbox 错误 |
| 3000-3999 | FileSystem 错误 |
| 4000-4199 | Process/PTY 错误 |
| 9000-9999 | 系统错误 |

### 3.2 认证错误 (1000-1999)

| 错误码 | 名称 | HTTP | 描述 |
|-------|------|------|------|
| 1001 | `UNAUTHORIZED` | 401 | 未认证 |
| 1002 | `FORBIDDEN` | 403 | 无权限 |
| 1003 | `TOKEN_EXPIRED` | 401 | Token 已过期 |
| 1004 | `INVALID_TOKEN` | 401 | 无效 Token |

### 3.3 Sandbox 错误 (2000-2999)

| 错误码 | 名称 | HTTP | 描述 |
|-------|------|------|------|
| 2001 | `SANDBOX_NOT_FOUND` | 404 | Sandbox 不存在 |
| 2002 | `SANDBOX_ALREADY_EXISTS` | 409 | 同名 Sandbox 已存在 |
| 2004 | `SANDBOX_LIMIT_EXCEEDED` | 429 | 超过数量限制 |
| 2005 | `SANDBOX_CREATION_FAILED` | 500 | 创建失败 |
| 2007 | `TEMPLATE_NOT_FOUND` | 404 | 模板不存在 |

### 3.4 FileSystem 错误 (3000-3999)

| 错误码 | 名称 | HTTP | 描述 |
|-------|------|------|------|
| 3001 | `FILE_NOT_FOUND` | 404 | 文件不存在 |
| 3002 | `FILE_ALREADY_EXISTS` | 409 | 文件已存在 |
| 3003 | `DIRECTORY_NOT_EMPTY` | 409 | 目录非空 |
| 3004 | `PERMISSION_DENIED` | 403 | 权限不足 |
| 3005 | `DISK_QUOTA_EXCEEDED` | 507 | 磁盘空间不足 |
| 3006 | `INVALID_PATH` | 400 | 无效路径 |
| 3007 | `FILE_TOO_LARGE` | 413 | 文件过大 |
| 3008 | `NOT_A_DIRECTORY` | 400 | 不是目录 |
| 3009 | `NOT_A_FILE` | 400 | 不是文件 |

### 3.5 Process 错误 (4000-4099)

| 错误码 | 名称 | HTTP | 描述 |
|-------|------|------|------|
| 4002 | `PROCESS_TIMEOUT` | 408 | 执行超时 |
| 4004 | `COMMAND_FAILED` | 500 | 命令执行失败 |

### 3.6 PTY 错误 (4100-4199)

| 错误码 | 名称 | HTTP | 描述 |
|-------|------|------|------|
| 4100 | `PTY_NOT_FOUND` | 404 | PTY 不存在 |
| 4101 | `PTY_ALREADY_EXISTS` | 409 | PTY 已存在 |
| 4102 | `PTY_LIMIT_EXCEEDED` | 429 | 超过 PTY 数量限制 |
| 4103 | `PTY_CONNECTION_FAILED` | 500 | 连接失败 |
| 4104 | `PTY_WRITE_FAILED` | 500 | 写入失败 |
| 4105 | `PTY_RESIZE_FAILED` | 500 | 调整大小失败 |

### 3.7 系统错误 (9000-9999)

| 错误码 | 名称 | HTTP | 描述 |
|-------|------|------|------|
| 9001 | `INTERNAL_ERROR` | 500 | 内部错误 |
| 9002 | `SERVICE_UNAVAILABLE` | 503 | 服务不可用 |
| 9003 | `RATE_LIMITED` | 429 | 超过速率限制 |
| 9004 | `VALIDATION_ERROR` | 400 | 验证错误 |
| 9005 | `INVALID_REQUEST` | 400 | 无效请求 |

---

## 4. 错误类

### 4.1 TypeScript 错误类

```typescript
/**
 * SDK 基础错误类
 */
class WorkspaceError extends Error {
  /** 错误码 */
  readonly code: number
  /** 错误名称 */
  readonly name: string
  /** 详细信息 */
  readonly details?: Record<string, any>
  /** 请求 ID */
  readonly requestId?: string

  constructor(
    code: number,
    name: string,
    message: string,
    details?: Record<string, any>,
    requestId?: string
  ) {
    super(message)
    this.code = code
    this.name = name
    this.details = details
    this.requestId = requestId
  }
}

// Sandbox 错误
class SandboxNotFoundError extends WorkspaceError {
  constructor(sandboxId: string) {
    super(2001, 'SANDBOX_NOT_FOUND', `Sandbox '${sandboxId}' not found`, { sandboxId })
  }
}

class SandboxAlreadyExistsError extends WorkspaceError {
  constructor(name: string) {
    super(2002, 'SANDBOX_ALREADY_EXISTS', `Sandbox '${name}' already exists`, { name })
  }
}

class SandboxLimitExceededError extends WorkspaceError {
  constructor() {
    super(2004, 'SANDBOX_LIMIT_EXCEEDED', 'Sandbox limit exceeded')
  }
}

class TemplateNotFoundError extends WorkspaceError {
  constructor(template: string) {
    super(2007, 'TEMPLATE_NOT_FOUND', `Template '${template}' not found`, { template })
  }
}

// FileSystem 错误
class FileNotFoundError extends WorkspaceError {
  constructor(path: string) {
    super(3001, 'FILE_NOT_FOUND', `File '${path}' not found`, { path })
  }
}

class FileAlreadyExistsError extends WorkspaceError {
  constructor(path: string) {
    super(3002, 'FILE_ALREADY_EXISTS', `File '${path}' already exists`, { path })
  }
}

class PermissionDeniedError extends WorkspaceError {
  constructor(path: string) {
    super(3004, 'PERMISSION_DENIED', `Permission denied: '${path}'`, { path })
  }
}

class DiskQuotaExceededError extends WorkspaceError {
  constructor() {
    super(3005, 'DISK_QUOTA_EXCEEDED', 'Disk quota exceeded')
  }
}

// Process 错误
class ProcessTimeoutError extends WorkspaceError {
  constructor(timeoutMs: number) {
    super(4002, 'PROCESS_TIMEOUT', `Process timed out after ${timeoutMs}ms`, { timeoutMs })
  }
}

// PTY 错误
class PTYNotFoundError extends WorkspaceError {
  constructor(ptyId: string) {
    super(4100, 'PTY_NOT_FOUND', `PTY '${ptyId}' not found`, { ptyId })
  }
}

class PTYLimitExceededError extends WorkspaceError {
  constructor() {
    super(4102, 'PTY_LIMIT_EXCEEDED', 'PTY limit exceeded')
  }
}
```

### 4.2 Python 错误类

```python
from dataclasses import dataclass
from typing import Optional, Dict, Any

@dataclass
class WorkspaceError(Exception):
    """SDK 基础错误类"""
    code: int
    name: str
    message: str
    details: Optional[Dict[str, Any]] = None
    request_id: Optional[str] = None

    def __str__(self) -> str:
        return f"[{self.code}] {self.name}: {self.message}"


# Sandbox 错误
class SandboxNotFoundError(WorkspaceError):
    def __init__(self, sandbox_id: str):
        super().__init__(
            code=2001,
            name="SANDBOX_NOT_FOUND",
            message=f"Sandbox '{sandbox_id}' not found",
            details={"sandbox_id": sandbox_id}
        )


class SandboxAlreadyExistsError(WorkspaceError):
    def __init__(self, name: str):
        super().__init__(
            code=2002,
            name="SANDBOX_ALREADY_EXISTS",
            message=f"Sandbox '{name}' already exists",
            details={"name": name}
        )


class SandboxLimitExceededError(WorkspaceError):
    def __init__(self):
        super().__init__(
            code=2004,
            name="SANDBOX_LIMIT_EXCEEDED",
            message="Sandbox limit exceeded"
        )


class TemplateNotFoundError(WorkspaceError):
    def __init__(self, template: str):
        super().__init__(
            code=2007,
            name="TEMPLATE_NOT_FOUND",
            message=f"Template '{template}' not found",
            details={"template": template}
        )


# FileSystem 错误
class FileNotFoundError(WorkspaceError):
    def __init__(self, path: str):
        super().__init__(
            code=3001,
            name="FILE_NOT_FOUND",
            message=f"File '{path}' not found",
            details={"path": path}
        )


class FileAlreadyExistsError(WorkspaceError):
    def __init__(self, path: str):
        super().__init__(
            code=3002,
            name="FILE_ALREADY_EXISTS",
            message=f"File '{path}' already exists",
            details={"path": path}
        )


class PermissionDeniedError(WorkspaceError):
    def __init__(self, path: str):
        super().__init__(
            code=3004,
            name="PERMISSION_DENIED",
            message=f"Permission denied: '{path}'",
            details={"path": path}
        )


class DiskQuotaExceededError(WorkspaceError):
    def __init__(self):
        super().__init__(
            code=3005,
            name="DISK_QUOTA_EXCEEDED",
            message="Disk quota exceeded"
        )


# Process 错误
class ProcessTimeoutError(WorkspaceError):
    def __init__(self, timeout_ms: int):
        super().__init__(
            code=4002,
            name="PROCESS_TIMEOUT",
            message=f"Process timed out after {timeout_ms}ms",
            details={"timeout_ms": timeout_ms}
        )


# PTY 错误
class PTYNotFoundError(WorkspaceError):
    def __init__(self, pty_id: str):
        super().__init__(
            code=4100,
            name="PTY_NOT_FOUND",
            message=f"PTY '{pty_id}' not found",
            details={"pty_id": pty_id}
        )


class PTYLimitExceededError(WorkspaceError):
    def __init__(self):
        super().__init__(
            code=4102,
            name="PTY_LIMIT_EXCEEDED",
            message="PTY limit exceeded"
        )
```

### 4.3 错误处理示例

```typescript
// TypeScript
import {
  WorkspaceError,
  SandboxNotFoundError,
  FileNotFoundError,
  ProcessTimeoutError
} from '@workspace-sdk/typescript'

try {
  const sandbox = await client.sandbox.get('sbx-invalid')
} catch (error) {
  if (error instanceof SandboxNotFoundError) {
    console.error(`Sandbox not found: ${error.details?.sandboxId}`)
  } else if (error instanceof WorkspaceError) {
    console.error(`API Error [${error.code}]: ${error.message}`)
  } else {
    throw error
  }
}
```

```python
# Python
from workspace_sdk.errors import (
    WorkspaceError,
    SandboxNotFoundError,
    FileNotFoundError,
    ProcessTimeoutError
)

try:
    sandbox = await client.sandbox.get("sbx-invalid")
except SandboxNotFoundError as e:
    print(f"Sandbox not found: {e.details['sandbox_id']}")
except WorkspaceError as e:
    print(f"API Error [{e.code}]: {e.message}")
```

---

## 5. REST API 错误响应格式

```json
{
  "error": {
    "code": 2001,
    "name": "SANDBOX_NOT_FOUND",
    "message": "Sandbox 'sbx-abc123' not found",
    "details": {
      "sandboxId": "sbx-abc123"
    },
    "requestId": "req-xyz789"
  }
}
```
