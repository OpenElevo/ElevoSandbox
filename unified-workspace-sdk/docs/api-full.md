# 统一 SDK API - 完整版

包含 E2B 和 Daytona **所有功能**的并集，不做额外扩展。

---

## 1. Sandbox 服务

```typescript
interface SandboxService {
  // ========== 两者都有 ==========
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
  list(params?: ListParams): Promise<PaginatedResult<Sandbox>>

  /**
   * 删除 Sandbox
   */
  delete(sandboxId: string): Promise<void>

  // ========== 来自 Daytona ==========
  /**
   * 启动 Sandbox
   */
  start(sandboxId: string, timeout?: number): Promise<void>

  /**
   * 停止 Sandbox
   */
  stop(sandboxId: string, timeout?: number): Promise<void>

  /**
   * 归档 Sandbox
   */
  archive(sandboxId: string): Promise<void>

  /**
   * 恢复 Sandbox
   */
  recover(sandboxId: string, timeout?: number): Promise<void>

  /**
   * 设置标签
   */
  setLabels(sandboxId: string, labels: Record<string, string>): Promise<Record<string, string>>

  /**
   * 设置自动停止间隔
   */
  setAutostopInterval(sandboxId: string, interval: number): Promise<void>

  /**
   * 获取预览链接
   */
  getPreviewLink(sandboxId: string, port: number): Promise<string>

  // ========== 来自 E2B ==========
  /**
   * 暂停 Sandbox (beta)
   */
  pause(sandboxId: string): Promise<boolean>

  /**
   * 设置超时时间
   */
  setTimeout(sandboxId: string, timeoutMs: number): Promise<void>

  /**
   * 检查是否运行中
   */
  isRunning(sandboxId: string): Promise<boolean>

  /**
   * 获取指标
   */
  getMetrics(sandboxId: string): Promise<SandboxMetrics[]>

  /**
   * 获取主机地址
   */
  getHost(sandboxId: string, port: number): string

  /**
   * 获取上传 URL
   */
  uploadUrl(sandboxId: string, path?: string): Promise<string>

  /**
   * 获取下载 URL
   */
  downloadUrl(sandboxId: string, path: string): Promise<string>
}

interface CreateSandboxParams {
  // 两者都有
  name?: string
  envs?: Record<string, string>

  // E2B: template, Daytona: image/snapshot
  template?: string
  image?: string
  snapshot?: string

  // 来自 Daytona
  user?: string
  labels?: Record<string, string>
  public?: boolean
  autoStopInterval?: number
  autoArchiveInterval?: number
  autoDeleteInterval?: number
  volumes?: VolumeMount[]
  resources?: Resources

  // 来自 E2B
  metadata?: Record<string, string>
  timeoutMs?: number
}

interface Sandbox {
  id: string
  name?: string
  state: 'running' | 'stopped' | 'paused' | 'creating' | 'archiving' | 'archived'
  createdAt: Date

  // 来自 E2B
  templateId?: string
  metadata?: Record<string, string>
  endAt?: Date
  cpuCount?: number
  memoryMB?: number

  // 来自 Daytona
  image?: string
  user?: string
  labels?: Record<string, string>
  resources?: Resources
}

interface SandboxMetrics {
  timestamp: Date
  cpuUsedPct: number
  cpuCount: number
  memUsed: number
  memTotal: number
  diskUsed: number
  diskTotal: number
}

interface Resources {
  cpu?: number
  gpu?: number
  memory?: number
  disk?: number
}

interface VolumeMount {
  volumeId: string
  mountPath: string
}

interface ListParams {
  page?: number
  limit?: number
  labels?: Record<string, string>
}

interface PaginatedResult<T> {
  data: T[]
  total: number
  page: number
  limit: number
  hasMore: boolean
}
```

---

## 2. FileSystem 服务

