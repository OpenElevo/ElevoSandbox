# Process 服务接口文档

Process 服务提供 Sandbox 内的命令执行功能。

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

Process 服务通过 Sandbox 实例访问，提供命令执行能力。

### 1.1 功能列表

| 方法 | 描述 |
|-----|------|
| `run` | 执行命令并等待完成 |

### 1.2 访问方式

```typescript
const sandbox = await client.sandbox.get('sbx-abc123')
const result = await sandbox.process.run('ls -la')
```

---

## 2. 类型定义

### 2.1 Process

Process 服务接口。

```typescript
interface Process {
  run(command: string, options?: RunOptions): Promise<CommandResult>
}
```

**Python 定义**:

```python
class Process(Protocol):
    async def run(
        self,
        command: str,
        options: Optional[RunOptions] = None
    ) -> CommandResult: ...
```

### 2.2 RunOptions

命令执行选项。

```typescript
interface RunOptions {
  /**
   * 工作目录
   * 命令执行的当前目录
   * @optional
   * @default "/"
   * @example "/app"
   */
  cwd?: string

  /**
   * 环境变量
   * 附加到命令执行环境的变量
   * @optional
   * @example { "NODE_ENV": "production" }
   */
  envs?: Record<string, string>

  /**
   * 超时时间 (毫秒)
   * 命令执行的最大等待时间
   * @optional
   * @default 60000
   * @example 30000
   */
  timeout?: number
}
```

**Python 定义**:

```python
@dataclass
class RunOptions:
    cwd: Optional[str] = None
    """工作目录"""

    envs: Optional[Dict[str, str]] = None
    """环境变量"""

    timeout: Optional[int] = None
    """超时时间 (毫秒)"""
```

### 2.3 CommandResult

命令执行结果。

```typescript
interface CommandResult {
  /**
   * 退出码
   * 0 表示成功，非 0 表示失败
   * @example 0
   */
  exitCode: number

  /**
   * 标准输出内容
   * @example "Hello World\n"
   */
  stdout: string

  /**
   * 标准错误输出内容
   * @example ""
   */
  stderr: string
}
```

**Python 定义**:

```python
@dataclass
class CommandResult:
    exit_code: int
    """退出码"""

    stdout: str
    """标准输出"""

    stderr: str
    """标准错误"""
```

---

## 3. 方法详情

### 3.1 run

执行命令并等待完成。

**签名**:

```typescript
run(command: string, options?: RunOptions): Promise<CommandResult>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `command` | `string` | 是 | 要执行的命令 |
| `options` | `RunOptions` | 否 | 执行选项 |
| `options.cwd` | `string` | 否 | 工作目录，默认 `/` |
| `options.envs` | `Record<string, string>` | 否 | 环境变量 |
| `options.timeout` | `number` | 否 | 超时时间 (ms)，默认 60000 |

**返回值**:

| 类型 | 描述 |
|-----|------|
| `Promise<CommandResult>` | 命令执行结果 |

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 4002 | `PROCESS_TIMEOUT` | 命令执行超时 |
| 4004 | `COMMAND_FAILED` | 命令执行失败 (内部错误) |

**说明**:
- 命令通过 shell 执行 (`/bin/sh -c`)
- 支持管道、重定向等 shell 特性
- `stdout` 和 `stderr` 会完整捕获
- 即使退出码非 0，也会正常返回结果而非抛出异常

**示例**:

```typescript
// TypeScript
const result = await sandbox.process.run('echo "Hello World"')
console.log(result.stdout)  // Hello World
console.log(result.exitCode)  // 0
```

```python
# Python
result = await sandbox.process.run('echo "Hello World"')
print(result.stdout)  # Hello World
print(result.exit_code)  # 0
```

---

## 4. REST API

### 4.1 执行命令

```
POST /api/v1/sandboxes/{sandboxId}/process/run
```

**请求**:

```json
{
  "command": "python main.py",
  "cwd": "/app",
  "envs": {
    "DEBUG": "true"
  },
  "timeout": 30000
}
```

**响应** (200 OK):

```json
{
  "exitCode": 0,
  "stdout": "Hello World\n",
  "stderr": ""
}
```

**错误响应** (408 Request Timeout):

```json
{
  "error": {
    "code": 4002,
    "name": "PROCESS_TIMEOUT",
    "message": "Command execution timed out after 30000ms"
  }
}
```

---

## 5. 使用示例

### 5.1 基础命令执行

```typescript
// TypeScript
// 简单命令
const result = await sandbox.process.run('ls -la')
console.log(result.stdout)

// 带选项
const result2 = await sandbox.process.run('npm install', {
  cwd: '/app',
  timeout: 120000
})
```

```python
# Python
# 简单命令
result = await sandbox.process.run("ls -la")
print(result.stdout)

# 带选项
result2 = await sandbox.process.run("pip install -r requirements.txt", RunOptions(
    cwd="/app",
    timeout=120000
))
```

### 5.2 检查退出码

```typescript
// TypeScript
const result = await sandbox.process.run('grep "error" /var/log/app.log')

if (result.exitCode === 0) {
  console.log('Errors found:')
  console.log(result.stdout)
} else if (result.exitCode === 1) {
  console.log('No errors found')
} else {
  console.error('Command failed:', result.stderr)
}
```

```python
# Python
result = await sandbox.process.run('grep "error" /var/log/app.log')

if result.exit_code == 0:
    print("Errors found:")
    print(result.stdout)
elif result.exit_code == 1:
    print("No errors found")
