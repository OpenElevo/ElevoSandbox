# 统一 Workspace SDK - 最简版文档

本文档是 Workspace SDK 最简版的完整 API 参考，仅包含 E2B 和 Daytona 两个 SDK 的公共交集功能。

---

## 概述

最简版包含 **4 个服务**，共 **15 个方法**，覆盖最基础的使用场景。

| 服务 | 方法数 | 描述 |
|-----|-------|------|
| [Sandbox](01-sandbox.md) | 4 | 沙盒生命周期管理 |
| [FileSystem](02-filesystem.md) | 7 | 文件系统操作 |
| [Process](03-process.md) | 1 | 命令执行 |
| [PTY](04-pty.md) | 3 | 交互式终端 |

---

## 快速开始

### 安装

```bash
# TypeScript
npm install @workspace-sdk/typescript

# Python
pip install workspace-sdk
```

### 基础使用

```typescript
// TypeScript
import { WorkspaceClient } from '@workspace-sdk/typescript'

async function main() {
  const client = new WorkspaceClient({
    apiUrl: 'https://api.example.com',
    apiKey: process.env.API_KEY!
  })

  // 创建 Sandbox
  const sandbox = await client.sandbox.create({
    template: 'python:3.11'
  })

  try {
    // 写入文件
    await sandbox.fs.write('/app/main.py', 'print("Hello World")')

    // 执行命令
    const result = await sandbox.process.run('python /app/main.py')
    console.log(result.stdout)  // Hello World
  } finally {
    // 删除 Sandbox
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
        template="python:3.11"
    ))

    try:
        # 写入文件
        await sandbox.fs.write("/app/main.py", 'print("Hello World")')

        # 执行命令
        result = await sandbox.process.run("python /app/main.py")
        print(result.stdout)  # Hello World
    finally:
        # 删除 Sandbox
        await client.sandbox.delete(sandbox.id)
```

---

## API 速查

### Sandbox 服务

| 方法 | 签名 | 描述 |
|-----|------|------|
| `create` | `create(params) → Sandbox` | 创建 Sandbox |
| `get` | `get(id) → Sandbox` | 获取 Sandbox |
| `list` | `list() → Sandbox[]` | 列出 Sandbox |
| `delete` | `delete(id) → void` | 删除 Sandbox |

### FileSystem 服务

| 方法 | 签名 | 描述 |
|-----|------|------|
| `read` | `read(path) → string` | 读取文件 |
| `write` | `write(path, content) → void` | 写入文件 |
| `mkdir` | `mkdir(path) → void` | 创建目录 |
| `list` | `list(path) → FileInfo[]` | 列出目录 |
| `remove` | `remove(path) → void` | 删除文件/目录 |
| `move` | `move(src, dst) → void` | 移动/重命名 |
| `getInfo` | `getInfo(path) → FileInfo` | 获取文件信息 |

### Process 服务

| 方法 | 签名 | 描述 |
|-----|------|------|
| `run` | `run(cmd, opts?) → CommandResult` | 执行命令 |

### PTY 服务

| 方法 | 签名 | 描述 |
|-----|------|------|
| `create` | `create(opts) → PTYHandle` | 创建 PTY |
| `resize` | `resize(id, cols, rows) → void` | 调整大小 |
| `kill` | `kill(id) → void` | 关闭 PTY |

---

## 文档结构

```
docs/minimal/
├── README.md           # 本文件
├── 01-sandbox.md       # Sandbox 服务
├── 02-filesystem.md    # FileSystem 服务
├── 03-process.md       # Process 服务
├── 04-pty.md           # PTY 服务
└── 05-common.md        # 通用类型和错误码
```

---

## 核心类型

```typescript
// Sandbox
interface Sandbox {
  id: string
  name?: string
  state: 'running' | 'stopped'
  createdAt: Date
  fs: FileSystem
  process: Process
  pty: PTY
}

// FileInfo
interface FileInfo {
  name: string
  path: string
  type: 'file' | 'directory'
  size: number
}

// CommandResult
interface CommandResult {
  exitCode: number
  stdout: string
  stderr: string
}

// PTYHandle
interface PTYHandle {
  id: string
  write(data: Uint8Array): Promise<void>
  kill(): Promise<void>
}
```

---

## 错误码速查

| 范围 | 类别 | 常见错误 |
|-----|------|---------|
| 2000-2999 | Sandbox | `SANDBOX_NOT_FOUND`, `TEMPLATE_NOT_FOUND` |
| 3000-3999 | FileSystem | `FILE_NOT_FOUND`, `PERMISSION_DENIED` |
| 4000-4099 | Process | `PROCESS_TIMEOUT` |
| 4100-4199 | PTY | `PTY_NOT_FOUND`, `PTY_LIMIT_EXCEEDED` |

详细错误码参见 [05-common.md](05-common.md#3-错误码)

---

## 与完整版的差异

最简版**不包含**以下功能：

| 功能 | 来源 |
|-----|------|
| `sandbox.start/stop` | Daytona |
| `sandbox.pause/getMetrics` | E2B |
| `fs.exists/watch` | E2B |
| `fs.findFiles/searchFiles` | Daytona |
| `process.spawn/list/kill` | E2B |
| `process.codeRun/Session` | Daytona |
| Git 服务 | Daytona |
| LSP 服务 | Daytona |
| Snapshot/Volume 服务 | Daytona |

如需这些功能，请参考 [完整版 API](../api-full.md)。
