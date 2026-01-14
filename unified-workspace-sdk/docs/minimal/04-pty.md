# PTY 服务接口文档

PTY (Pseudo-Terminal) 服务提供 Sandbox 内的交互式终端功能。

---

## 目录

- [1. 概述](#1-概述)
- [2. 类型定义](#2-类型定义)
- [3. 方法详情](#3-方法详情)
- [4. REST API / WebSocket](#4-rest-api--websocket)
- [5. 使用示例](#5-使用示例)
- [6. 错误处理](#6-错误处理)

---

## 1. 概述

PTY 服务通过 Sandbox 实例访问，提供交互式终端能力，适用于需要实时输入输出的场景。

### 1.1 功能列表

| 方法 | 描述 |
|-----|------|
| `create` | 创建新的 PTY 会话 |
| `resize` | 调整终端大小 |
| `kill` | 关闭 PTY 会话 |

### 1.2 访问方式

```typescript
const sandbox = await client.sandbox.get('sbx-abc123')
const pty = await sandbox.pty.create({
  cols: 80,
  rows: 24,
  onData: (data) => process.stdout.write(data)
})
```

### 1.3 PTY vs Process

| 特性 | PTY | Process |
|-----|-----|---------|
| 交互性 | 实时双向 | 单次执行 |
| 输出方式 | 流式回调 | 完整返回 |
| 适用场景 | 交互式 shell、vim、top | 脚本、命令执行 |
| 控制字符 | 支持 | 不支持 |

---

## 2. 类型定义

### 2.1 PTY

PTY 服务接口。

```typescript
interface PTY {
  create(options: PTYOptions): Promise<PTYHandle>
  resize(ptyId: string, cols: number, rows: number): Promise<void>
  kill(ptyId: string): Promise<void>
}
```

**Python 定义**:

```python
class PTY(Protocol):
    async def create(self, options: PTYOptions) -> PTYHandle: ...
    async def resize(self, pty_id: str, cols: int, rows: int) -> None: ...
    async def kill(self, pty_id: str) -> None: ...
```

### 2.2 PTYOptions

创建 PTY 的选项。

```typescript
interface PTYOptions {
  /**
   * 终端列数 (宽度)
   * @required
   * @example 80
   */
  cols: number

  /**
   * 终端行数 (高度)
   * @required
   * @example 24
   */
  rows: number

  /**
   * 数据回调
   * 接收终端输出的回调函数
   * @required
   */
  onData: (data: Uint8Array) => void
}
```

**Python 定义**:

```python
@dataclass
class PTYOptions:
    cols: int
    """终端列数"""

    rows: int
    """终端行数"""

    on_data: Callable[[bytes], None]
    """数据回调"""
```

### 2.3 PTYHandle

PTY 会话句柄。

```typescript
interface PTYHandle {
  /**
   * PTY 会话 ID
   */
  readonly id: string

  /**
   * 写入数据到终端
   * @param data 要写入的数据
   */
  write(data: Uint8Array): Promise<void>

  /**
   * 关闭 PTY 会话
   */
  kill(): Promise<void>
}
```

**Python 定义**:

```python
class PTYHandle(Protocol):
    id: str
    """PTY 会话 ID"""

    async def write(self, data: bytes) -> None:
        """写入数据到终端"""
        ...

    async def kill(self) -> None:
        """关闭 PTY 会话"""
        ...
```

---

## 3. 方法详情

### 3.1 create

创建新的 PTY 会话。

**签名**:

```typescript
create(options: PTYOptions): Promise<PTYHandle>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `options` | `PTYOptions` | 是 | 创建选项 |
| `options.cols` | `number` | 是 | 终端列数 |
| `options.rows` | `number` | 是 | 终端行数 |
| `options.onData` | `(data: Uint8Array) => void` | 是 | 数据回调 |

**返回值**:

| 类型 | 描述 |
|-----|------|
| `Promise<PTYHandle>` | PTY 会话句柄 |

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 4102 | `PTY_LIMIT_EXCEEDED` | 超过 PTY 数量限制 |
| 4103 | `PTY_CONNECTION_FAILED` | PTY 连接失败 |

**示例**:

```typescript
// TypeScript
const pty = await sandbox.pty.create({
  cols: 80,
  rows: 24,
  onData: (data) => {
    // 解码并打印输出
    const text = new TextDecoder().decode(data)
    process.stdout.write(text)
  }
})

console.log(`PTY created: ${pty.id}`)
```

```python
# Python
async def on_data(data: bytes):
    print(data.decode("utf-8"), end="", flush=True)

pty = await sandbox.pty.create(PTYOptions(
    cols=80,
    rows=24,
    on_data=on_data
))

print(f"PTY created: {pty.id}")
```

---

### 3.2 resize

调整 PTY 终端大小。

**签名**:

```typescript
resize(ptyId: string, cols: number, rows: number): Promise<void>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `ptyId` | `string` | 是 | PTY 会话 ID |
| `cols` | `number` | 是 | 新的列数 |
| `rows` | `number` | 是 | 新的行数 |

**返回值**: 无

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 4100 | `PTY_NOT_FOUND` | PTY 会话不存在 |
| 4105 | `PTY_RESIZE_FAILED` | 调整大小失败 |

**示例**:

```typescript
// TypeScript
await sandbox.pty.resize(pty.id, 120, 40)
```

```python
# Python
await sandbox.pty.resize(pty.id, 120, 40)
```

---

### 3.3 kill

关闭 PTY 会话。

**签名**:

```typescript
kill(ptyId: string): Promise<void>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `ptyId` | `string` | 是 | PTY 会话 ID |

**返回值**: 无

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 4100 | `PTY_NOT_FOUND` | PTY 会话不存在 |

**示例**:

```typescript
// TypeScript
await sandbox.pty.kill(pty.id)
// 或使用 handle
await pty.kill()
```

```python
# Python
await sandbox.pty.kill(pty.id)
# 或使用 handle
await pty.kill()
```

---

### 3.4 PTYHandle.write

向 PTY 写入数据。

**签名**:

```typescript
write(data: Uint8Array): Promise<void>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `data` | `Uint8Array` | 是 | 要写入的数据 |

**返回值**: 无

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 4104 | `PTY_WRITE_FAILED` | 写入失败 |

**示例**:

```typescript
// TypeScript
// 发送命令
const encoder = new TextEncoder()
await pty.write(encoder.encode('ls -la\n'))

// 发送 Ctrl+C
await pty.write(new Uint8Array([0x03]))
```

```python
# Python
# 发送命令
await pty.write(b"ls -la\n")

# 发送 Ctrl+C
await pty.write(bytes([0x03]))
```

---

## 4. REST API / WebSocket

PTY 服务使用 WebSocket 进行实时双向通信。

### 4.1 创建 PTY

```
POST /api/v1/sandboxes/{sandboxId}/pty
```

**请求**:

```json
{
  "cols": 80,
  "rows": 24
}
```

**响应** (201 Created):

```json
{
  "id": "pty-abc123",
  "cols": 80,
  "rows": 24
}
```

### 4.2 WebSocket 连接

```
WSS /api/v1/sandboxes/{sandboxId}/pty/{ptyId}/connect
```

**连接示例**:

```
wss://api.example.com/api/v1/sandboxes/sbx-abc123/pty/pty-xyz789/connect?token=<auth-token>
```

### 4.3 WebSocket 消息格式

#### 客户端 -> 服务器

**输入数据**:
```json
{
  "type": "input",
  "data": "bHMgLWxhCg=="
}
```
- `data`: Base64 编码的输入数据

**调整大小**:
```json
{
  "type": "resize",
  "cols": 120,
  "rows": 40
}
```

#### 服务器 -> 客户端

**输出数据**:
```json
{
  "type": "output",
  "data": "dXNlckBzYW5kYm94Oi9hcHAkIA=="
}
```
- `data`: Base64 编码的输出数据

**退出**:
```json
{
  "type": "exit",
  "exitCode": 0
}
```

### 4.4 调整大小

```
POST /api/v1/sandboxes/{sandboxId}/pty/{ptyId}/resize
```

**请求**:

```json
{
  "cols": 120,
  "rows": 40
}
```

**响应** (204 No Content): 无响应体

### 4.5 关闭 PTY

```
DELETE /api/v1/sandboxes/{sandboxId}/pty/{ptyId}
```

**响应** (204 No Content): 无响应体

---

## 5. 使用示例

### 5.1 基础交互式 Shell

```typescript
// TypeScript
import * as readline from 'readline'

async function interactiveShell(sandbox: Sandbox) {
  const pty = await sandbox.pty.create({
    cols: 80,
    rows: 24,
    onData: (data) => {
      process.stdout.write(new TextDecoder().decode(data))
    }
  })

  // 设置标准输入为原始模式
  process.stdin.setRawMode(true)
  process.stdin.resume()

  // 转发输入到 PTY
  process.stdin.on('data', async (data) => {
    await pty.write(new Uint8Array(data))
  })

  // 处理退出
  process.on('SIGINT', async () => {
    await pty.kill()
    process.exit()
  })
}
```

```python
# Python
import sys
import asyncio

async def interactive_shell(sandbox: Sandbox):
    pty = await sandbox.pty.create(PTYOptions(
        cols=80,
        rows=24,
        on_data=lambda data: sys.stdout.write(data.decode("utf-8"))
    ))

    # 读取输入并发送
    async def read_input():
        while True:
            line = await asyncio.get_event_loop().run_in_executor(
                None, sys.stdin.readline
            )
            if line:
                await pty.write(line.encode("utf-8"))

    try:
        await read_input()
    finally:
        await pty.kill()
```

### 5.2 执行交互式命令

```typescript
// TypeScript
async function runInteractiveCommand(sandbox: Sandbox, command: string): Promise<string> {
  return new Promise(async (resolve, reject) => {
    let output = ''

    const pty = await sandbox.pty.create({
      cols: 80,
      rows: 24,
      onData: (data) => {
        output += new TextDecoder().decode(data)
      }
    })

    // 发送命令
    const encoder = new TextEncoder()
    await pty.write(encoder.encode(command + '\n'))

    // 发送 exit
    await pty.write(encoder.encode('exit\n'))

    // 等待一段时间后返回
    setTimeout(async () => {
      await pty.kill()
      resolve(output)
    }, 1000)
  })
}
```

### 5.3 处理终端大小变化

```typescript
// TypeScript (Node.js)
async function responsivePTY(sandbox: Sandbox) {
  const pty = await sandbox.pty.create({
    cols: process.stdout.columns || 80,
    rows: process.stdout.rows || 24,
    onData: (data) => {
      process.stdout.write(new TextDecoder().decode(data))
    }
  })

  // 监听终端大小变化
  process.stdout.on('resize', async () => {
    await sandbox.pty.resize(
      pty.id,
      process.stdout.columns || 80,
      process.stdout.rows || 24
    )
  })

  return pty
}
```

### 5.4 Web 终端集成 (xterm.js)

```typescript
// Browser TypeScript
import { Terminal } from 'xterm'
import { FitAddon } from 'xterm-addon-fit'

async function createWebTerminal(sandbox: Sandbox, container: HTMLElement) {
  // 创建 xterm 实例
  const term = new Terminal()
  const fitAddon = new FitAddon()
  term.loadAddon(fitAddon)
  term.open(container)
  fitAddon.fit()

  // 创建 PTY
  const pty = await sandbox.pty.create({
    cols: term.cols,
    rows: term.rows,
    onData: (data) => {
      term.write(data)
    }
  })

  // 转发输入
  term.onData(async (data) => {
    await pty.write(new TextEncoder().encode(data))
  })

  // 处理大小变化
  term.onResize(async ({ cols, rows }) => {
    await sandbox.pty.resize(pty.id, cols, rows)
  })

  // 窗口大小变化时自适应
  window.addEventListener('resize', () => {
    fitAddon.fit()
  })

  return { term, pty }
}
```

### 5.5 特殊键处理

```typescript
// TypeScript
const SPECIAL_KEYS = {
  CTRL_C: new Uint8Array([0x03]),      // 中断
  CTRL_D: new Uint8Array([0x04]),      // EOF
  CTRL_Z: new Uint8Array([0x1a]),      // 挂起
  CTRL_L: new Uint8Array([0x0c]),      // 清屏
  TAB: new Uint8Array([0x09]),         // Tab 补全
  ENTER: new Uint8Array([0x0d]),       // 回车
  BACKSPACE: new Uint8Array([0x7f]),   // 退格
  ESCAPE: new Uint8Array([0x1b]),      // ESC
  UP: new Uint8Array([0x1b, 0x5b, 0x41]),    // 上箭头
  DOWN: new Uint8Array([0x1b, 0x5b, 0x42]),  // 下箭头
  RIGHT: new Uint8Array([0x1b, 0x5b, 0x43]), // 右箭头
  LEFT: new Uint8Array([0x1b, 0x5b, 0x44]),  // 左箭头
}

// 发送 Ctrl+C
await pty.write(SPECIAL_KEYS.CTRL_C)
```

---

## 6. 错误处理

### 6.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 4100 | `PTY_NOT_FOUND` | 404 | PTY 会话不存在 |
| 4101 | `PTY_ALREADY_EXISTS` | 409 | PTY 会话已存在 |
| 4102 | `PTY_LIMIT_EXCEEDED` | 429 | 超过 PTY 数量限制 |
| 4103 | `PTY_CONNECTION_FAILED` | 500 | PTY 连接失败 |
| 4104 | `PTY_WRITE_FAILED` | 500 | 写入失败 |
| 4105 | `PTY_RESIZE_FAILED` | 500 | 调整大小失败 |

### 6.2 错误处理示例

```typescript
// TypeScript
import { PTYNotFoundError, PTYLimitExceededError } from '@workspace-sdk/typescript'

try {
  const pty = await sandbox.pty.create({
    cols: 80,
    rows: 24,
    onData: console.log
  })
} catch (error) {
  if (error instanceof PTYLimitExceededError) {
    console.error('Too many PTY sessions, please close some first')
  } else {
    throw error
  }
}
```

```python
# Python
from workspace_sdk.errors import PTYNotFoundError, PTYLimitExceededError

try:
    pty = await sandbox.pty.create(PTYOptions(
        cols=80,
        rows=24,
        on_data=print
    ))
except PTYLimitExceededError:
    print("Too many PTY sessions, please close some first")
```

### 6.3 连接断开处理

```typescript
// TypeScript
async function createRobustPTY(sandbox: Sandbox) {
  let pty: PTYHandle | null = null

  const connect = async () => {
    pty = await sandbox.pty.create({
      cols: 80,
      rows: 24,
      onData: (data) => {
        process.stdout.write(new TextDecoder().decode(data))
      }
    })
  }

  // 初始连接
  await connect()

  // 返回包装后的 handle
  return {
    write: async (data: Uint8Array) => {
      if (!pty) {
        await connect()
      }
      try {
        await pty!.write(data)
      } catch (error) {
        // 重连
        await connect()
        await pty!.write(data)
      }
    },
    kill: async () => {
      if (pty) {
        await pty.kill()
        pty = null
      }
    }
  }
}
```
