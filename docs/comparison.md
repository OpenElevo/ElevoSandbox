# SDK 接口对比分析

本文档对比 E2B、Daytona 原始接口与统一 SDK 设计的差异。

---

## 1. Sandbox 生命周期管理

### E2B 原始接口

```typescript
// 静态方法
static async create(template?: string, opts?: SandboxOpts): Promise<Sandbox>
static async connect(sandboxId: string, opts?: SandboxConnectOpts): Promise<Sandbox>
static list(opts?: SandboxListOpts): SandboxPaginator

// 实例方法
async kill(opts?): Promise<void>
async setTimeout(timeoutMs: number, opts?): Promise<void>
async isRunning(opts?): Promise<boolean>
async betaPause(opts?): Promise<boolean>  // Beta 功能
async getInfo(opts?): Promise<SandboxInfo>
async getMetrics(opts?): Promise<SandboxMetrics[]>
getHost(port: number): string
```

### Daytona 原始接口

```typescript
// Daytona 主类
async create(params?: CreateSandboxParams, options?): Promise<Sandbox>
async get(sandboxIdOrName: string): Promise<Sandbox>
async findOne(filter: SandboxFilter): Promise<Sandbox>
async list(labels?, page?, limit?): Promise<PaginatedSandboxes>
async start(sandbox: Sandbox, timeout?: number): Promise<void>
async stop(sandbox: Sandbox): Promise<void>
async delete(sandbox: Sandbox, timeout?: number): Promise<void>

// Sandbox 实例方法
async start(timeout?: number): Promise<void>
async stop(timeout?: number): Promise<void>
async delete(timeout?: number): Promise<void>
async archive(): Promise<void>
async recover(timeout?: number): Promise<void>
async waitUntilStarted(timeout?: number): Promise<void>
async waitUntilStopped(timeout?: number): Promise<void>
async setLabels(labels: Record<string, string>): Promise<Record<string, string>>
async setAutostopInterval(interval: number): Promise<void>
async getPreviewLink(port: number): Promise<PortPreviewUrl>
```

### 统一 SDK 设计

```typescript
interface SandboxService {
  create(params: CreateSandboxParams): Promise<SandboxInfo>
  get(sandboxId: string): Promise<SandboxInfo>
  list(params?: ListSandboxParams): Promise<PaginatedResult<SandboxInfo>>
  delete(sandboxId: string, force?: boolean): Promise<void>
  start(sandboxId: string): Promise<void>
  stop(sandboxId: string, timeout?: number): Promise<void>
  pause(sandboxId: string): Promise<void>      // ✓ 来自 E2B betaPause
  resume(sandboxId: string): Promise<void>     // ✓ 新增
  getMetrics(sandboxId: string): Promise<SandboxMetrics>  // ✓ 来自 E2B
}
```

### 对比总结

| 功能 | E2B | Daytona | 统一设计 | 说明 |
|-----|-----|---------|---------|------|
| 创建 | `Sandbox.create()` | `daytona.create()` | `sandbox.create()` | 统一为服务方法 |
| 获取 | `Sandbox.connect()` | `daytona.get()` | `sandbox.get()` | E2B 用 connect |
| 列表 | `Sandbox.list()` | `daytona.list()` | `sandbox.list()` | ✓ |
| 删除 | `sandbox.kill()` | `sandbox.delete()` | `sandbox.delete()` | E2B 用 kill |
| 启动 | ✗ (创建即启动) | `sandbox.start()` | `sandbox.start()` | 来自 Daytona |
| 停止 | ✗ | `sandbox.stop()` | `sandbox.stop()` | 来自 Daytona |
| 暂停 | `betaPause()` | ✗ | `sandbox.pause()` | 来自 E2B |
| 恢复 | ✗ | `recover()` | `sandbox.resume()` | 重命名 |
| 指标 | `getMetrics()` | ✗ | `getMetrics()` | 来自 E2B |
| 归档 | ✗ | `archive()` | ✗ (用快照替代) | 简化设计 |