```typescript
interface FileSystem {
  // ========== 两者都有 ==========
  /**
   * 读取文件
   * E2B 支持: text, bytes, blob, stream
   * Daytona 支持: Buffer
   */
  read(path: string, options?: ReadOptions): Promise<string | Uint8Array | Blob | ReadableStream>

  /**
   * 写入文件
   */
  write(path: string, content: string | Uint8Array | Blob | ReadableStream): Promise<void>

  /**
   * 批量写入
   */
  writeMultiple(files: WriteEntry[]): Promise<void>

  /**
   * 创建目录
   */
  mkdir(path: string, mode?: string): Promise<void>

  /**
   * 列出目录
   */
  list(path: string, options?: ListOptions): Promise<FileInfo[]>

  /**
   * 删除文件/目录
   */
  remove(path: string, recursive?: boolean): Promise<void>

  /**
   * 移动/重命名
   */
  move(source: string, destination: string): Promise<void>

  /**
   * 获取文件信息
   */
  getInfo(path: string): Promise<FileInfo>

  // ========== 来自 E2B ==========
  /**
   * 检查是否存在
   */
  exists(path: string): Promise<boolean>

  /**
   * 监听目录变化
   */
  watch(path: string, onEvent: (event: FileEvent) => void, options?: WatchOptions): Promise<WatchHandle>

  // ========== 来自 Daytona ==========
  /**
   * 下载文件到本地
   */
  downloadFile(remotePath: string, localPath: string, timeout?: number): Promise<void>

  /**
   * 批量下载
   */
  downloadFiles(files: FileDownloadRequest[], timeout?: number): Promise<FileDownloadResponse[]>

  /**
   * 上传本地文件
   */
  uploadFile(localPath: string, remotePath: string, timeout?: number): Promise<void>

  /**
   * 批量上传
   */
  uploadFiles(files: FileUpload[], timeout?: number): Promise<void>

  /**
   * 搜索文件名 (glob)
   */
  searchFiles(path: string, pattern: string): Promise<SearchFilesResponse>

  /**
   * 搜索文件内容 (grep)
   */
  findFiles(path: string, pattern: string): Promise<Match[]>

  /**
   * 批量替换内容
   */
  replaceInFiles(files: string[], pattern: string, newValue: string): Promise<ReplaceResult[]>

  /**
   * 设置文件权限
   */
  setPermissions(path: string, permissions: FilePermissions): Promise<void>
}

interface ReadOptions {
  format?: 'text' | 'bytes' | 'blob' | 'stream'
  user?: string
}

interface ListOptions {
  depth?: number  // 来自 E2B
  user?: string
}

interface WriteEntry {
  path: string
  data: string | Uint8Array | Blob | ReadableStream
}

interface FileInfo {
  name: string
  path: string
  type: 'file' | 'directory'
  size: number

  // 来自 E2B
  mode?: number
  permissions?: string
  owner?: string
  group?: string
  modifiedTime?: Date
  symlinkTarget?: string
}

interface FileEvent {
  name: string
  type: 'chmod' | 'create' | 'remove' | 'rename' | 'write'
}

interface WatchOptions {
  recursive?: boolean
  timeoutMs?: number
}

interface WatchHandle {
  stop(): Promise<void>
}

interface FileDownloadRequest {
  source: string
  destination?: string
}

interface FileDownloadResponse {
  source: string
  content?: Buffer
}

interface FileUpload {
  source: string | Buffer
  destination: string
}

interface SearchFilesResponse {
  files: string[]
}

interface Match {
  file: string
  line: number
  content: string
}

interface ReplaceResult {
  file: string
  replacements: number
}

interface FilePermissions {
  mode?: string
  owner?: string
  group?: string
}
```

---

## 3. Process 服务