else:
    print(f"Command failed: {result.stderr}")
```

### 5.3 链式命令

```typescript
// TypeScript
// 使用 && 链接命令
const result = await sandbox.process.run(
  'cd /app && npm install && npm run build',
  { timeout: 300000 }
)

if (result.exitCode !== 0) {
  console.error('Build failed:', result.stderr)
}
```

```python
# Python
result = await sandbox.process.run(
    "cd /app && pip install -r requirements.txt && python setup.py build",
    RunOptions(timeout=300000)
)

if result.exit_code != 0:
    print(f"Build failed: {result.stderr}")
```

### 5.4 使用管道

```typescript
// TypeScript
// 统计代码行数
const result = await sandbox.process.run(
  'find /app -name "*.py" | xargs wc -l | tail -1',
  { cwd: '/app' }
)
console.log(`Total lines: ${result.stdout.trim()}`)
```

### 5.5 环境变量

```typescript
// TypeScript
const result = await sandbox.process.run('node server.js', {
  cwd: '/app',
  envs: {
    NODE_ENV: 'production',
    PORT: '3000',
    DATABASE_URL: 'postgres://localhost/mydb'
  },
  timeout: 5000
})
```

```python
# Python
result = await sandbox.process.run("python app.py", RunOptions(
    cwd="/app",
    envs={
        "FLASK_ENV": "production",
        "DATABASE_URL": "postgres://localhost/mydb"
    },
    timeout=5000
))
```

### 5.6 执行脚本

```typescript
// TypeScript
async function runScript(sandbox: Sandbox) {
  // 先写入脚本
  await sandbox.fs.write('/tmp/setup.sh', `#!/bin/bash
set -e
echo "Setting up..."
apt-get update
apt-get install -y curl
echo "Done!"
`)

  // 执行脚本
  const result = await sandbox.process.run('bash /tmp/setup.sh', {
    timeout: 300000
  })

  return result.exitCode === 0
}
```

### 5.7 完整工作流

```typescript
// TypeScript
async function runPythonProject(sandbox: Sandbox) {
  // 1. 创建项目文件
  await sandbox.fs.mkdir('/app')
  await sandbox.fs.write('/app/main.py', `
import sys
print(f"Python version: {sys.version}")
print("Hello from sandbox!")
`)

  // 2. 执行
  const result = await sandbox.process.run('python main.py', {
    cwd: '/app'
  })

  // 3. 返回结果
  return {
    success: result.exitCode === 0,
    output: result.stdout,
    error: result.stderr
  }
}
```

```python
# Python
async def run_python_project(sandbox: Sandbox) -> dict:
    # 1. 创建项目文件
    await sandbox.fs.mkdir("/app")
    await sandbox.fs.write("/app/main.py", '''
import sys
print(f"Python version: {sys.version}")
print("Hello from sandbox!")
''')

    # 2. 执行
    result = await sandbox.process.run("python main.py", RunOptions(cwd="/app"))

    # 3. 返回结果
    return {
        "success": result.exit_code == 0,
        "output": result.stdout,
        "error": result.stderr
    }
```

---

## 6. 错误处理

### 6.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 4002 | `PROCESS_TIMEOUT` | 408 | 命令执行超时 |
| 4004 | `COMMAND_FAILED` | 500 | 命令执行失败 (内部错误) |

### 6.2 超时处理

```typescript
// TypeScript
import { ProcessTimeoutError } from '@workspace-sdk/typescript'

try {
  const result = await sandbox.process.run('sleep 100', {
    timeout: 5000
  })
} catch (error) {
  if (error instanceof ProcessTimeoutError) {
    console.error('Command timed out')
  } else {
    throw error
  }
}
```

```python
# Python
from workspace_sdk.errors import ProcessTimeoutError

try:
    result = await sandbox.process.run("sleep 100", RunOptions(timeout=5000))
except ProcessTimeoutError:
    print("Command timed out")
```

### 6.3 命令失败 vs 异常

**重要**: 命令返回非 0 退出码**不会**抛出异常，只有系统级错误才会抛出异常。

```typescript
// TypeScript
// 这不会抛出异常，即使命令"失败"
const result = await sandbox.process.run('exit 1')
console.log(result.exitCode)  // 1

// 需要手动检查退出码
if (result.exitCode !== 0) {
  throw new Error(`Command failed with exit code ${result.exitCode}`)
}
```

### 6.4 辅助函数

```typescript
// TypeScript
async function runOrThrow(
  sandbox: Sandbox,
  command: string,
  options?: RunOptions
): Promise<string> {
  const result = await sandbox.process.run(command, options)

  if (result.exitCode !== 0) {
    throw new Error(
      `Command "${command}" failed with exit code ${result.exitCode}\n` +
      `stdout: ${result.stdout}\n` +
      `stderr: ${result.stderr}`
    )
  }

  return result.stdout
}

// 使用
const output = await runOrThrow(sandbox, 'npm run build', { cwd: '/app' })
```

```python
# Python
async def run_or_throw(
    sandbox: Sandbox,
    command: str,
    options: Optional[RunOptions] = None
) -> str:
    result = await sandbox.process.run(command, options)

    if result.exit_code != 0:
        raise RuntimeError(
            f'Command "{command}" failed with exit code {result.exit_code}\n'
            f'stdout: {result.stdout}\n'
            f'stderr: {result.stderr}'
        )

    return result.stdout

# 使用
output = await run_or_throw(sandbox, "python -m pytest", RunOptions(cwd="/app"))
```