---

## 2. 文件系统操作

### E2B 原始接口

```typescript
class Filesystem {
  // 读取 - 支持多种格式
  async read(path: string, opts?: { format?: 'text' }): Promise<string>
  async read(path: string, opts?: { format: 'bytes' }): Promise<Uint8Array>
  async read(path: string, opts?: { format: 'blob' }): Promise<Blob>
  async read(path: string, opts?: { format: 'stream' }): Promise<ReadableStream<Uint8Array>>

  // 写入 - 支持单个和批量
  async write(path: string, data: string | ArrayBuffer | Blob | ReadableStream, opts?): Promise<WriteInfo>
  async write(files: WriteEntry[], opts?): Promise<WriteInfo[]>

  // 其他操作
  async list(path: string, opts?: { depth?: number }): Promise<EntryInfo[]>
  async makeDir(path: string, opts?): Promise<boolean>
  async rename(oldPath: string, newPath: string, opts?): Promise<EntryInfo>
  async remove(path: string, opts?): Promise<void>
  async exists(path: string, opts?): Promise<boolean>
  async getInfo(path: string, opts?): Promise<EntryInfo>
  async watchDir(path: string, onEvent: (event) => void, opts?): Promise<WatchHandle>
}
```

### Daytona 原始接口

```typescript
class FileSystem {
  async createFolder(path: string, mode: string): Promise<void>
  async deleteFile(path: string, recursive?: boolean): Promise<void>
  async downloadFile(remotePath: string): Promise<Buffer>
  async downloadFile(remotePath: string, localPath: string, timeout?: number): Promise<void>
  async downloadFiles(files: FileDownloadRequest[], timeoutSec?: number): Promise<FileDownloadResponse[]>
  async uploadFile(file: Buffer, remotePath: string, timeout?: number): Promise<void>
  async uploadFile(localPath: string, remotePath: string, timeout?: number): Promise<void>
  async uploadFiles(files: FileUpload[], timeout?: number): Promise<void>
  async findFiles(path: string, pattern: string): Promise<Array<Match>>  // grep
  async searchFiles(path: string, pattern: string): Promise<SearchFilesResponse>  // glob
  async getFileDetails(path: string): Promise<FileInfo>
  async listFiles(path: string): Promise<FileInfo[]>
  async moveFiles(source: string, destination: string): Promise<void>
  async replaceInFiles(files: string[], pattern: string, newValue: string): Promise<Array<ReplaceResult>>
  async setFilePermissions(path: string, permissions: FilePermissionsParams): Promise<void>
}
```

### 统一 SDK 设计

```typescript
interface FileSystemService {
  // 读取
  read(sandboxId: string, path: string, options?: ReadOptions): Promise<string | Uint8Array | ReadableStream>

  // 写入
  write(sandboxId: string, path: string, content: string | Uint8Array | ReadableStream, options?: WriteOptions): Promise<FileInfo>
  writeMultiple(sandboxId: string, entries: WriteEntry[]): Promise<FileInfo[]>

  // 目录操作
  mkdir(sandboxId: string, path: string, options?: MkdirOptions): Promise<void>
  list(sandboxId: string, path: string, options?: ListOptions): Promise<FileInfo[]>

  // 文件操作
  copy(sandboxId: string, source: string, destination: string): Promise<void>
  move(sandboxId: string, source: string, destination: string): Promise<void>
  remove(sandboxId: string, path: string, recursive?: boolean): Promise<void>
  getInfo(sandboxId: string, path: string): Promise<FileInfo>
  exists(sandboxId: string, path: string): Promise<boolean>

  // 搜索
  find(sandboxId: string, path: string, pattern: string): Promise<string[]>
  grep(sandboxId: string, path: string, pattern: string, options?: GrepOptions): Promise<GrepMatch[]>

  // 监听
  watch(sandboxId: string, path: string, callback: (event: FileEvent) => void, options?: WatchOptions): Promise<WatchHandle>
}
```