```typescript
interface Process {
  // ========== 两者都有 ==========
  /**
   * 执行命令（同步等待结果）
   */
  run(command: string, options?: RunOptions): Promise<CommandResult>

  // ========== 来自 E2B ==========
  /**
   * 启动后台进程
   */
  spawn(command: string, options?: SpawnOptions): Promise<CommandHandle>

  /**
   * 列出进程
   */
  list(): Promise<ProcessInfo[]>

  /**
   * 杀死进程
   */
  kill(pid: number): Promise<boolean>

  /**
   * 发送 stdin
   */
  sendStdin(pid: number, data: string): Promise<void>

  /**
   * 连接到已有进程
   */
  connect(pid: number, options?: ConnectOptions): Promise<CommandHandle>

  // ========== 来自 Daytona ==========
  /**
   * 执行代码
   */
  codeRun(code: string, params?: CodeRunParams, timeout?: number): Promise<CommandResult>

  /**
   * 创建 Session
   */
  createSession(sessionId: string): Promise<void>

  /**
   * 获取 Session
   */
  getSession(sessionId: string): Promise<Session>

  /**
   * 列出 Sessions
   */
  listSessions(): Promise<Session[]>

  /**
   * 删除 Session
   */
  deleteSession(sessionId: string): Promise<void>

  /**
   * 在 Session 中执行命令
   */
  executeSessionCommand(sessionId: string, req: SessionExecuteRequest, timeout?: number): Promise<SessionExecuteResponse>

  /**
   * 获取 Session 命令
   */
  getSessionCommand(sessionId: string, commandId: string): Promise<Command>

  /**
   * 获取 Session 命令日志
   */
  getSessionCommandLogs(sessionId: string, commandId: string): Promise<SessionCommandLogsResponse>
}

interface RunOptions {
  cwd?: string
  envs?: Record<string, string>
  timeout?: number
  user?: string  // 来自 E2B
}

interface SpawnOptions extends RunOptions {
  onStdout?: (data: string) => void
  onStderr?: (data: string) => void
  stdin?: boolean
}

interface ConnectOptions {
  onStdout?: (data: string) => void
  onStderr?: (data: string) => void
  timeoutMs?: number
}

interface CommandResult {
  exitCode: number
  stdout: string
  stderr: string
  error?: string  // 来自 E2B
}

interface CommandHandle {
  pid: number
  stdout: string
  stderr: string
  exitCode?: number

  wait(): Promise<CommandResult>
  disconnect(): Promise<void>
  kill(): Promise<boolean>
}

interface ProcessInfo {
  pid: number
  cmd: string
  args: string[]
  envs: Record<string, string>
  cwd?: string
  tag?: string
}

interface CodeRunParams {
  argv?: string[]
  env?: Record<string, string>
}

interface Session {
  id: string
  // Daytona session 结构
}

interface SessionExecuteRequest {
  command: string
  runAsync?: boolean
}

interface SessionExecuteResponse {
  commandId: string
  output?: string
}

interface Command {
  id: string
  command: string
  exitCode?: number
}

interface SessionCommandLogsResponse {
  stdout: string
  stderr: string
}
```

---

## 4. PTY 服务

```typescript
interface PTY {
  // ========== 两者都有 ==========
  /**
   * 创建 PTY
   */
  create(options: PTYCreateOptions): Promise<PTYHandle>

  /**
   * 调整大小
   */
  resize(ptyId: string, cols: number, rows: number): Promise<void>

  /**
   * 关闭 PTY
   */
  kill(ptyId: string): Promise<void>

  // ========== 来自 E2B ==========
  /**
   * 连接到已有 PTY
   */
  connect(pid: number, options?: PTYConnectOptions): Promise<PTYHandle>

  /**
   * 发送输入
   */
  sendInput(pid: number, data: Uint8Array): Promise<void>

  // ========== 来自 Daytona ==========
  /**
   * 列出 PTY Sessions
   */
  listSessions(): Promise<PTYSessionInfo[]>

  /**
   * 获取 PTY Session 信息
   */
  getSessionInfo(sessionId: string): Promise<PTYSessionInfo>
}

interface PTYCreateOptions {
  cols: number
  rows: number
  onData: (data: Uint8Array) => void

  // 来自 E2B
  timeoutMs?: number
  user?: string
  envs?: Record<string, string>
  cwd?: string
}

interface PTYConnectOptions {
  onData: (data: Uint8Array) => void
  timeoutMs?: number
}

interface PTYHandle {
  id: string
  write(data: Uint8Array): Promise<void>
  resize(cols: number, rows: number): Promise<void>
  kill(): Promise<void>
}

interface PTYSessionInfo {
  id: string
  cols: number
  rows: number
}
```

---

## 5. Git 服务 (仅 Daytona)

