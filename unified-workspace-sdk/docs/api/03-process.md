# Process 服务接口文档

Process 服务提供 Sandbox 内的进程执行和管理能力，包括命令执行、代码运行、会话管理等功能。

---

## 目录

- [1. 概述](#1-概述)
- [2. 数据类型](#2-数据类型)
- [3. ProcessService 接口](#3-processservice-接口)
- [4. REST API](#4-rest-api)
- [5. 使用示例](#5-使用示例)
- [6. 错误处理](#6-错误处理)

---

## 1. 概述

### 1.1 功能说明

| 功能类别 | 功能 | 描述 |
|---------|------|------|
| **命令执行** | run | 执行命令并等待完成 |
| | spawn | 启动后台命令 |
| **进程管理** | list | 列出运行中的进程 |
| | kill | 终止进程 |
| | sendStdin | 发送标准输入 |
| **代码执行** | runCode | 执行代码片段 |
| **会话管理** | createSession | 创建持久会话 |
| | executeInSession | 在会话中执行命令 |

### 1.2 执行模型

```
┌─────────────────────────────────────────────────────────────┐
│                      ProcessService                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    同步执行 (run)                        ││
│  │  Command ──► Shell ──► stdout/stderr ──► Result         ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    后台执行 (spawn)                      ││
│  │  Command ──► Shell ──► Handle ──► wait()/kill()         ││
│  │                          ↓                               ││
│  │                    onStdout/onStderr                     ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    会话执行 (Session)                    ││
│  │  Session ──► Commands ──► 环境持久化                     ││
│  │     ↓                                                    ││
│  │  环境变量、工作目录、进程状态保持                         ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 数据类型

### 2.1 CommandResult

命令执行结果。

```typescript
/**
 * 命令执行结果
 */
interface CommandResult {
  /**
   * 退出码
   * - 0: 成功
   * - 非0: 失败
   * - -1: 被信号终止
   */
  exitCode: number

  /**
   * 标准输出内容
   */
  stdout: string

  /**
   * 标准错误内容
   */
  stderr: string

  /**
   * 错误信息（如果执行失败）
   */
  error?: string

  /**
   * 终止信号（如果被信号终止）
   * @example "SIGTERM"
   */
  signal?: string

  /**
   * 执行耗时（毫秒）
   */
  durationMs: number

  /**
   * 是否超时
   */
  timedOut: boolean
}
```

```python
from dataclasses import dataclass
from typing import Optional

@dataclass
class CommandResult:
    """命令执行结果"""

    exit_code: int
    """退出码"""

    stdout: str
    """标准输出"""

    stderr: str
    """标准错误"""

    error: Optional[str] = None
    """错误信息"""

    signal: Optional[str] = None
    """终止信号"""

    duration_ms: int = 0
    """执行耗时（毫秒）"""

    timed_out: bool = False
    """是否超时"""
```

### 2.2 CommandOptions

命令执行选项。

```typescript
/**
 * 命令执行选项
 */
interface CommandOptions {
  /**
   * 工作目录
   * @default Sandbox 的默认工作目录
   */
  cwd?: string

  /**
   * 环境变量（合并到现有环境）
   */
  env?: Record<string, string>

  /**
   * 是否替换全部环境变量（而非合并）
   * @default false
   */
  replaceEnv?: boolean

  /**
   * 运行用户
   * @default Sandbox 的默认用户
   */
  user?: string

  /**
   * 执行超时（毫秒）
   * @default 60000 (1分钟)
   */
  timeoutMs?: number

  /**
   * 使用的 Shell
   * @default "/bin/bash"
   */
  shell?: string

  /**
   * 是否使用 Shell 执行（对 spawn 有效）
   * @default true
   */
  useShell?: boolean

  /**
   * 标准输出回调（实时）
   */
  onStdout?: (data: string) => void | Promise<void>

  /**
   * 标准错误回调（实时）
   */
  onStderr?: (data: string) => void | Promise<void>

  /**
   * 是否保持 stdin 打开（用于交互式命令）
   * @default false
   */
  stdin?: boolean

  /**
   * 输出编码
   * @default 'utf-8'
   */
  encoding?: string

  /**
   * 最大输出缓冲区大小（字节）
   * 超出后截断
   * @default 10485760 (10MB)
   */
  maxBuffer?: number
}
```

```python
from dataclasses import dataclass
from typing import Optional, Dict, Callable

@dataclass
class CommandOptions:
    """命令执行选项"""

    cwd: Optional[str] = None
    """工作目录"""

    env: Optional[Dict[str, str]] = None
    """环境变量"""

    replace_env: bool = False
    """是否替换全部环境变量"""

    user: Optional[str] = None
    """运行用户"""

    timeout_ms: int = 60000
    """执行超时（毫秒）"""

    shell: str = "/bin/bash"
    """使用的 Shell"""

    use_shell: bool = True
    """是否使用 Shell 执行"""

    on_stdout: Optional[Callable[[str], None]] = None
    """标准输出回调"""

    on_stderr: Optional[Callable[[str], None]] = None
    """标准错误回调"""

    stdin: bool = False
    """是否保持 stdin 打开"""

    encoding: str = "utf-8"
    """输出编码"""

    max_buffer: int = 10485760
    """最大输出缓冲区大小"""
```

### 2.3 ProcessInfo

进程信息。

```typescript
/**
 * 进程信息
 */
interface ProcessInfo {
  /**
   * 进程 ID
   */
  pid: number

  /**
   * 父进程 ID
   */
  ppid: number

  /**
   * 执行的命令
   */
  command: string

  /**
   * 命令参数
   */
  args: string[]

  /**
   * 工作目录
   */
  cwd: string

  /**
   * 环境变量
   */
  env: Record<string, string>

  /**
   * 运行用户
   */
  user: string

  /**
   * 启动时间
   */
  startedAt: Date

  /**
   * CPU 使用率 (%)
   */
  cpuPercent: number

  /**
   * 内存使用（字节）
   */
  memoryBytes: number

  /**
   * 进程状态
   */
  state: ProcessState

  /**
   * 标签（用于标识特殊进程）
   */
  tag?: string
}

/**
 * 进程状态
 */
enum ProcessState {
  /** 运行中 */
  RUNNING = 'running',
  /** 睡眠中 */
  SLEEPING = 'sleeping',
  /** 停止 */
  STOPPED = 'stopped',
  /** 僵尸进程 */
  ZOMBIE = 'zombie'
}
```

### 2.4 CommandHandle

命令句柄（用于后台命令）。

```typescript
/**
 * 命令句柄
 */
interface CommandHandle {
  /**
   * 进程 ID
   */
  readonly pid: number

  /**
   * 退出码（运行中为 undefined）
   */
  readonly exitCode?: number

  /**
   * 累积的标准输出
   */
  readonly stdout: string

  /**
   * 累积的标准错误
   */
  readonly stderr: string

  /**
   * 是否正在运行
   */
  readonly running: boolean

  /**
   * 等待命令完成
   * @returns 命令结果
   */
  wait(): Promise<CommandResult>

  /**
   * 终止命令
   * @param signal - 信号名称，默认 SIGTERM
   * @returns 是否成功终止
   */
  kill(signal?: string): Promise<boolean>

  /**
   * 发送数据到标准输入
   * @param data - 输入数据
   */
  sendStdin(data: string): Promise<void>

  /**
   * 断开连接（不终止进程）
   */
  disconnect(): Promise<void>

  /**
   * 注册标准输出回调
   */
  onStdout(callback: (data: string) => void): void

  /**
   * 注册标准错误回调
   */
  onStderr(callback: (data: string) => void): void

  /**
   * 注册退出回调
   */
  onExit(callback: (result: CommandResult) => void): void
}
```

```python
class CommandHandle:
    """命令句柄"""

    @property
    def pid(self) -> int:
        """进程 ID"""
        ...

    @property
    def exit_code(self) -> Optional[int]:
        """退出码"""
        ...

    @property
    def stdout(self) -> str:
        """累积的标准输出"""
        ...

    @property
    def stderr(self) -> str:
        """累积的标准错误"""
        ...

    @property
    def running(self) -> bool:
        """是否正在运行"""
        ...

    def wait(self) -> CommandResult:
        """等待命令完成"""
        ...

    def kill(self, signal: str = "SIGTERM") -> bool:
        """终止命令"""
        ...

    def send_stdin(self, data: str) -> None:
        """发送标准输入"""
        ...

    def disconnect(self) -> None:
        """断开连接"""
        ...

    def __iter__(self):
        """迭代输出流"""
        ...
```

### 2.5 Session

会话信息。

```typescript
/**
 * 执行会话
 */
interface Session {
  /**
   * 会话 ID
   */
  id: string

  /**
   * 创建时间
   */
  createdAt: Date

  /**
   * 最后活动时间
   */
  lastActivityAt: Date

  /**
   * 当前工作目录
   */
  cwd: string

  /**
   * 环境变量
   */
  env: Record<string, string>

  /**
   * 运行用户
   */
  user: string

  /**
   * 执行的命令历史
   */
  commandHistory: string[]

  /**
   * 是否活跃
   */
  active: boolean
}
```

### 2.6 CodeRunParams

代码执行参数。

```typescript
/**
 * 代码执行参数
 */
interface CodeRunParams {
  /**
   * 代码内容
   */
  code: string

  /**
   * 编程语言
   */
  language: CodeLanguage

  /**
   * 命令行参数
   */
  args?: string[]

  /**
   * 环境变量
   */
  env?: Record<string, string>

  /**
   * 执行超时（毫秒）
   * @default 30000
   */
  timeoutMs?: number

  /**
   * 工作目录
   */
  cwd?: string

  /**
   * 输入数据（通过 stdin）
   */
  input?: string

  /**
   * 是否捕获图表输出（Python matplotlib）
   */
  captureCharts?: boolean
}

/**
 * 代码执行结果
 */
interface CodeRunResult extends CommandResult {
  /**
   * 图表数据（如果 captureCharts=true）
   */
  charts?: Chart[]

  /**
   * 变量状态（用于 REPL）
   */
  variables?: Record<string, unknown>
}

/**
 * 图表数据
 */
interface Chart {
  /**
   * 图表类型
   */
  type: 'image' | 'html' | 'json'

  /**
   * 图表数据（base64 或 JSON）
   */
  data: string

  /**
   * MIME 类型
   */
  mimeType: string
}
```

---

## 3. ProcessService 接口

### 3.1 run

执行命令并等待完成。

```typescript
/**
 * 执行命令并等待完成
 *
 * @param command - 要执行的命令
 * @param options - 执行选项
 * @returns 命令执行结果
 * @throws {CommandTimeoutError} 命令执行超时
 * @throws {CommandError} 命令执行失败（exitCode != 0）
 *
 * @example
 * // 简单命令
 * const result = await sandbox.process.run('echo "Hello, World!"')
 * console.log(result.stdout) // "Hello, World!\n"
 *
 * @example
 * // 指定工作目录和环境变量
 * const result = await sandbox.process.run('npm install', {
 *   cwd: '/app',
 *   env: { NODE_ENV: 'production' },
 *   timeoutMs: 300000
 * })
 *
 * @example
 * // 实时输出
 * const result = await sandbox.process.run('npm test', {
 *   onStdout: (data) => process.stdout.write(data),
 *   onStderr: (data) => process.stderr.write(data)
 * })
 */
run(command: string, options?: CommandOptions): Promise<CommandResult>
```

```python
def run(
    self,
    command: str,
    *,
    cwd: Optional[str] = None,
    env: Optional[Dict[str, str]] = None,
    user: Optional[str] = None,
    timeout_ms: int = 60000,
    on_stdout: Optional[Callable[[str], None]] = None,
    on_stderr: Optional[Callable[[str], None]] = None,
    encoding: str = "utf-8"
) -> CommandResult:
    """
    执行命令并等待完成

    Args:
        command: 要执行的命令
        cwd: 工作目录
        env: 环境变量
        user: 运行用户
        timeout_ms: 超时时间（毫秒）
        on_stdout: 标准输出回调
        on_stderr: 标准错误回调
        encoding: 输出编码

    Returns:
        命令执行结果

    Raises:
        CommandTimeoutError: 命令执行超时
        CommandError: 命令执行失败

    Example:
        >>> result = sandbox.process.run('python --version')
        >>> print(result.stdout)  # "Python 3.11.0\n"
    """
```

### 3.2 spawn

启动后台命令。

```typescript
/**
 * 启动后台命令
 *
 * 命令在后台运行，返回句柄用于控制和监视。
 *
 * @param command - 要执行的命令
 * @param options - 执行选项
 * @returns 命令句柄
 *
 * @example
 * // 启动后台服务
 * const handle = await sandbox.process.spawn('python -m http.server 8080', {
 *   cwd: '/app/static'
 * })
 *
 * // 等待服务就绪
 * await new Promise(resolve => setTimeout(resolve, 1000))
 *
 * // 后续操作...
 *
 * // 停止服务
 * await handle.kill()
 *
 * @example
 * // 交互式命令
 * const handle = await sandbox.process.spawn('python', {
 *   stdin: true
 * })
 *
 * await handle.sendStdin('print("Hello")\n')
 * await handle.sendStdin('exit()\n')
 *
 * const result = await handle.wait()
 *
 * @example
 * // 实时监听输出
 * const handle = await sandbox.process.spawn('tail -f /var/log/app.log')
 *
 * handle.onStdout((data) => {
 *   console.log('Log:', data)
 * })
 *
 * // 10秒后停止
 * setTimeout(() => handle.kill(), 10000)
 */
spawn(command: string, options?: CommandOptions): Promise<CommandHandle>
```

```python
def spawn(
    self,
    command: str,
    *,
    cwd: Optional[str] = None,
    env: Optional[Dict[str, str]] = None,
    user: Optional[str] = None,
    timeout_ms: Optional[int] = None,
    stdin: bool = False
) -> CommandHandle:
    """
    启动后台命令

    Args:
        command: 要执行的命令
        cwd: 工作目录
        env: 环境变量
        user: 运行用户
        timeout_ms: 超时时间（None 表示不超时）
        stdin: 是否保持 stdin 打开

    Returns:
        命令句柄

    Example:
        >>> handle = sandbox.process.spawn('python -m http.server 8080')
        >>> # ... 执行其他操作
        >>> handle.kill()
    """
```

### 3.3 list

列出运行中的进程。

```typescript
/**
 * 列出运行中的进程
 *
 * @param options - 选项
 * @returns 进程信息列表
 *
 * @example
 * const processes = await sandbox.process.list()
 * for (const proc of processes) {
 *   console.log(`PID ${proc.pid}: ${proc.command} (${proc.cpuPercent}% CPU)`)
 * }
 *
 * @example
 * // 只列出用户进程
 * const processes = await sandbox.process.list({ user: 'user' })
 */
list(options?: {
  /**
   * 按用户过滤
   */
  user?: string

  /**
   * 按命令过滤（支持通配符）
   */
  command?: string

  /**
   * 是否包含系统进程
   * @default false
   */
  includeSystem?: boolean
}): Promise<ProcessInfo[]>
```

### 3.4 kill

终止进程。

```typescript
/**
 * 终止进程
 *
 * @param pid - 进程 ID
 * @param signal - 信号名称
 * @param options - 选项
 * @returns 是否成功终止
 * @throws {ProcessNotFoundError} 进程不存在
 *
 * @example
 * // 优雅终止
 * await sandbox.process.kill(1234)
 *
 * @example
 * // 强制终止
 * await sandbox.process.kill(1234, 'SIGKILL')
 *
 * @example
 * // 终止并等待
 * await sandbox.process.kill(1234, 'SIGTERM', { wait: true, timeoutMs: 5000 })
 */
kill(
  pid: number,
  signal?: ProcessSignal | string,
  options?: {
    /**
     * 是否等待进程退出
     * @default false
     */
    wait?: boolean

    /**
     * 等待超时（毫秒）
     * @default 5000
     */
    timeoutMs?: number
  }
): Promise<boolean>
```

### 3.5 sendStdin

发送标准输入。

```typescript
/**
 * 发送数据到进程的标准输入
 *
 * @param pid - 进程 ID
 * @param data - 输入数据
 * @throws {ProcessNotFoundError} 进程不存在
 * @throws {StdinNotOpenError} 进程的 stdin 未打开
 *
 * @example
 * await sandbox.process.sendStdin(1234, 'user input\n')
 */
sendStdin(pid: number, data: string): Promise<void>
```

### 3.6 runCode

执行代码片段。

```typescript
/**
 * 执行代码片段
 *
 * 支持多种语言，自动处理临时文件创建和清理。
 *
 * @param code - 代码内容
 * @param language - 编程语言
 * @param options - 执行选项
 * @returns 执行结果
 * @throws {LanguageNotSupportedError} 不支持的语言
 * @throws {CommandTimeoutError} 执行超时
 *
 * @example
 * // Python 代码
 * const result = await sandbox.process.runCode(
 *   'print("Hello, World!")',
 *   'python'
 * )
 *
 * @example
 * // JavaScript 代码
 * const result = await sandbox.process.runCode(
 *   'console.log(1 + 2)',
 *   'javascript'
 * )
 *
 * @example
 * // 带参数和输入
 * const result = await sandbox.process.runCode(
 *   `
 *   import sys
 *   name = input("Name: ")
 *   print(f"Hello, {name}!")
 *   print(f"Args: {sys.argv[1:]}")
 *   `,
 *   'python',
 *   {
 *     args: ['arg1', 'arg2'],
 *     input: 'World\n'
 *   }
 * )
 *
 * @example
 * // 捕获 matplotlib 图表
 * const result = await sandbox.process.runCode(
 *   `
 *   import matplotlib.pyplot as plt
 *   plt.plot([1, 2, 3], [1, 4, 9])
 *   plt.show()
 *   `,
 *   'python',
 *   { captureCharts: true }
 * )
 * console.log(result.charts) // [{ type: 'image', data: 'base64...', mimeType: 'image/png' }]
 */
runCode(
  code: string,
  language: CodeLanguage,
  options?: {
    args?: string[]
    env?: Record<string, string>
    timeoutMs?: number
    cwd?: string
    input?: string
    captureCharts?: boolean
  }
): Promise<CodeRunResult>
```

```python
def run_code(
    self,
    code: str,
    language: CodeLanguage,
    *,
    args: Optional[List[str]] = None,
    env: Optional[Dict[str, str]] = None,
    timeout_ms: int = 30000,
    cwd: Optional[str] = None,
    input: Optional[str] = None,
    capture_charts: bool = False
) -> CodeRunResult:
    """
    执行代码片段

    Args:
        code: 代码内容
        language: 编程语言
        args: 命令行参数
        env: 环境变量
        timeout_ms: 超时时间
        cwd: 工作目录
        input: 标准输入
        capture_charts: 是否捕获图表

    Returns:
        执行结果

    Example:
        >>> result = sandbox.process.run_code('print(1 + 2)', 'python')
        >>> print(result.stdout)  # "3\n"
    """
```

### 3.7 createSession

创建执行会话。

```typescript
/**
 * 创建执行会话
 *
 * 会话保持环境状态（工作目录、环境变量、命令历史）。
 *
 * @param options - 会话选项
 * @returns 会话信息
 *
 * @example
 * const session = await sandbox.process.createSession({
 *   id: 'my-session',
 *   cwd: '/app',
 *   env: { DEBUG: 'true' }
 * })
 */
createSession(options?: {
  /**
   * 会话 ID（不提供则自动生成）
   */
  id?: string

  /**
   * 初始工作目录
   */
  cwd?: string

  /**
   * 初始环境变量
   */
  env?: Record<string, string>

  /**
   * 运行用户
   */
  user?: string

  /**
   * 会话超时（毫秒，无活动后自动销毁）
   * @default 3600000 (1小时)
   */
  timeoutMs?: number
}): Promise<Session>
```

### 3.8 getSession

获取会话信息。

```typescript
/**
 * 获取会话信息
 *
 * @param id - 会话 ID
 * @returns 会话信息
 * @throws {SessionNotFoundError} 会话不存在
 *
 * @example
 * const session = await sandbox.process.getSession('my-session')
 * console.log(`CWD: ${session.cwd}`)
 */
getSession(id: string): Promise<Session>
```

### 3.9 listSessions

列出所有会话。

```typescript
/**
 * 列出所有会话
 *
 * @returns 会话列表
 *
 * @example
 * const sessions = await sandbox.process.listSessions()
 */
listSessions(): Promise<Session[]>
```

### 3.10 deleteSession

删除会话。

```typescript
/**
 * 删除会话
 *
 * @param id - 会话 ID
 * @throws {SessionNotFoundError} 会话不存在
 *
 * @example
 * await sandbox.process.deleteSession('my-session')
 */
deleteSession(id: string): Promise<void>
```

### 3.11 executeInSession

在会话中执行命令。

```typescript
/**
 * 在会话中执行命令
 *
 * 命令在会话的环境中执行，可以访问之前命令设置的环境变量等。
 *
 * @param sessionId - 会话 ID
 * @param command - 要执行的命令
 * @param options - 执行选项
 * @returns 命令句柄
 * @throws {SessionNotFoundError} 会话不存在
 *
 * @example
 * // 创建会话
 * const session = await sandbox.process.createSession({ cwd: '/app' })
 *
 * // 在会话中执行命令（环境变量会保留）
 * await sandbox.process.executeInSession(session.id, 'export API_KEY=xxx')
 * await sandbox.process.executeInSession(session.id, 'cd src')
 * const result = await sandbox.process.executeInSession(session.id, 'echo $API_KEY && pwd')
 * // result.stdout: "xxx\n/app/src\n"
 */
executeInSession(
  sessionId: string,
  command: string,
  options?: {
    /**
     * 执行超时（毫秒）
     */
    timeoutMs?: number

    /**
     * 是否等待完成
     * @default true
     */
    wait?: boolean

    /**
     * 标准输出回调
     */
    onStdout?: (data: string) => void

    /**
     * 标准错误回调
     */
    onStderr?: (data: string) => void
  }
): Promise<CommandHandle>
```

```python
def execute_in_session(
    self,
    session_id: str,
    command: str,
    *,
    timeout_ms: int = 60000,
    wait: bool = True,
    on_stdout: Optional[Callable[[str], None]] = None,
    on_stderr: Optional[Callable[[str], None]] = None
) -> CommandHandle:
    """
    在会话中执行命令

    Args:
        session_id: 会话 ID
        command: 要执行的命令
        timeout_ms: 超时时间
        wait: 是否等待完成
        on_stdout: 标准输出回调
        on_stderr: 标准错误回调

    Returns:
        命令句柄

    Raises:
        SessionNotFoundError: 会话不存在
    """
```

---

## 4. REST API

### 4.1 端点列表

| 方法 | 端点 | 描述 |
|------|------|------|
| POST | `/api/v1/sandboxes/{id}/process/run` | 执行命令 |
| POST | `/api/v1/sandboxes/{id}/process/spawn` | 启动后台命令 |
| GET | `/api/v1/sandboxes/{id}/process/list` | 列出进程 |
| POST | `/api/v1/sandboxes/{id}/process/{pid}/kill` | 终止进程 |
| POST | `/api/v1/sandboxes/{id}/process/{pid}/stdin` | 发送标准输入 |
| GET | `/api/v1/sandboxes/{id}/process/{pid}` | 获取进程信息 |
| POST | `/api/v1/sandboxes/{id}/process/code` | 执行代码 |
| POST | `/api/v1/sandboxes/{id}/sessions` | 创建会话 |
| GET | `/api/v1/sandboxes/{id}/sessions` | 列出会话 |
| GET | `/api/v1/sandboxes/{id}/sessions/{sid}` | 获取会话 |
| DELETE | `/api/v1/sandboxes/{id}/sessions/{sid}` | 删除会话 |
| POST | `/api/v1/sandboxes/{id}/sessions/{sid}/exec` | 在会话中执行 |
| WS | `/api/v1/sandboxes/{id}/process/{pid}/attach` | 附加到进程 |

### 4.2 执行命令

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/process/run
Content-Type: application/json
Authorization: Bearer <token>

{
  "command": "npm install",
  "cwd": "/app",
  "env": {
    "NODE_ENV": "production"
  },
  "timeoutMs": 300000
}
```

**响应** (200 OK):

```json
{
  "exitCode": 0,
  "stdout": "added 150 packages in 10s\n",
  "stderr": "",
  "durationMs": 10234,
  "timedOut": false
}
```

### 4.3 启动后台命令

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/process/spawn
Content-Type: application/json
Authorization: Bearer <token>

{
  "command": "python -m http.server 8080",
  "cwd": "/app/static",
  "stdin": false
}
```

**响应** (200 OK):

```json
{
  "pid": 1234,
  "command": "python -m http.server 8080",
  "startedAt": "2024-01-15T10:30:00Z"
}
```

### 4.4 列出进程

**请求**:

```http
GET /api/v1/sandboxes/sbx-abc123/process/list?user=user
Authorization: Bearer <token>
```

**响应** (200 OK):

```json
{
  "processes": [
    {
      "pid": 1,
      "ppid": 0,
      "command": "/sbin/init",
      "args": [],
      "cwd": "/",
      "user": "root",
      "startedAt": "2024-01-15T10:00:00Z",
      "cpuPercent": 0.1,
      "memoryBytes": 10485760,
      "state": "sleeping"
    },
    {
      "pid": 1234,
      "ppid": 1,
      "command": "python",
      "args": ["-m", "http.server", "8080"],
      "cwd": "/app/static",
      "user": "user",
      "startedAt": "2024-01-15T10:30:00Z",
      "cpuPercent": 0.5,
      "memoryBytes": 52428800,
      "state": "running"
    }
  ]
}
```

### 4.5 执行代码

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/process/code
Content-Type: application/json
Authorization: Bearer <token>

{
  "code": "print('Hello, World!')",
  "language": "python",
  "timeoutMs": 30000
}
```

**响应** (200 OK):

```json
{
  "exitCode": 0,
  "stdout": "Hello, World!\n",
  "stderr": "",
  "durationMs": 156,
  "timedOut": false
}
```

### 4.6 附加到进程 (WebSocket)

**连接**:

```
WS /api/v1/sandboxes/sbx-abc123/process/1234/attach
```

**服务器消息**:

```json
{
  "type": "stdout",
  "data": "Server started on port 8080\n"
}
```

```json
{
  "type": "exit",
  "exitCode": 0
}
```

**客户端消息**（发送 stdin）:

```json
{
  "type": "stdin",
  "data": "user input\n"
}
```

---

## 5. 使用示例

### 5.1 TypeScript 示例

```typescript
import { WorkspaceClient, CodeLanguage } from '@workspace-sdk/typescript'

async function processExamples() {
  const client = new WorkspaceClient({ apiUrl: 'https://api.example.com' })
  const sandbox = await client.sandbox.create({ template: 'python:3.11' })

  try {
    // ========== 基本命令执行 ==========
    console.log('--- Basic Command Execution ---')

    const result = await sandbox.process.run('echo "Hello, World!"')
    console.log('Output:', result.stdout)
    console.log('Exit code:', result.exitCode)

    // ========== 带选项的命令 ==========
    console.log('\n--- Command with Options ---')

    const npmResult = await sandbox.process.run('pip install requests', {
      cwd: '/app',
      timeoutMs: 120000,
      onStdout: (data) => process.stdout.write(data),
      onStderr: (data) => process.stderr.write(data)
    })

    // ========== 后台命令 ==========
    console.log('\n--- Background Command ---')

    // 启动 HTTP 服务器
    const serverHandle = await sandbox.process.spawn('python -m http.server 8080', {
      cwd: '/app'
    })
    console.log('Server PID:', serverHandle.pid)

    // 等待服务器启动
    await new Promise(resolve => setTimeout(resolve, 2000))

    // 测试服务器
    const testResult = await sandbox.process.run('curl -s http://localhost:8080')
    console.log('Server response:', testResult.stdout.slice(0, 100))

    // 停止服务器
    await serverHandle.kill()
    console.log('Server stopped')

    // ========== 代码执行 ==========
    console.log('\n--- Code Execution ---')

    const codeResult = await sandbox.process.runCode(`
import sys
import json

data = {"python_version": sys.version, "platform": sys.platform}
print(json.dumps(data, indent=2))
    `, CodeLanguage.PYTHON)

    console.log('Code output:', codeResult.stdout)

    // ========== 会话管理 ==========
    console.log('\n--- Session Management ---')

    // 创建会话
    const session = await sandbox.process.createSession({
      id: 'dev-session',
      cwd: '/app'
    })
    console.log('Session created:', session.id)

    // 在会话中执行多个命令（环境保持）
    await sandbox.process.executeInSession(session.id, 'export MY_VAR=hello')
    await sandbox.process.executeInSession(session.id, 'cd /tmp')

    const sessionResult = await sandbox.process.executeInSession(
      session.id,
      'echo $MY_VAR && pwd'
    )
    await sessionResult.wait()
    console.log('Session output:', sessionResult.stdout) // "hello\n/tmp\n"

    // 删除会话
    await sandbox.process.deleteSession(session.id)

    // ========== 进程列表 ==========
    console.log('\n--- Process List ---')

    const processes = await sandbox.process.list()
    for (const proc of processes) {
      console.log(`PID ${proc.pid}: ${proc.command} (${proc.cpuPercent.toFixed(1)}% CPU)`)
    }

  } finally {
    await sandbox.delete()
  }
}

processExamples().catch(console.error)
```

### 5.2 Python 示例

```python
from workspace_sdk import WorkspaceClient, CodeLanguage
import time

def process_examples():
    client = WorkspaceClient()

    with client.sandbox.create(template='python:3.11') as sandbox:
        # ========== 基本命令执行 ==========
        print('--- Basic Command Execution ---')

        result = sandbox.process.run('echo "Hello, World!"')
        print(f'Output: {result.stdout}')
        print(f'Exit code: {result.exit_code}')

        # ========== 带选项的命令 ==========
        print('\n--- Command with Options ---')

        def on_output(data):
            print(data, end='')

        result = sandbox.process.run(
            'pip install requests',
            cwd='/app',
            timeout_ms=120000,
            on_stdout=on_output,
            on_stderr=on_output
        )

        # ========== 后台命令 ==========
        print('\n--- Background Command ---')

        # 启动后台进程
        handle = sandbox.process.spawn('python -m http.server 8080', cwd='/app')
        print(f'Server PID: {handle.pid}')

        # 等待启动
        time.sleep(2)

        # 测试
        test_result = sandbox.process.run('curl -s http://localhost:8080')
        print(f'Server response: {test_result.stdout[:100]}')

        # 停止
        handle.kill()
        print('Server stopped')

        # ========== 代码执行 ==========
        print('\n--- Code Execution ---')

        code_result = sandbox.process.run_code('''
import sys
import json

data = {"python_version": sys.version, "platform": sys.platform}
print(json.dumps(data, indent=2))
        ''', CodeLanguage.PYTHON)

        print(f'Code output: {code_result.stdout}')

        # ========== 会话管理 ==========
        print('\n--- Session Management ---')

        # 创建会话
        session = sandbox.process.create_session(id='dev-session', cwd='/app')
        print(f'Session created: {session.id}')

        # 在会话中执行
        sandbox.process.execute_in_session(session.id, 'export MY_VAR=hello')
        sandbox.process.execute_in_session(session.id, 'cd /tmp')

        handle = sandbox.process.execute_in_session(session.id, 'echo $MY_VAR && pwd')
        result = handle.wait()
        print(f'Session output: {result.stdout}')  # "hello\n/tmp\n"

        # 删除会话
        sandbox.process.delete_session(session.id)

        # ========== 进程列表 ==========
        print('\n--- Process List ---')

        processes = sandbox.process.list()
        for proc in processes:
            print(f'PID {proc.pid}: {proc.command} ({proc.cpu_percent:.1f}% CPU)')

if __name__ == '__main__':
    process_examples()
```

### 5.3 交互式命令示例

```typescript
// 交互式 Python REPL
async function interactiveREPL() {
  const handle = await sandbox.process.spawn('python -i', {
    stdin: true
  })

  // 发送命令
  await handle.sendStdin('x = 10\n')
  await handle.sendStdin('y = 20\n')
  await handle.sendStdin('print(x + y)\n')
  await handle.sendStdin('exit()\n')

  // 等待完成
  const result = await handle.wait()
  console.log('Output:', result.stdout)
}

// 监听长时间运行的命令
async function monitorLongRunningCommand() {
  const handle = await sandbox.process.spawn('npm run build', {
    cwd: '/app'
  })

  handle.onStdout((data) => {
    console.log('[stdout]', data)
  })

  handle.onStderr((data) => {
    console.error('[stderr]', data)
  })

  handle.onExit((result) => {
    console.log('Build finished with code:', result.exitCode)
  })

  // 等待完成或超时取消
  const timeoutId = setTimeout(() => {
    console.log('Build taking too long, cancelling...')
    handle.kill()
  }, 300000) // 5分钟超时

  await handle.wait()
  clearTimeout(timeoutId)
}
```

---

## 6. 错误处理

### 6.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 4000 | PROCESS_NOT_FOUND | 404 | 进程不存在 |
| 4001 | PROCESS_ALREADY_RUNNING | 409 | 进程已在运行 |
| 4002 | PROCESS_TIMEOUT | 408 | 命令执行超时 |
| 4003 | PROCESS_FAILED | 500 | 命令执行失败 |
| 4004 | PROCESS_KILLED | 500 | 进程被终止 |
| 4005 | SESSION_NOT_FOUND | 404 | 会话不存在 |
| 4006 | SESSION_EXPIRED | 410 | 会话已过期 |
| 4007 | STDIN_NOT_OPEN | 400 | stdin 未打开 |
| 4008 | LANGUAGE_NOT_SUPPORTED | 400 | 不支持的语言 |

### 6.2 CommandError

```typescript
/**
 * 命令执行错误
 */
class CommandError extends WorkspaceError {
  /**
   * 退出码
   */
  exitCode: number

  /**
   * 标准输出
   */
  stdout: string

  /**
   * 标准错误
   */
  stderr: string

  /**
   * 执行的命令
   */
  command: string
}
```

### 6.3 错误处理示例

```typescript
import {
  CommandError,
  CommandTimeoutError,
  ProcessNotFoundError
} from '@workspace-sdk/typescript'

async function runWithRetry(
  sandbox: Sandbox,
  command: string,
  retries: number = 3
): Promise<CommandResult> {
  for (let i = 0; i < retries; i++) {
    try {
      return await sandbox.process.run(command, {
        timeoutMs: 60000
      })
    } catch (error) {
      if (error instanceof CommandTimeoutError) {
        console.log(`Attempt ${i + 1} timed out, retrying...`)
        continue
      }

      if (error instanceof CommandError) {
        console.error(`Command failed with code ${error.exitCode}`)
        console.error('stderr:', error.stderr)

        // 某些退出码可以重试
        if (error.exitCode === 1 && i < retries - 1) {
          console.log('Retrying...')
          continue
        }
      }

      throw error
    }
  }

  throw new Error(`Command failed after ${retries} retries`)
}
```

```python
from workspace_sdk import (
    CommandError,
    CommandTimeoutError,
    ProcessNotFoundError
)

def run_with_retry(sandbox, command: str, retries: int = 3):
    for i in range(retries):
        try:
            return sandbox.process.run(command, timeout_ms=60000)
        except CommandTimeoutError:
            print(f'Attempt {i + 1} timed out, retrying...')
            continue
        except CommandError as e:
            print(f'Command failed with code {e.exit_code}')
            print(f'stderr: {e.stderr}')

            if e.exit_code == 1 and i < retries - 1:
                print('Retrying...')
                continue

            raise

    raise Exception(f'Command failed after {retries} retries')
```

---

## 附录

### A. 支持的编程语言

| 语言 | 语言 ID | 执行命令 | 文件扩展名 |
|------|--------|---------|-----------|
| Python | `python` | `python {file}` | `.py` |
| JavaScript | `javascript` | `node {file}` | `.js` |
| TypeScript | `typescript` | `npx ts-node {file}` | `.ts` |
| Bash | `bash` | `bash {file}` | `.sh` |
| Go | `go` | `go run {file}` | `.go` |
| Rust | `rust` | `rustc {file} -o /tmp/a && /tmp/a` | `.rs` |
| Java | `java` | `java {file}` | `.java` |
| C | `c` | `gcc {file} -o /tmp/a && /tmp/a` | `.c` |
| C++ | `cpp` | `g++ {file} -o /tmp/a && /tmp/a` | `.cpp` |
| Ruby | `ruby` | `ruby {file}` | `.rb` |
| PHP | `php` | `php {file}` | `.php` |

### B. 信号参考

| 信号 | 值 | 描述 |
|------|---|------|
| SIGHUP | 1 | 挂起 |
| SIGINT | 2 | 中断（Ctrl+C） |
| SIGQUIT | 3 | 退出 |
| SIGKILL | 9 | 强制终止（不可捕获） |
| SIGTERM | 15 | 终止（默认） |
| SIGSTOP | 19 | 停止（不可捕获） |
| SIGCONT | 18 | 继续 |
| SIGUSR1 | 10 | 用户定义信号 1 |
| SIGUSR2 | 12 | 用户定义信号 2 |

### C. 资源限制

| 资源 | 默认限制 | 说明 |
|------|---------|------|
| 命令超时 | 60 秒 | 可通过 timeoutMs 调整 |
| 最大并发进程 | 100 | - |
| stdout/stderr 缓冲区 | 10 MB | 超出截断 |
| 会话超时 | 1 小时 | 无活动后自动销毁 |
| 最大会话数 | 10 | 每个 Sandbox |