### 对比总结

| 功能 | E2B | Daytona | 统一设计 | 说明 |
|-----|-----|---------|---------|------|
| 读取文件 | `read()` 多格式 | `downloadFile()` | `read()` 多格式 | E2B 更优雅 |
| 写入文件 | `write()` | `uploadFile()` | `write()` | 命名来自 E2B |
| 批量写入 | `write(files[])` | `uploadFiles()` | `writeMultiple()` | 合并两者 |
| 创建目录 | `makeDir()` | `createFolder()` | `mkdir()` | 统一命名 |
| 列出目录 | `list()` | `listFiles()` | `list()` | ✓ |
| 重命名/移动 | `rename()` | `moveFiles()` | `move()` | 统一命名 |
| 删除 | `remove()` | `deleteFile()` | `remove()` | ✓ |
| 文件信息 | `getInfo()` | `getFileDetails()` | `getInfo()` | ✓ |
| 是否存在 | `exists()` | ✗ | `exists()` | 来自 E2B |
| 监听变化 | `watchDir()` | ✗ | `watch()` | 来自 E2B |
| 搜索文件名 | ✗ | `searchFiles()` | `find()` | 来自 Daytona |
| 搜索内容 | ✗ | `findFiles()` | `grep()` | 来自 Daytona |
| 批量替换 | ✗ | `replaceInFiles()` | ✗ | 未纳入 (复杂) |
| 设置权限 | ✗ | `setFilePermissions()` | ✗ | 未纳入 (少用) |

---

## 3. 进程执行

### E2B 原始接口

```typescript
class Commands {
  async run(cmd: string, opts?: CommandStartOpts & { background?: false }): Promise<CommandResult>
  async run(cmd: string, opts: CommandStartOpts & { background: true }): Promise<CommandHandle>
  async list(opts?): Promise<ProcessInfo[]>
  async sendStdin(pid: number, data: string, opts?): Promise<void>
  async kill(pid: number, opts?): Promise<boolean>
  async connect(pid: number, opts?: CommandConnectOpts): Promise<CommandHandle>
}

// CommandStartOpts
{
  background?: boolean
  cwd?: string
  user?: string
  envs?: Record<string, string>
  onStdout?: (data: string) => void
  onStderr?: (data: string) => void
  stdin?: boolean
  timeoutMs?: number
}

// CommandResult
{
  exitCode: number
  error?: string
  stdout: string
  stderr: string
}
```

### Daytona 原始接口

```typescript
class Process {
  // 基本执行
  async executeCommand(command: string, cwd?: string, env?: Record<string, string>, timeout?: number): Promise<ExecuteResponse>
  async codeRun(code: string, params?: CodeRunParams, timeout?: number): Promise<ExecuteResponse>

  // Session 管理
  async createSession(sessionId: string): Promise<void>
  async getSession(sessionId: string): Promise<Session>
  async listSessions(): Promise<Session[]>
  async deleteSession(sessionId: string): Promise<void>
  async executeSessionCommand(sessionId: string, req: SessionExecuteRequest, timeout?: number): Promise<SessionExecuteResponse>
  async getSessionCommandLogs(sessionId: string, commandId: string): Promise<SessionCommandLogsResponse>

  // PTY
  async createPty(options?: PtyCreateOptions & PtyConnectOptions): Promise<PtyHandle>
  async connectPty(sessionId: string, options?: PtyConnectOptions): Promise<PtyHandle>
  async listPtySessions(): Promise<PtySessionInfo[]>
  async getPtySessionInfo(sessionId: string): Promise<PtySessionInfo>
  async killPtySession(sessionId: string): Promise<void>
  async resizePtySession(sessionId: string, cols: number, rows: number): Promise<PtySessionInfo>
}

// CodeRunParams
{
  argv?: string[]
  env?: Record<string, string>
}
```