```typescript
interface Git {
  /**
   * 克隆仓库
   */
  clone(url: string, path: string, options?: CloneOptions): Promise<void>

  /**
   * 获取状态
   */
  status(path: string): Promise<GitStatus>

  /**
   * 暂存文件
   */
  add(path: string, files: string[]): Promise<void>

  /**
   * 提交
   */
  commit(path: string, message: string, author: string, email: string, allowEmpty?: boolean): Promise<GitCommitResponse>

  /**
   * 推送
   */
  push(path: string, username?: string, password?: string): Promise<void>

  /**
   * 拉取
   */
  pull(path: string, username?: string, password?: string): Promise<void>

  /**
   * 列出分支
   */
  branches(path: string): Promise<ListBranchResponse>

  /**
   * 创建分支
   */
  createBranch(path: string, name: string): Promise<void>

  /**
   * 删除分支
   */
  deleteBranch(path: string, name: string): Promise<void>

  /**
   * 切换分支
   */
  checkoutBranch(path: string, branch: string): Promise<void>
}

interface CloneOptions {
  branch?: string
  commitId?: string
  username?: string
  password?: string
}

interface GitStatus {
  currentBranch: string
  files: GitFileStatus[]
}

interface GitFileStatus {
  path: string
  status: string
}

interface GitCommitResponse {
  sha: string
}

interface ListBranchResponse {
  branches: BranchInfo[]
}

interface BranchInfo {
  name: string
  isCurrent: boolean
  isRemote: boolean
}
```

---

## 6. LSP 服务 (仅 Daytona)

```typescript
interface LSPService {
  /**
   * 创建 LSP Server
   */
  createServer(languageId: string, pathToProject: string): Promise<LspServer>
}

interface LspServer {
  /**
   * 启动
   */
  start(): Promise<void>

  /**
   * 停止
   */
  stop(): Promise<void>

  /**
   * 通知文件打开
   */
  didOpen(path: string): Promise<void>

  /**
   * 通知文件关闭
   */
  didClose(path: string): Promise<void>

  /**
   * 获取文档符号
   */
  documentSymbols(path: string): Promise<LspSymbol[]>

  /**
   * 搜索工作区符号
   */
  workspaceSymbols(query: string): Promise<LspSymbol[]>

  /**
   * 获取代码补全
   */
  completions(path: string, position: Position): Promise<CompletionList>
}

type LspLanguageId = 'python' | 'typescript' | 'javascript'

interface Position {
  line: number
  character: number
}

interface LspSymbol {
  name: string
  kind: number
  location: Location
}

interface Location {
  uri: string
  range: Range
}

interface Range {
  start: Position
  end: Position
}

interface CompletionList {
  isIncomplete: boolean
  items: CompletionItem[]
}

interface CompletionItem {
  label: string
  kind?: number
  detail?: string
  insertText?: string
}
```

---

## 7. Snapshot 服务 (仅 Daytona)

```typescript
interface SnapshotService {
  /**
   * 创建快照
   */
  create(params: CreateSnapshotParams, options?: SnapshotOptions): Promise<Snapshot>

  /**
   * 获取快照
   */
  get(name: string): Promise<Snapshot>

  /**
   * 列出快照
   */
  list(page?: number, limit?: number): Promise<PaginatedSnapshots>

  /**
   * 删除快照
   */
  delete(snapshot: Snapshot): Promise<void>

  /**
   * 激活快照
   */
  activate(snapshot: Snapshot): Promise<Snapshot>
}

interface CreateSnapshotParams {
  sandboxId: string
  name?: string
}

interface SnapshotOptions {
  onLogs?: (chunk: string) => void
  timeout?: number
}

interface Snapshot {
  id: string
  name: string
  sandboxId: string
  createdAt: Date
  state: string
}

interface PaginatedSnapshots {
  data: Snapshot[]
  total: number
  page: number
  limit: number
}
```

---

## 8. Volume 服务 (仅 Daytona)

```typescript
interface VolumeService {
  /**
   * 创建卷
   */
  create(name: string): Promise<Volume>

  /**
   * 获取卷
   */
  get(name: string, create?: boolean): Promise<Volume>

  /**
   * 列出卷
   */
  list(): Promise<Volume[]>

  /**
   * 删除卷
   */
  delete(volume: Volume): Promise<void>
}

interface Volume {
  id: string
  name: string
  createdAt: Date
  state: string
}
```

