# PTY 服务接口文档

PTY (Pseudo Terminal) 服务提供 Sandbox 内的伪终端功能，支持完整的终端模拟，适用于交互式命令行操作。

---

## 目录

- [1. 概述](#1-概述)
- [2. 数据类型](#2-数据类型)
- [3. PTYService 接口](#3-ptyservice-接口)
- [4. REST API](#4-rest-api)
- [5. 使用示例](#5-使用示例)
- [6. 错误处理](#6-错误处理)

---

## 1. 概述

### 1.1 功能说明

PTY 服务与 Process 服务的区别：

| 特性 | Process | PTY |
|------|---------|-----|
| 输出模��� | 分离的 stdout/stderr | 合并的终端输出 |
| 输入模式 | 行缓冲 | 字符级别 |
| 终端控制 | 不支持 | 支持 (ANSI 转义码) |
| 交互支持 | 有限 | 完整 |
| 尺寸控制 | 无 | 支持 cols/rows |
| 典型用途 | 脚本执行 | 交互式 Shell |

### 1.2 架构图

```
┌─────────────────────────────────────────────────────────────┐
│                        PTY Service                           │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   Client                     Sandbox                         │
│  ┌──────┐                   ┌──────────────────────────┐    │
│  │ PTY  │ ◄───WebSocket───► │  /dev/pts/N              │    │
│  │Handle│                   │  ┌────────────────────┐  │    │
│  └──────┘                   │  │      Shell         │  │    │
│     │                       │  │   (bash/zsh/...)   │  │    │
│     │ write()               │  └────────────────────┘  │    │
│     │ resize()              │           │              │    │
│     ▼                       │           ▼              │    │
│  Terminal                   │    子进程 (vim, top)     │    │
│  Emulator                   └──────────────────────────┘    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 数据类型

### 2.1 PTYConfig

PTY 创建配置。

```typescript
/**
 * PTY 创建配置
 */
interface PTYConfig {
  /**
   * 终端列数
   * @minimum 10
   * @maximum 500
   * @default 80
   */
  cols: number

  /**
   * 终端行数
   * @minimum 5
   * @maximum 200
   * @default 24
   */
  rows: number

  /**
   * 工作目录
   * @default Sandbox 默认工作目录
   */
  cwd?: string

  /**
   * 环境变量
   */
  env?: Record<string, string>

  /**
   * 运行用户
   * @default Sandbox 默认用户
   */
  user?: string

  /**
   * Shell 路径
   * @default "/bin/bash"
   */
  shell?: string

  /**
   * Shell 参数
   * @example ["-l"] 登录 Shell
   */
  shellArgs?: string[]

  /**
   * TERM 环境变量
   * @default "xterm-256color"
   */
  term?: string

  /**
   * PTY 超时（毫秒），无活动后自动关闭
   * @default 3600000 (1小时)
   */
  timeoutMs?: number
}
```

```python
from dataclasses import dataclass
from typing import Optional, List, Dict

@dataclass
class PTYConfig:
    """PTY 创建配置"""

    cols: int = 80
    """终端列数"""

    rows: int = 24
    """终端行数"""

    cwd: Optional[str] = None
    """工作目录"""

    env: Optional[Dict[str, str]] = None
    """环境变量"""

    user: Optional[str] = None
    """运行用户"""

    shell: str = "/bin/bash"
    """Shell 路径"""

    shell_args: Optional[List[str]] = None
    """Shell 参数"""

    term: str = "xterm-256color"
    """TERM 环境变量"""

    timeout_ms: int = 3600000
    """超时时间（毫秒）"""
```

### 2.2 PTYInfo

PTY 信息。

```typescript
/**
 * PTY 信息
 */
interface PTYInfo {
  /**
   * PTY ID（进程 ID）
   */
  pid: number

  /**
   * 终端列数
   */
  cols: number

  /**
   * 终端行数
   */
  rows: number

  /**
   * 工作目录
   */
  cwd: string

  /**
   * 运行用户
   */
  user: string

  /**
   * Shell 路径
   */
  shell: string

  /**
   * 创建时间
   */
  createdAt: Date

  /**
   * 最后活动时间
   */
  lastActivityAt: Date

  /**
   * 是否活跃
   */
  active: boolean
}
```

### 2.3 PTYOutput

PTY 输出数据。

```typescript
/**
 * PTY 输出数据
 */
interface PTYOutput {
  /**
   * 输出数据（包含 ANSI 转义码）
   */
  data: Uint8Array

  /**
   * 时间戳
   */
  timestamp: Date
}
```

### 2.4 PTYHandle

PTY 句柄。

```typescript
/**
 * PTY 句柄
 */
interface PTYHandle {
  /**
   * PTY ID（进程 ID）
   */
  readonly pid: number

  /**
   * 当前列数
   */
  readonly cols: number

  /**
   * 当前行数
   */
  readonly rows: number

  /**
   * 是否活跃
   */
  readonly active: boolean

  /**
   * 写入数据到 PTY
   *
   * @param data - 输入数据（字符串或字节数组）
   *
   * @example
   * // 发送命令
   * await pty.write('ls -la\r')
   *
   * @example
   * // 发送特殊键
   * await pty.write('\x03') // Ctrl+C
   * await pty.write('\x1b[A') // 上箭头
   */
  write(data: string | Uint8Array): Promise<void>

  /**
   * 调整 PTY 大小
   *
   * @param cols - 新的列数
   * @param rows - 新的行数
   *
   * @example
   * await pty.resize(120, 40)
   */
  resize(cols: number, rows: number): Promise<void>

  /**
   * 关闭 PTY
   *
   * @example
   * await pty.kill()
   */
  kill(): Promise<void>

  /**
   * 注册数据输出回调
   *
   * @param callback - 输出回调函数
   *
   * @example
   * pty.onData((output) => {
   *   terminal.write(output.data)
   * })
   */
  onData(callback: (output: PTYOutput) => void): void

  /**
   * 注册退出回调
   *
   * @param callback - 退出回调函数
   */
  onExit(callback: (exitCode: number) => void): void

  /**
   * 等待 PTY 退出
   *
   * @returns 退出码
   */
  wait(): Promise<number>
}
```

```python
class PTYHandle:
    """PTY 句柄"""

    @property
    def pid(self) -> int:
        """PTY ID"""
        ...

    @property
    def cols(self) -> int:
        """当前列数"""
        ...

    @property
    def rows(self) -> int:
        """当前行数"""
        ...

    @property
    def active(self) -> bool:
        """是否活跃"""
        ...

    def write(self, data: Union[str, bytes]) -> None:
        """写入数据"""
        ...

    def resize(self, cols: int, rows: int) -> None:
        """调整大小"""
        ...

    def kill(self) -> None:
        """关闭 PTY"""
        ...

    def wait(self) -> int:
        """等待退出"""
        ...

    def __iter__(self):
        """迭代输出数据"""
        ...
```

---

## 3. PTYService 接口

### 3.1 create

创建新的 PTY。

```typescript
/**
 * 创建新的 PTY
 *
 * @param config - PTY 配置
 * @param options - 请求选项
 * @returns PTY 句柄
 *
 * @example
 * // 创建默认 PTY
 * const pty = await sandbox.pty.create({
 *   cols: 80,
 *   rows: 24
 * })
 *
 * // 处理输出
 * pty.onData((output) => {
 *   process.stdout.write(output.data)
 * })
 *
 * // 发送命令
 * await pty.write('ls -la\r')
 *
 * @example
 * // 创建带配置的 PTY
 * const pty = await sandbox.pty.create({
 *   cols: 120,
 *   rows: 40,
 *   cwd: '/app',
 *   shell: '/bin/zsh',
 *   env: { EDITOR: 'vim' }
 * })
 */
create(config: PTYConfig, options?: RequestOptions): Promise<PTYHandle>
```

```python
def create(
    self,
    cols: int = 80,
    rows: int = 24,
    *,
    cwd: Optional[str] = None,
    env: Optional[Dict[str, str]] = None,
    user: Optional[str] = None,
    shell: str = "/bin/bash",
    shell_args: Optional[List[str]] = None,
    term: str = "xterm-256color",
    timeout_ms: int = 3600000
) -> PTYHandle:
    """
    创建新的 PTY

    Args:
        cols: 终端列数
        rows: 终端行数
        cwd: 工作目录
        env: 环境变量
        user: 运行用户
        shell: Shell 路径
        shell_args: Shell 参数
        term: TERM 环境变量
        timeout_ms: 超时时间

    Returns:
        PTY 句柄

    Example:
        >>> pty = sandbox.pty.create(cols=120, rows=40)
        >>> pty.write('ls -la\\r')
        >>> for output in pty:
        ...     print(output.data.decode(), end='')
    """
```

### 3.2 connect

连接到现有 PTY。

```typescript
/**
 * 连接到现有 PTY
 *
 * @param pid - PTY ID（进程 ID）
 * @param options - 连接选项
 * @returns PTY 句柄
 * @throws {PTYNotFoundError} PTY 不存在
 *
 * @example
 * // 重新连接到 PTY
 * const pty = await sandbox.pty.connect(1234)
 */
connect(
  pid: number,
  options?: {
    /**
     * 数据回调
     */
    onData?: (output: PTYOutput) => void

    /**
     * 是否从头获取历史输出
     * @default false
     */
    includeHistory?: boolean
  }
): Promise<PTYHandle>
```

### 3.3 list

列出所有 PTY。

```typescript
/**
 * 列出所有 PTY
 *
 * @param options - 请求选项
 * @returns PTY 信息列表
 *
 * @example
 * const ptys = await sandbox.pty.list()
 * for (const pty of ptys) {
 *   console.log(`PTY ${pty.pid}: ${pty.cols}x${pty.rows}`)
 * }
 */
list(options?: RequestOptions): Promise<PTYInfo[]>
```

### 3.4 kill

关闭 PTY。

```typescript
/**
 * 关闭 PTY
 *
 * @param pid - PTY ID
 * @param options - 请求选项
 * @throws {PTYNotFoundError} PTY 不存在
 *
 * @example
 * await sandbox.pty.kill(1234)
 */
kill(pid: number, options?: RequestOptions): Promise<void>
```

### 3.5 resize

调整 PTY 大小。

```typescript
/**
 * 调整 PTY 大小
 *
 * @param pid - PTY ID
 * @param cols - 新的列数
 * @param rows - 新的行数
 * @param options - 请求选项
 * @throws {PTYNotFoundError} PTY 不存在
 *
 * @example
 * await sandbox.pty.resize(1234, 120, 40)
 */
resize(pid: number, cols: number, rows: number, options?: RequestOptions): Promise<void>
```

### 3.6 write

写入数据到 PTY。

```typescript
/**
 * 写入数据到 PTY
 *
 * @param pid - PTY ID
 * @param data - 输入数据
 * @param options - 请求选项
 * @throws {PTYNotFoundError} PTY 不存在
 *
 * @example
 * await sandbox.pty.write(1234, 'echo hello\r')
 */
write(pid: number, data: string | Uint8Array, options?: RequestOptions): Promise<void>
```

---

## 4. REST API

### 4.1 端点列表

| 方法 | 端点 | 描述 |
|------|------|------|
| POST | `/api/v1/sandboxes/{id}/pty` | 创建 PTY |
| GET | `/api/v1/sandboxes/{id}/pty` | 列出 PTY |
| GET | `/api/v1/sandboxes/{id}/pty/{pid}` | 获取 PTY 信息 |
| DELETE | `/api/v1/sandboxes/{id}/pty/{pid}` | 关闭 PTY |
| POST | `/api/v1/sandboxes/{id}/pty/{pid}/resize` | 调整大小 |
| POST | `/api/v1/sandboxes/{id}/pty/{pid}/write` | 写入数据 |
| WS | `/api/v1/sandboxes/{id}/pty/{pid}/connect` | 连接 PTY |

### 4.2 创建 PTY

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/pty
Content-Type: application/json
Authorization: Bearer <token>

{
  "cols": 80,
  "rows": 24,
  "cwd": "/app",
  "shell": "/bin/bash",
  "env": {
    "TERM": "xterm-256color"
  }
}
```

**响应** (201 Created):

```json
{
  "pid": 1234,
  "cols": 80,
  "rows": 24,
  "cwd": "/app",
  "user": "user",
  "shell": "/bin/bash",
  "createdAt": "2024-01-15T10:30:00Z",
  "active": true
}
```

### 4.3 连接 PTY (WebSocket)

**连接**:

```
WS /api/v1/sandboxes/sbx-abc123/pty/1234/connect
```

**服务器消息** (输出数据):

```json
{
  "type": "data",
  "data": "dXNlckBzYW5kYm94Oi9hcHAkIA==",  // base64 编码
  "timestamp": "2024-01-15T10:30:00Z"
}
```

**服务器消息** (退出):

```json
{
  "type": "exit",
  "exitCode": 0
}
```

**客户端消息** (输入):

```json
{
  "type": "input",
  "data": "bHMgLWxhDQ=="  // base64: "ls -la\r"
}
```

**客户端消息** (调整大小):

```json
{
  "type": "resize",
  "cols": 120,
  "rows": 40
}
```

### 4.4 调整大小

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/pty/1234/resize
Content-Type: application/json
Authorization: Bearer <token>

{
  "cols": 120,
  "rows": 40
}
```

**响应** (200 OK):

```json
{
  "pid": 1234,
  "cols": 120,
  "rows": 40
}
```

---

## 5. 使用示例

### 5.1 TypeScript 示例

```typescript
import { WorkspaceClient } from '@workspace-sdk/typescript'

async function ptyExample() {
  const client = new WorkspaceClient({ apiUrl: 'https://api.example.com' })
  const sandbox = await client.sandbox.create({ template: 'ubuntu:22.04' })

  try {
    // ========== 创建 PTY ==========
    console.log('Creating PTY...')

    const pty = await sandbox.pty.create({
      cols: 80,
      rows: 24,
      cwd: '/home/user'
    })

    console.log(`PTY created with PID: ${pty.pid}`)

    // 收集输出
    let output = ''
    pty.onData((data) => {
      const text = new TextDecoder().decode(data.data)
      output += text
      process.stdout.write(text)
    })

    // 等待 Shell 提示符
    await new Promise(resolve => setTimeout(resolve, 500))

    // ========== 执行命令 ==========
    console.log('\nExecuting commands...')

    // 发送命令
    await pty.write('echo "Hello from PTY"\r')
    await new Promise(resolve => setTimeout(resolve, 100))

    await pty.write('pwd\r')
    await new Promise(resolve => setTimeout(resolve, 100))

    await pty.write('ls -la\r')
    await new Promise(resolve => setTimeout(resolve, 200))

    // ========== 调整大小 ==========
    console.log('\nResizing PTY...')
    await pty.resize(120, 40)

    // ========== 使用 vim ==========
    console.log('\nOpening vim...')

    await pty.write('vim test.txt\r')
    await new Promise(resolve => setTimeout(resolve, 500))

    // 进入插入模式并输入
    await pty.write('i')
    await pty.write('Hello, World!')

    // 保存并退出
    await pty.write('\x1b') // ESC
    await pty.write(':wq\r')
    await new Promise(resolve => setTimeout(resolve, 200))

    // 验证文件
    await pty.write('cat test.txt\r')
    await new Promise(resolve => setTimeout(resolve, 100))

    // ========== 退出 ==========
    console.log('\nExiting...')
    await pty.write('exit\r')

    const exitCode = await pty.wait()
    console.log(`PTY exited with code: ${exitCode}`)

  } finally {
    await sandbox.delete()
  }
}

ptyExample().catch(console.error)
```

### 5.2 Python 示例

```python
import time
from workspace_sdk import WorkspaceClient

def pty_example():
    client = WorkspaceClient()

    with client.sandbox.create(template='ubuntu:22.04') as sandbox:
        # 创建 PTY
        print('Creating PTY...')
        pty = sandbox.pty.create(cols=80, rows=24, cwd='/home/user')
        print(f'PTY created with PID: {pty.pid}')

        # 等待 Shell 就绪
        time.sleep(0.5)

        # 执行命令并收集输出
        print('\nExecuting commands...')

        pty.write('echo "Hello from PTY"\r')
        time.sleep(0.1)

        pty.write('pwd\r')
        time.sleep(0.1)

        pty.write('ls -la\r')
        time.sleep(0.2)

        # 读取输出
        for output in pty:
            print(output.data.decode('utf-8'), end='')
            if b'$' in output.data:  # 检测到提示符
                break

        # 调整大小
        print('\nResizing PTY...')
        pty.resize(120, 40)

        # 退出
        print('\nExiting...')
        pty.write('exit\r')

        exit_code = pty.wait()
        print(f'PTY exited with code: {exit_code}')

if __name__ == '__main__':
    pty_example()
```

### 5.3 Web 终端集成示例

```typescript
import { Terminal } from 'xterm'
import { FitAddon } from 'xterm-addon-fit'
import { WorkspaceClient } from '@workspace-sdk/typescript'

async function webTerminal(container: HTMLElement) {
  // 创建 xterm.js 终端
  const terminal = new Terminal({
    cursorBlink: true,
    fontSize: 14,
    fontFamily: 'Menlo, Monaco, "Courier New", monospace',
    theme: {
      background: '#1e1e1e',
      foreground: '#d4d4d4'
    }
  })

  const fitAddon = new FitAddon()
  terminal.loadAddon(fitAddon)
  terminal.open(container)
  fitAddon.fit()

  // 创建 SDK 客户端
  const client = new WorkspaceClient({ apiUrl: 'https://api.example.com' })
  const sandbox = await client.sandbox.create({ template: 'ubuntu:22.04' })

  // 创建 PTY
  const pty = await sandbox.pty.create({
    cols: terminal.cols,
    rows: terminal.rows
  })

  // 连接 PTY 输出到终端
  pty.onData((output) => {
    terminal.write(output.data)
  })

  // 连接终端输入到 PTY
  terminal.onData((data) => {
    pty.write(data)
  })

  // 处理终端大小变化
  const resizeObserver = new ResizeObserver(() => {
    fitAddon.fit()
    pty.resize(terminal.cols, terminal.rows)
  })
  resizeObserver.observe(container)

  // 处理 PTY 退出
  pty.onExit((exitCode) => {
    terminal.write(`\r\n[Process exited with code ${exitCode}]\r\n`)
    terminal.dispose()
    sandbox.delete()
  })

  // 清理函数
  return () => {
    resizeObserver.disconnect()
    pty.kill()
    terminal.dispose()
    sandbox.delete()
  }
}
```

### 5.4 特殊键发送参考

```typescript
// 常用特殊键
const SpecialKeys = {
  // 控制键
  CTRL_A: '\x01',
  CTRL_B: '\x02',
  CTRL_C: '\x03',  // 中断
  CTRL_D: '\x04',  // EOF
  CTRL_E: '\x05',
  CTRL_F: '\x06',
  CTRL_G: '\x07',
  CTRL_H: '\x08',  // Backspace
  CTRL_I: '\x09',  // Tab
  CTRL_J: '\x0A',  // Enter (LF)
  CTRL_K: '\x0B',
  CTRL_L: '\x0C',  // 清屏
  CTRL_M: '\x0D',  // Enter (CR)
  CTRL_N: '\x0E',
  CTRL_O: '\x0F',
  CTRL_P: '\x10',
  CTRL_Q: '\x11',
  CTRL_R: '\x12',  // 反向搜索
  CTRL_S: '\x13',
  CTRL_T: '\x14',
  CTRL_U: '\x15',  // 清除行
  CTRL_V: '\x16',
  CTRL_W: '\x17',  // 删除单词
  CTRL_X: '\x18',
  CTRL_Y: '\x19',
  CTRL_Z: '\x1A',  // 挂起

  // 转义序列
  ESC: '\x1b',

  // 箭头键
  UP: '\x1b[A',
  DOWN: '\x1b[B',
  RIGHT: '\x1b[C',
  LEFT: '\x1b[D',

  // 功能键
  HOME: '\x1b[H',
  END: '\x1b[F',
  INSERT: '\x1b[2~',
  DELETE: '\x1b[3~',
  PAGE_UP: '\x1b[5~',
  PAGE_DOWN: '\x1b[6~',

  // F 键
  F1: '\x1bOP',
  F2: '\x1bOQ',
  F3: '\x1bOR',
  F4: '\x1bOS',
  F5: '\x1b[15~',
  F6: '\x1b[17~',
  F7: '\x1b[18~',
  F8: '\x1b[19~',
  F9: '\x1b[20~',
  F10: '\x1b[21~',
  F11: '\x1b[23~',
  F12: '\x1b[24~',

  // 换行
  ENTER: '\r',
  TAB: '\t',
  BACKSPACE: '\x7f'
}

// 使用示例
await pty.write(SpecialKeys.CTRL_C)  // 发送 Ctrl+C
await pty.write(SpecialKeys.UP)       // 发送上箭头
await pty.write('hello' + SpecialKeys.ENTER)  // 输入并回车
```

---

## 6. 错误处理

### 6.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 4100 | PTY_NOT_FOUND | 404 | PTY 不存在 |
| 4101 | PTY_ALREADY_EXISTS | 409 | PTY 已存在 |
| 4102 | PTY_LIMIT_EXCEEDED | 429 | 超过 PTY 数量限制 |
| 4103 | PTY_CONNECTION_FAILED | 500 | PTY 连接失败 |
| 4104 | PTY_WRITE_FAILED | 500 | PTY 写入失败 |
| 4105 | PTY_RESIZE_FAILED | 500 | PTY 调整大小失败 |

### 6.2 错误处理示例

```typescript
import {
  PTYNotFoundError,
  PTYConnectionFailedError
} from '@workspace-sdk/typescript'

async function connectWithRetry(sandbox: Sandbox, pid: number, retries: number = 3) {
  for (let i = 0; i < retries; i++) {
    try {
      return await sandbox.pty.connect(pid)
    } catch (error) {
      if (error instanceof PTYNotFoundError) {
        console.log(`PTY ${pid} not found`)
        throw error
      }

      if (error instanceof PTYConnectionFailedError) {
        console.log(`Connection failed, attempt ${i + 1}/${retries}`)
        await new Promise(resolve => setTimeout(resolve, 1000))
        continue
      }

      throw error
    }
  }

  throw new Error(`Failed to connect to PTY after ${retries} retries`)
}
```

---

## 附录

### A. TERM 环境变量参考

| 值 | 描述 |
|---|------|
| `xterm` | 基本 xterm 终端 |
| `xterm-256color` | 支持 256 色 (推荐) |
| `xterm-16color` | 支持 16 色 |
| `screen` | GNU Screen |
| `screen-256color` | GNU Screen 256 色 |
| `vt100` | VT100 兼容 |
| `dumb` | 无终端功能 |

### B. 常见 ANSI 转义码

| 序列 | 描述 |
|-----|------|
| `\x1b[0m` | 重置样式 |
| `\x1b[1m` | 粗体 |
| `\x1b[4m` | 下划线 |
| `\x1b[31m` | 红色前景 |
| `\x1b[32m` | 绿色前景 |
| `\x1b[33m` | 黄色前景 |
| `\x1b[34m` | 蓝色前景 |
| `\x1b[41m` | 红色背景 |
| `\x1b[2J` | 清屏 |
| `\x1b[H` | 移动到左上角 |
| `\x1b[K` | 清除到行尾 |

### C. 资源限制

| 资源 | 默认限制 | 说明 |
|------|---------|------|
| 最大 PTY 数 | 10 | 每个 Sandbox |
| PTY 超时 | 1 小时 | 无活动后自动关闭 |
| 最大列数 | 500 | - |
| 最大行数 | 200 | - |
| 输出缓冲区 | 1 MB | 历史输出 |