### 统一 SDK 设计

```typescript
interface ProcessService {
  // 命令执行
  run(sandboxId: string, command: string, options?: CommandOptions): Promise<CommandResult>
  spawn(sandboxId: string, command: string, options?: SpawnOptions): Promise<CommandHandle>

  // 进程管理
  list(sandboxId: string): Promise<ProcessInfo[]>
  kill(sandboxId: string, pid: number, signal?: string): Promise<boolean>
  sendInput(sandboxId: string, pid: number, data: string): Promise<void>

  // 代码执行
  runCode(sandboxId: string, code: string, language: string, options?: CodeRunOptions): Promise<CommandResult>

  // Session 管理
  createSession(sandboxId: string, sessionId?: string): Promise<Session>
  getSession(sandboxId: string, sessionId: string): Promise<Session>
  listSessions(sandboxId: string): Promise<Session[]>
  closeSession(sandboxId: string, sessionId: string): Promise<void>
  executeInSession(sandboxId: string, sessionId: string, command: string, options?): Promise<CommandResult>
}

interface PTYService {
  create(sandboxId: string, config?: PTYConfig): Promise<PTYHandle>
  connect(sandboxId: string, ptyId: string): Promise<PTYHandle>
  list(sandboxId: string): Promise<PTYInfo[]>
  kill(sandboxId: string, ptyId: string): Promise<void>
  resize(sandboxId: string, ptyId: string, cols: number, rows: number): Promise<void>
  write(sandboxId: string, ptyId: string, data: string | Uint8Array): Promise<void>
}
```

### 对比总结

| 功能 | E2B | Daytona | 统一设计 | 说明 |
|-----|-----|---------|---------|------|
| 执行命令 | `run()` | `executeCommand()` | `run()` | E2B 更简洁 |
| 后台执行 | `run({background:true})` | Session 模式 | `spawn()` | 统一命名 |
| 进程列表 | `list()` | ✗ | `list()` | 来自 E2B |
| 杀死进程 | `kill()` | ✗ | `kill()` | 来自 E2B |
| 写入 stdin | `sendStdin()` | ✗ | `sendInput()` | 来自 E2B |
| 连接进程 | `connect()` | ✗ | ✗ | 简化设计 |
| 执行代码 | ✗ | `codeRun()` | `runCode()` | 来自 Daytona |
| Session | ✗ | `createSession()` 等 | `createSession()` 等 | 来自 Daytona |
| PTY 创建 | `pty.create()` | `createPty()` | `pty.create()` | 两者都有 |
| PTY 调整 | `pty.resize()` | `resizePtySession()` | `pty.resize()` | ✓ |

---

## 4. Git 操作

### E2B 原始接口

**E2B 不提供 Git 支持**

### Daytona 原始接口

```typescript
class Git {
  async clone(url: string, path: string, branch?: string, commitId?: string, username?: string, password?: string): Promise<void>
  async status(path: string): Promise<GitStatus>
  async add(path: string, files: string[]): Promise<void>
  async commit(path: string, message: string, author: string, email: string, allowEmpty?: boolean): Promise<GitCommitResponse>
  async push(path: string, username?: string, password?: string): Promise<void>
  async pull(path: string, username?: string, password?: string): Promise<void>
  async branches(path: string): Promise<ListBranchResponse>
  async createBranch(path: string, name: string): Promise<void>
  async deleteBranch(path: string, name: string): Promise<void>
  async checkoutBranch(path: string, branch: string): Promise<void>
}
```

### 统一 SDK 设计

