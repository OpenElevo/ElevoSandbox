# 统一 SDK API - 最简版

仅包含 E2B 和 Daytona **两者都有**的功能接口。

---

## 1. Sandbox 服务

```typescript
interface SandboxService {
  /**
   * 创建 Sandbox
   */
  create(params: CreateSandboxParams): Promise<Sandbox>

  /**
   * 获取 Sandbox
   */
  get(sandboxId: string): Promise<Sandbox>

  /**
   * 列出 Sandbox
   */
  list(): Promise<Sandbox[]>

  /**
   * 删除 Sandbox
   */
  delete(sandboxId: string): Promise<void>
}

interface CreateSandboxParams {
  /** 模板/镜像 */
  template: string
  /** 名称 */
  name?: string
  /** 环境变量 */
  envs?: Record<string, string>
}

interface Sandbox {
  id: string
  name?: string
  state: 'running' | 'stopped'
  createdAt: Date

  /** 子服务 */
  fs: FileSystem
  process: Process
  pty: PTY
}
```

---

## 2. FileSystem 服务

```typescript
interface FileSystem {
  /**
   * 读取文件
   */
  read(path: string): Promise<string>

  /**
   * 写入文件
   */
  write(path: string, content: string): Promise<void>

  /**
   * 创建目录
   */
  mkdir(path: string): Promise<void>

  /**
   * 列出目录
   */
  list(path: string): Promise<FileInfo[]>

  /**
   * 删除文件/目录
   */
  remove(path: string): Promise<void>

  /**
   * 移动/重命名
   */
  move(source: string, destination: string): Promise<void>

  /**
   * 获取文件信息
   */
  getInfo(path: string): Promise<FileInfo>
}

interface FileInfo {
  name: string
  path: string
  type: 'file' | 'directory'
  size: number
}
```

---

## 3. Process 服务

```typescript
interface Process {
  /**
   * 执行命令（同步等待结果）
   */
  run(command: string, options?: RunOptions): Promise<CommandResult>
}

interface RunOptions {
  /** 工作目录 */
  cwd?: string
  /** 环境变量 */
  envs?: Record<string, string>
  /** 超时(ms) */
  timeout?: number
}

interface CommandResult {
  exitCode: number
  stdout: string
  stderr: string
}
```

---

## 4. PTY 服务

```typescript
interface PTY {
  /**
   * 创建 PTY
   */
  create(options: PTYOptions): Promise<PTYHandle>

  /**
   * 调整大小
   */
  resize(ptyId: string, cols: number, rows: number): Promise<void>

  /**
   * 关闭 PTY
   */
  kill(ptyId: string): Promise<void>
}

interface PTYOptions {
  cols: number
  rows: number
  onData: (data: Uint8Array) => void
}

interface PTYHandle {
  id: string
  /** 写入数据 */
  write(data: Uint8Array): Promise<void>
  /** 关闭 */
  kill(): Promise<void>
}
```

---

## 总结

最简版仅包含 4 个服务，共 **15 个方法**：

| 服务 | 方法数 | 方法列表 |
|-----|-------|---------|
| Sandbox | 4 | create, get, list, delete |
| FileSystem | 7 | read, write, mkdir, list, remove, move, getInfo |
| Process | 1 | run |
| PTY | 3 | create, resize, kill |

这是两个 SDK 功能的最小公共子集，实现这些接口即可覆盖最基础的使用场景。