---

## 9. CodeInterpreter 服务 (仅 Daytona)

```typescript
interface CodeInterpreter {
  /**
   * 运行代码
   */
  runCode(code: string, options?: RunCodeOptions): Promise<ExecutionResult>

  /**
   * 创建上下文
   */
  createContext(cwd?: string): Promise<InterpreterContext>

  /**
   * 列出上下文
   */
  listContexts(): Promise<InterpreterContext[]>

  /**
   * 删除上下文
   */
  deleteContext(context: InterpreterContext): Promise<void>
}

interface RunCodeOptions {
  context?: InterpreterContext
}

interface ExecutionResult {
  output: string
  error?: string
}

interface InterpreterContext {
  id: string
  cwd: string
}
```

---

## 10. ComputerUse 服务 (仅 Daytona)

```typescript
interface ComputerUse {
  mouse: Mouse
  keyboard: Keyboard
  screenshot: Screenshot
  display: Display

  start(): Promise<ComputerUseStartResponse>
  stop(): Promise<ComputerUseStopResponse>
  getStatus(): Promise<ComputerUseStatusResponse>
  getProcessStatus(processName: string): Promise<ProcessStatusResponse>
  restartProcess(processName: string): Promise<ProcessRestartResponse>
  getProcessLogs(processName: string): Promise<ProcessLogsResponse>
  getProcessErrors(processName: string): Promise<ProcessErrorsResponse>
}

interface Mouse {
  getPosition(): Promise<MousePositionResponse>
  move(x: number, y: number): Promise<MousePositionResponse>
  click(x: number, y: number, button?: string, double?: boolean): Promise<MouseClickResponse>
  drag(startX: number, startY: number, endX: number, endY: number, button?: string): Promise<MouseDragResponse>
  scroll(x: number, y: number, direction: 'up' | 'down', amount?: number): Promise<boolean>
}

interface Keyboard {
  type(text: string, delay?: number): Promise<void>
  press(key: string, modifiers?: string[]): Promise<void>
  hotkey(keys: string): Promise<void>
}

interface Screenshot {
  takeFullScreen(showCursor?: boolean): Promise<ScreenshotResponse>
  takeRegion(region: ScreenshotRegion, showCursor?: boolean): Promise<ScreenshotResponse>
  takeCompressed(options?: ScreenshotOptions): Promise<ScreenshotResponse>
  takeCompressedRegion(region: ScreenshotRegion, options?: ScreenshotOptions): Promise<ScreenshotResponse>
}

interface Display {
  getInfo(): Promise<DisplayInfoResponse>
  getWindows(): Promise<WindowsResponse>
}

interface ScreenshotRegion {
  x: number
  y: number
  width: number
  height: number
}

interface ScreenshotOptions {
  quality?: number
  format?: string
}

interface ScreenshotResponse {
  data: string  // base64
  width: number
  height: number
}
```

---

## 11. SSH 访问 (仅 Daytona)

```typescript
// Sandbox 实例方法
interface Sandbox {
  /**
   * 创建 SSH 访问令牌
   */
  createSshAccess(expiresInMinutes?: number): Promise<SshAccessDto>

  /**
   * 撤销 SSH 访问令牌
   */
  revokeSshAccess(token: string): Promise<void>

  /**
   * 验证 SSH 访问令牌
   */
  validateSshAccess(token: string): Promise<SshAccessValidationDto>
}

interface SshAccessDto {
  token: string
  expiresAt: Date
}

interface SshAccessValidationDto {
  valid: boolean
  sandboxId?: string
}
```

---

## 总结

| 服务 | 来源 | 方法数 |
|-----|------|-------|
| Sandbox | 两者 | 17 |
| FileSystem | 两者 | 17 |
| Process | 两者 | 14 |
| PTY | 两者 | 7 |
| Git | Daytona | 10 |
| LSP | Daytona | 7 |
| Snapshot | Daytona | 5 |
| Volume | Daytona | 4 |
| CodeInterpreter | Daytona | 4 |
| ComputerUse | Daytona | ~20 |
| SSH | Daytona | 3 |

**完整版共约 100+ 个方法**，涵盖了 E2B 和 Daytona 的全部公开 API。