```typescript
interface GitService {
  // 仓库初始化
  clone(sandboxId: string, url: string, path: string, options?: CloneOptions): Promise<void>
  init(sandboxId: string, path: string): Promise<void>

  // 状态查询
  status(sandboxId: string, path: string): Promise<GitStatus>
  log(sandboxId: string, path: string, options?: LogOptions): Promise<CommitInfo[]>
  diff(sandboxId: string, path: string, options?: DiffOptions): Promise<string>

  // 分支操作
  branches(sandboxId: string, path: string): Promise<BranchInfo[]>
  createBranch(sandboxId: string, path: string, name: string, startPoint?: string): Promise<void>
  deleteBranch(sandboxId: string, path: string, name: string, force?: boolean): Promise<void>
  checkout(sandboxId: string, path: string, ref: string, options?: CheckoutOptions): Promise<void>

  // 提交操作
  add(sandboxId: string, path: string, files: string[]): Promise<void>
  commit(sandboxId: string, path: string, options: CommitOptions): Promise<CommitInfo>
  reset(sandboxId: string, path: string, options?: ResetOptions): Promise<void>

  // 远程操作
  push(sandboxId: string, path: string, options?: PushOptions): Promise<void>
  pull(sandboxId: string, path: string, options?: PullOptions): Promise<void>
  merge(sandboxId: string, path: string, branch: string, options?: MergeOptions): Promise<MergeResult>
}
```

### 对比总结

| 功能 | E2B | Daytona | 统一设计 | 说明 |
|-----|-----|---------|---------|------|
| clone | ✗ | `clone()` | `clone()` | 来自 Daytona |
| init | ✗ | ✗ | `init()` | **新增** |
| status | ✗ | `status()` | `status()` | 来自 Daytona |
| log | ✗ | ✗ | `log()` | **新增** |
| diff | ✗ | ✗ | `diff()` | **新增** |
| add | ✗ | `add()` | `add()` | 来自 Daytona |
| commit | ✗ | `commit()` | `commit()` | 来自 Daytona |
| push | ✗ | `push()` | `push()` | 来自 Daytona |
| pull | ✗ | `pull()` | `pull()` | 来自 Daytona |
| merge | ✗ | ✗ | `merge()` | **新增** |
| reset | ✗ | ✗ | `reset()` | **新增** |
| branches | ✗ | `branches()` | `branches()` | 来自 Daytona |
| checkout | ✗ | `checkoutBranch()` | `checkout()` | 来自 Daytona |

---

## 5. LSP 语言服务

### E2B 原始接口

**E2B 不提供 LSP 支持**

### Daytona 原始接口

```typescript
class LspServer {
  async start(): Promise<void>
  async stop(): Promise<void>
  async didOpen(path: string): Promise<void>
  async didClose(path: string): Promise<void>
  async documentSymbols(path: string): Promise<LspSymbol[]>
  async workspaceSymbols(query: string): Promise<LspSymbol[]>  // deprecated
  async sandboxSymbols(query: string): Promise<LspSymbol[]>
  async completions(path: string, position: Position): Promise<CompletionList>
}

// 创建方式
sandbox.createLspServer(languageId: LspLanguageId | string, pathToProject: string): Promise<LspServer>

enum LspLanguageId {
  PYTHON = 'python',
  TYPESCRIPT = 'typescript',
  JAVASCRIPT = 'javascript'
}
```

### 统一 SDK 设计

```typescript
interface LSPService {
  start(sandboxId: string, language: string, rootPath: string): Promise<LSPServerHandle>
  stop(sandboxId: string, serverId: string): Promise<void>
  list(sandboxId: string): Promise<LSPServerInfo[]>
}

interface LSPServerHandle {
  // 文档生命周期
  didOpen(path: string): Promise<void>
  didClose(path: string): Promise<void>
  didChange(path: string, changes: TextChange[]): Promise<void>

  // 符号查询
  documentSymbols(path: string): Promise<Symbol[]>
  workspaceSymbols(query: string): Promise<Symbol[]>

  // 代码智能
  completion(path: string, position: Position): Promise<CompletionItem[]>
  hover(path: string, position: Position): Promise<HoverInfo | null>
  definition(path: string, position: Position): Promise<Location[]>
  references(path: string, position: Position, includeDeclaration?: boolean): Promise<Location[]>

  // 诊断和操作
  diagnostics(path: string): Promise<Diagnostic[]>
  rename(path: string, position: Position, newName: string): Promise<WorkspaceEdit>
  codeAction(path: string, range: Range, diagnostics?: Diagnostic[]): Promise<CodeAction[]>
  format(path: string, options?: FormatOptions): Promise<TextEdit[]>
}
```

