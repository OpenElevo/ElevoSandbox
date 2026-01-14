# Sandbox 服务接口文档

Sandbox 服务提供沙盒实例的生命周期管理功能。

---

## 目录

- [1. 概述](#1-概述)
- [2. 类型定义](#2-类型定义)
- [3. 方法详情](#3-方法详情)
- [4. REST API](#4-rest-api)
- [5. 使用示例](#5-使用示例)
- [6. 错误处理](#6-错误处理)

---

## 1. 概述

Sandbox 服务是 SDK 的核心入口，负责创建和管理隔离的运行环境。

### 1.1 功能列表

| 方法 | 描述 |
|-----|------|
| `create` | 创建新的 Sandbox 实例 |
| `get` | 获取指定 Sandbox 的信息 |
| `list` | 列出所有 Sandbox |
| `delete` | 删除指定 Sandbox |

### 1.2 服务架构

```
┌─────────────────────────────────────────┐
│              SandboxService             │
├─────────────────────────────────────────┤
│  create() ──► Sandbox                   │
│  get()    ──► Sandbox                   │
│  list()   ──► Sandbox[]                 │
│  delete() ──► void                      │
└─────────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────┐
│               Sandbox                   │
├─────────────────────────────────────────┤
│  fs: FileSystem                         │
│  process: Process                       │
│  pty: PTY                               │
└─────────────────────────────────────────┘
```

---

## 2. 类型定义

### 2.1 CreateSandboxParams

创建 Sandbox 的参数。

```typescript
interface CreateSandboxParams {
  /**
   * 模板标识符
   * 指定 Sandbox 使用的运行时环境
   * @required
   * @example "python:3.11", "node:18", "ubuntu:22.04"
   */
  template: string

  /**
   * Sandbox 名称
   * 用于标识和检索，需在账户内唯一
   * @optional
   * @pattern ^[a-z0-9][a-z0-9-]{0,61}[a-z0-9]$
   * @example "my-sandbox"
   */
  name?: string

  /**
   * 环境变量
   * 在 Sandbox 内全局可用的环境变量
   * @optional
   * @example { "NODE_ENV": "development", "DEBUG": "true" }
   */
  envs?: Record<string, string>
}
```

**Python 定义**:

```python
@dataclass
class CreateSandboxParams:
    template: str
    """模板标识符，必填"""

    name: Optional[str] = None
    """Sandbox 名称，可选"""

    envs: Optional[Dict[str, str]] = None
    """环境变量，可选"""
```

### 2.2 Sandbox

Sandbox 实例，包含子服务访问器。

```typescript
interface Sandbox {
  /**
   * Sandbox 唯一标识符
   * @example "sbx-abc123def456"
   */
  readonly id: string

  /**
   * Sandbox 名称
   */
  readonly name?: string

  /**
   * 当前状态
   */
  readonly state: SandboxState

  /**
   * 创建时间
   */
  readonly createdAt: Date

  /**
   * 文件系统服务
   */
  readonly fs: FileSystem

  /**
   * 进程执行服务
   */
  readonly process: Process

  /**
   * 伪终端服务
   */
  readonly pty: PTY
}
```

**Python 定义**:

```python
@dataclass
class Sandbox:
    id: str
    """Sandbox 唯一标识符"""

    state: SandboxState
    """当前状态"""

    created_at: datetime
    """创建时间"""

    name: Optional[str] = None
    """Sandbox 名称"""

    # 子服务 (运行时注入)
    fs: "FileSystem" = field(init=False)
    process: "Process" = field(init=False)
    pty: "PTY" = field(init=False)
```

### 2.3 SandboxState

Sandbox 状态枚举。

```typescript
type SandboxState = 'running' | 'stopped'
```

**Python 定义**:

```python
class SandboxState(str, Enum):
    RUNNING = "running"
    STOPPED = "stopped"
```

### 2.4 SandboxService

Sandbox 服务接口。

```typescript
interface SandboxService {
  create(params: CreateSandboxParams): Promise<Sandbox>
  get(sandboxId: string): Promise<Sandbox>
  list(): Promise<Sandbox[]>
  delete(sandboxId: string): Promise<void>
}
```

**Python 定义**:

```python
class SandboxService(Protocol):
    async def create(self, params: CreateSandboxParams) -> Sandbox: ...
    async def get(self, sandbox_id: str) -> Sandbox: ...
    async def list(self) -> List[Sandbox]: ...
    async def delete(self, sandbox_id: str) -> None: ...
```

---

## 3. 方法详情

### 3.1 create

创建新的 Sandbox 实例。

**签名**:

```typescript
create(params: CreateSandboxParams): Promise<Sandbox>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `params` | `CreateSandboxParams` | 是 | 创建参数 |
| `params.template` | `string` | 是 | 模板标识符 |
| `params.name` | `string` | 否 | Sandbox 名称 |
| `params.envs` | `Record<string, string>` | 否 | 环境变量 |

**返回值**:

| 类型 | 描述 |
|-----|------|
| `Promise<Sandbox>` | 创建成功的 Sandbox 实例 |

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 2002 | `SANDBOX_ALREADY_EXISTS` | 同名 Sandbox 已存在 |
| 2004 | `SANDBOX_LIMIT_EXCEEDED` | 超过 Sandbox 数量限制 |
| 2007 | `TEMPLATE_NOT_FOUND` | 模板不存在 |

**示例**:

```typescript
// TypeScript
const sandbox = await client.sandbox.create({
  template: 'python:3.11',
  name: 'my-python-sandbox',
  envs: {
    PYTHONPATH: '/app/lib'
  }
})
console.log(`Created: ${sandbox.id}`)
```

```python
# Python
sandbox = await client.sandbox.create(CreateSandboxParams(
    template="python:3.11",
    name="my-python-sandbox",
    envs={"PYTHONPATH": "/app/lib"}
))
print(f"Created: {sandbox.id}")
```

---

### 3.2 get

获取指定 Sandbox 的信息。

**签名**:

```typescript
get(sandboxId: string): Promise<Sandbox>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `sandboxId` | `string` | 是 | Sandbox ID |

**返回值**:

| 类型 | 描述 |
|-----|------|
| `Promise<Sandbox>` | Sandbox 实例 |

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 2001 | `SANDBOX_NOT_FOUND` | Sandbox 不存在 |

**示例**:

```typescript
// TypeScript
const sandbox = await client.sandbox.get('sbx-abc123')
console.log(`State: ${sandbox.state}`)
```

```python
# Python
sandbox = await client.sandbox.get("sbx-abc123")
print(f"State: {sandbox.state}")
```

---

### 3.3 list

列出所有 Sandbox。

**签名**:

```typescript
list(): Promise<Sandbox[]>
```

**参数**: 无

**返回值**:

| 类型 | 描述 |
|-----|------|
| `Promise<Sandbox[]>` | Sandbox 列表 |

**示例**:

```typescript
// TypeScript
const sandboxes = await client.sandbox.list()
for (const sb of sandboxes) {
  console.log(`${sb.id}: ${sb.state}`)
}
```

```python
# Python
sandboxes = await client.sandbox.list()
for sb in sandboxes:
    print(f"{sb.id}: {sb.state}")
```

---

### 3.4 delete

删除指定 Sandbox。

**签名**:

```typescript
delete(sandboxId: string): Promise<void>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `sandboxId` | `string` | 是 | Sandbox ID |

**返回值**: 无

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 2001 | `SANDBOX_NOT_FOUND` | Sandbox 不存在 |

**示例**:

```typescript
// TypeScript
await client.sandbox.delete('sbx-abc123')
console.log('Sandbox deleted')
```

```python
# Python
await client.sandbox.delete("sbx-abc123")
print("Sandbox deleted")
```

---

## 4. REST API

### 4.1 创建 Sandbox

```
POST /api/v1/sandboxes
```

**请求**:

```json
{
  "template": "python:3.11",
  "name": "my-sandbox",
  "envs": {
    "DEBUG": "true"
  }
}
```

**响应** (201 Created):

```json
{
  "id": "sbx-abc123def456",
  "name": "my-sandbox",
  "state": "running",
  "createdAt": "2024-01-15T10:30:00Z"
}
```

### 4.2 获取 Sandbox

```
GET /api/v1/sandboxes/{id}
```

**响应** (200 OK):

```json
{
  "id": "sbx-abc123def456",
  "name": "my-sandbox",
  "state": "running",
  "createdAt": "2024-01-15T10:30:00Z"
}
```

### 4.3 列出 Sandbox

```
GET /api/v1/sandboxes
```

**响应** (200 OK):

```json
[
  {
    "id": "sbx-abc123",
    "name": "sandbox-1",
    "state": "running",
    "createdAt": "2024-01-15T10:30:00Z"
  },
  {
    "id": "sbx-def456",
    "name": "sandbox-2",
    "state": "stopped",
    "createdAt": "2024-01-15T11:00:00Z"
  }
]
```

### 4.4 删除 Sandbox

```
DELETE /api/v1/sandboxes/{id}
```

**响应** (204 No Content): 无响应体

---

## 5. 使用示例

### 5.1 基础使用

```typescript
// TypeScript
import { WorkspaceClient } from '@workspace-sdk/typescript'

async function main() {
  const client = new WorkspaceClient({
    apiKey: process.env.API_KEY
  })

  // 创建 Sandbox
  const sandbox = await client.sandbox.create({
    template: 'python:3.11',
    name: 'demo'
  })

  try {
    // 使用 Sandbox
    const result = await sandbox.process.run('python --version')
    console.log(result.stdout)
  } finally {
    // 清理
    await client.sandbox.delete(sandbox.id)
  }
}
```

```python
# Python
from workspace_sdk import WorkspaceClient, CreateSandboxParams

async def main():
    client = WorkspaceClient()

    # 创建 Sandbox
    sandbox = await client.sandbox.create(CreateSandboxParams(
        template="python:3.11",
        name="demo"
    ))

    try:
        # 使用 Sandbox
        result = await sandbox.process.run("python --version")
        print(result.stdout)
    finally:
        # 清理
        await client.sandbox.delete(sandbox.id)
```

### 5.2 使用上下文管理器 (Python)

```python
from workspace_sdk import WorkspaceClient, CreateSandboxParams

async def main():
    client = WorkspaceClient()

    # 使用上下文管理器自动清理
    async with client.sandbox.create(CreateSandboxParams(
        template="python:3.11"
    )) as sandbox:
        result = await sandbox.process.run("python --version")
        print(result.stdout)
    # 退出时自动删除
```

### 5.3 批量管理

```typescript
// TypeScript
async function cleanupAllSandboxes(client: WorkspaceClient) {
  const sandboxes = await client.sandbox.list()

  await Promise.all(
    sandboxes.map(sb => client.sandbox.delete(sb.id))
  )

  console.log(`Deleted ${sandboxes.length} sandboxes`)
}
```

---

## 6. 错误处理

### 6.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 2001 | `SANDBOX_NOT_FOUND` | 404 | Sandbox 不存在 |
| 2002 | `SANDBOX_ALREADY_EXISTS` | 409 | 同名 Sandbox 已存在 |
| 2004 | `SANDBOX_LIMIT_EXCEEDED` | 429 | 超过 Sandbox 数量限制 |
| 2005 | `SANDBOX_CREATION_FAILED` | 500 | 创建失败 |
| 2007 | `TEMPLATE_NOT_FOUND` | 404 | 模板不存在 |

### 6.2 错误处理示例

```typescript
// TypeScript
import { SandboxNotFoundError, TemplateNotFoundError } from '@workspace-sdk/typescript'

try {
  const sandbox = await client.sandbox.create({
    template: 'invalid-template'
  })
} catch (error) {
  if (error instanceof TemplateNotFoundError) {
    console.error('Template not found:', error.message)
  } else if (error instanceof SandboxNotFoundError) {
    console.error('Sandbox not found:', error.message)
  } else {
    throw error
  }
}
```

```python
# Python
from workspace_sdk.errors import SandboxNotFoundError, TemplateNotFoundError

try:
    sandbox = await client.sandbox.create(CreateSandboxParams(
        template="invalid-template"
    ))
except TemplateNotFoundError as e:
    print(f"Template not found: {e}")
except SandboxNotFoundError as e:
    print(f"Sandbox not found: {e}")
```