### 对比总结

| 功能 | E2B | Daytona | 统一设计 | 说明 |
|-----|-----|---------|---------|------|
| 启动服务器 | ✗ | `createLspServer()` + `start()` | `lsp.start()` | 简化 |
| 停止服务器 | ✗ | `stop()` | `lsp.stop()` | ✓ |
| didOpen | ✗ | `didOpen()` | `didOpen()` | ✓ |
| didClose | ✗ | `didClose()` | `didClose()` | ✓ |
| didChange | ✗ | ✗ | `didChange()` | **新增** |
| documentSymbols | ✗ | `documentSymbols()` | `documentSymbols()` | ✓ |
| workspaceSymbols | ✗ | `sandboxSymbols()` | `workspaceSymbols()` | 重命名 |
| completion | ✗ | `completions()` | `completion()` | ✓ |
| hover | ✗ | ✗ | `hover()` | **新增** |
| definition | ✗ | ✗ | `definition()` | **新增** |
| references | ✗ | ✗ | `references()` | **新增** |
| diagnostics | ✗ | ✗ | `diagnostics()` | **新增** |
| rename | ✗ | ✗ | `rename()` | **新增** |
| codeAction | ✗ | ✗ | `codeAction()` | **新增** |
| format | ✗ | ✗ | `format()` | **新增** |

---

## 6. 快照和卷

### E2B 原始接口

**E2B 不提供快照和卷支持**

### Daytona 原始接口

```typescript
// Snapshot
class Snapshot {
  async list(page?: number, limit?: number): Promise<PaginatedSnapshots>
  async get(name: string): Promise<Snapshot>
  async create(params: CreateSnapshotParams, options?): Promise<Snapshot>
  async delete(snapshot: Snapshot): Promise<void>
  async activate(snapshot: Snapshot): Promise<Snapshot>
}

// Volume
class Volume {
  async list(): Promise<Volume[]>
  async get(name: string, create?: boolean): Promise<Volume>
  async create(name: string): Promise<Volume>
  async delete(volume: Volume): Promise<void>
}
```

### 统一 SDK 设计

```typescript
interface SnapshotService {
  create(sandboxId: string, params: CreateSnapshotParams): Promise<SnapshotInfo>
  get(snapshotId: string): Promise<SnapshotInfo>
  list(params?: ListSnapshotParams): Promise<PaginatedResult<SnapshotInfo>>
  delete(snapshotId: string): Promise<void>
}

interface VolumeService {
  create(params: CreateVolumeParams): Promise<VolumeInfo>
  get(volumeId: string): Promise<VolumeInfo>
  getOrCreate(name: string, params?: CreateVolumeParams): Promise<VolumeInfo>
  list(params?: ListVolumeParams): Promise<PaginatedResult<VolumeInfo>>
  delete(volumeId: string): Promise<void>
  resize(volumeId: string, sizeGB: number): Promise<VolumeInfo>
}
```

### 对比总结

| 功能 | E2B | Daytona | 统一设计 | 说明 |
|-----|-----|---------|---------|------|
| 创建快照 | ✗ | `create()` | `create()` | 来自 Daytona |
| 获取快照 | ✗ | `get()` | `get()` | ✓ |
| 列出快照 | ✗ | `list()` | `list()` | ✓ |
| 删除快照 | ✗ | `delete()` | `delete()` | ✓ |
| 激活快照 | ✗ | `activate()` | ✗ (创建时指定) | 简化 |
| 创建卷 | ✗ | `create()` | `create()` | ✓ |
| 获取卷 | ✗ | `get()` | `get()` | ✓ |
| 获取或创建 | ✗ | `get(name, true)` | `getOrCreate()` | 显式方法 |
| 列出卷 | ✗ | `list()` | `list()` | ✓ |
| 删除卷 | ✗ | `delete()` | `delete()` | ✓ |
| 调整大小 | ✗ | ✗ | `resize()` | **新增** |

---

## 7. 其他功能对比

### E2B 独有功能

| 功能 | 方法 | 是否纳入统一设计 |
|-----|------|----------------|
| 获取主机地址 | `getHost(port)` | ✗ (使用 URL 模式) |
| 上传 URL | `uploadUrl(path)` | ✗ |
| 下载 URL | `downloadUrl(path)` | ✗ |
| MCP URL | `getMcpUrl()` | ✗ |
| MCP Token | `getMcpToken()` | ✗ |

### Daytona 独有功能

| 功能 | 方法 | 是否纳入统一设计 |
|-----|------|----------------|
| 桌面自动化 | `computerUse.*` | ✗ (特殊场景) |
| 代码解释器 | `codeInterpreter.*` | 部分 (合并到 runCode) |
| SSH 访问 | `createSshAccess()` | ✗ |
| 预览链接 | `getPreviewLink()` | ✗ |
| 自动停止间隔 | `setAutostopInterval()` | ✗ (创建时配置) |
| 文件权限 | `setFilePermissions()` | ✗ |
| 批量替换 | `replaceInFiles()` | ✗ |

---

## 8. 命名规范对比

| 概念 | E2B | Daytona | 统一设计 |
|-----|-----|---------|---------|
| 沙盒 | `Sandbox` | `Sandbox` | `Sandbox` |
| 删除沙盒 | `kill()` | `delete()` | `delete()` |
| 文件系统 | `files` / `Filesystem` | `fs` / `FileSystem` | `fs` / `FileSystem` |
| 读文件 | `read()` | `downloadFile()` | `read()` |
| 写文件 | `write()` | `uploadFile()` | `write()` |
| 创建目录 | `makeDir()` | `createFolder()` | `mkdir()` |
| 命令执行 | `commands` | `process` | `process` |
| 后台执行 | `background: true` | Session | `spawn()` |
| PTY | `pty` (属性) | `process.createPty()` | `pty` (服务) |
| 进程 ID | `pid` | `sessionId` | `pid` / `ptyId` |

---

## 9. 结论

统一 SDK 设计：

1. **来自 E2B 的设计**：
   - 文件读写的多格式支持 (`text/bytes/stream`)
   - 简洁的方法命名 (`read/write/run`)
   - 文件监听 (`watch`)
   - 进程列表和管理
   - 沙盒指标 (`getMetrics`)
   - 暂停/恢复功能

2. **来自 Daytona 的设计**：
   - 沙盒生命周期完整状态 (`start/stop`)
   - 文件搜索功能 (`find/grep`)
   - Session 会话管理
   - Git 完整操作
   - LSP 语言服务
   - 快照和卷管理
   - 代码执行 (`runCode`)

3. **新增/改进的设计**：
   - Git: `init`, `log`, `diff`, `merge`, `reset`
   - LSP: `hover`, `definition`, `references`, `diagnostics`, `rename`, `codeAction`, `format`
   - Volume: `resize`
   - 统一的错误码体系
   - 完整的 REST API 规范
   - TypeScript 和 Python 双语言 SDK

4. **未纳入的功能**：
   - Daytona 的桌面自动化 (ComputerUse) - 特殊场景
   - E2B 的 MCP 相关功能 - 特定用途
   - 文件权限设置 - 使用较少
   - 批量文本替换 - 可用命令替代
