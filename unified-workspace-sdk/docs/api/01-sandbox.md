# Sandbox 服务接口文档

Sandbox 服务是 Unified Workspace SDK 的核心服务，提供 Sandbox 实例的完整生命周期管理功能。

---

## 目录

- [1. 概述](#1-概述)
- [2. 数据类型](#2-数据类型)
- [3. SandboxService 接口](#3-sandboxservice-接口)
- [4. Sandbox 实例接口](#4-sandbox-实例接口)
- [5. REST API](#5-rest-api)
- [6. 使用示例](#6-使用示例)
- [7. 错误处理](#7-错误处理)

---

## 1. 概述

### 1.1 功能说明

Sandbox 服务提供以下核心功能：

| 功能 | 描述 |
|------|------|
| 创建 Sandbox | 基于模板或快照创建新的 Sandbox 实例 |
| 生命周期管理 | 启动、停止、暂停、恢复、删除 Sandbox |
| 状态查询 | 获取 Sandbox 信息、运行状态、性能指标 |
| 配置管理 | 更新标签、超时设置等配置 |
| 网络访问 | 获取服务端口的访问地址 |

### 1.2 架构图

```
┌─────────────────────────────────────────────────────────────┐
│                      WorkspaceClient                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                    SandboxService                        ││
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐       ││
│  │  │ create  │ │  list   │ │  start  │ │  stop   │       ││
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘       ││
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐       ││
│  │  │  pause  │ │ resume  │ │ delete  │ │ getInfo │       ││
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘       ││
│  └─────────────────────────────────────────────────────────┘│
│                              │                               │
│                              ▼                               │
│  ┌─────────────────────────────────────────────────────────┐│
│  │                       Sandbox                            ││
│  │  ┌────┐ ┌─────────┐ ┌─────┐ ┌─────┐ ┌─────┐            ││
│  │  │ fs │ │ process │ │ git │ │ lsp │ │ pty │            │��
│  │  └────┘ └─────────┘ └─────┘ └─────┘ └─────┘            ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 数据类型

### 2.1 CreateSandboxParams

创建 Sandbox 的参数。

```typescript
/**
 * 创建 Sandbox 参数
 */
interface CreateSandboxParams {
  // ==================== 基础配置 ====================

  /**
   * Sandbox 名称
   * - 用于标识和引用 Sandbox
   * - 在同一组织内必须唯一
   * - 如不提供，系统自动生成
   *
   * @pattern ^[a-z0-9][a-z0-9-]*[a-z0-9]$
   * @minLength 3
   * @maxLength 63
   * @example "my-dev-sandbox"
   */
  name?: string

  /**
   * 模板/镜像标识
   * - 指定 Sandbox 的基础运行环境
   * - 支持内置模板或自定义镜像
   *
   * @example "python:3.11"
   * @example "node:20-slim"
   * @example "custom/my-image:v1.0"
   */
  template?: string

  /**
   * 快照标识
   * - 从已有快照创建 Sandbox
   * - 与 template 互斥，优先使用 snapshot
   *
   * @example "snap-abc123"
   */
  snapshot?: string

  // ==================== 环境配置 ====================

  /**
   * 环境变量
   * - 注入到 Sandbox 运行环境中
   * - 支持覆盖模板默认环境变量
   *
   * @example {"NODE_ENV": "development", "DEBUG": "true"}
   */
  env?: Record<string, string>

  /**
   * 默认工作目录
   * - 命令执行的默认路径
   * - 必须是绝对路径
   *
   * @pattern ^/.*$
   * @default "/home/user"
   * @example "/app"
   */
  workDir?: string

  /**
   * 运行用户
   * - Sandbox 内进程的运行用户
   *
   * @default "user"
   * @example "root"
   */
  user?: string

  /**
   * Shell 路径
   * - 命令执行使用的 Shell
   *
   * @default "/bin/bash"
   */
  shell?: string

  // ==================== 资源配置 ====================

  /**
   * 资源限制
   * @see Resources
   */
  resources?: Resources

  /**
   * 网络配置
   * @see NetworkConfig
   */
  network?: NetworkConfig

  /**
   * 卷挂载列表
   * @see VolumeMount
   */
  volumes?: VolumeMount[]

  // ==================== 生命周期配置 ====================

  /**
   * 最大运行时间（毫秒）
   * - 超时后 Sandbox 自动停止
   * - 0 表示不限制
   *
   * @minimum 0
   * @maximum 86400000 (24小时)
   * @default 3600000 (1小时)
   */
  timeoutMs?: number

  /**
   * 空闲自动停止时间（分钟）
   * - 无活动后自动停止
   * - 0 表示不自动停止
   *
   * @minimum 0
   * @maximum 1440 (24小时)
   * @default 30
   */
  autoStopMinutes?: number

  /**
   * 空闲自动暂停
   * - 启用后，空闲时暂停而非停止
   * - 暂停的 Sandbox 可以快速恢复
   *
   * @default false
   */
  autoPause?: boolean

  /**
   * 自动归档时间（分钟）
   * - 停止后多久自���归档
   * - 0 表示不自动归档
   *
   * @minimum 0
   * @default 0
   */
  autoArchiveMinutes?: number

  /**
   * 自动删除时间（分钟）
   * - 停止后多久自动删除
   * - 0 表示不自动删除
   *
   * @minimum 0
   * @default 0
   */
  autoDeleteMinutes?: number

  // ==================== 元数据 ====================

  /**
   * 标签
   * - 用于筛选和组织 Sandbox
   *
   * @example {"project": "demo", "env": "dev"}
   */
  labels?: Record<string, string>

  /**
   * 元数据
   * - 用户自定义数据存储
   *
   * @example {"owner": "john", "purpose": "testing"}
   */
  metadata?: Record<string, string>

  // ==================== 高级配置 ====================

  /**
   * 是否为临时 Sandbox
   * - 临时 Sandbox 在断开连接后自动删除
   *
   * @default false
   */
  ephemeral?: boolean

  /**
   * 启动命令
   * - 覆盖模板的默认启动命令
   *
   * @example ["python", "-m", "http.server", "8080"]
   */
  entrypoint?: string[]

  /**
   * 启动参数
   * - 传递给启动命令的参数
   */
  args?: string[]

  /**
   * 健康检查配置
   */
  healthCheck?: HealthCheckConfig

  /**
   * 目标地区/集群
   * - 指定 Sandbox 运行的地区
   *
   * @example "us-west-1"
   */
  target?: string
}
```

```python
from dataclasses import dataclass, field
from typing import Optional, List, Dict

@dataclass
class CreateSandboxParams:
    """创建 Sandbox 参数"""

    # 基础配置
    name: Optional[str] = None
    """Sandbox 名称，3-63 字符，小写字母数字和连字符"""

    template: Optional[str] = None
    """模板/镜像标识"""

    snapshot: Optional[str] = None
    """快照标识，与 template 互斥"""

    # 环境配置
    env: Optional[Dict[str, str]] = None
    """环境变量"""

    work_dir: Optional[str] = None
    """默认工作目录"""

    user: Optional[str] = None
    """运行用户"""

    shell: Optional[str] = None
    """Shell 路径"""

    # 资源配置
    resources: Optional['Resources'] = None
    """资源限制"""

    network: Optional['NetworkConfig'] = None
    """网络配置"""

    volumes: Optional[List['VolumeMount']] = None
    """卷挂载列表"""

    # 生命周期配置
    timeout_ms: Optional[int] = None
    """最大运行时间（毫秒）"""

    auto_stop_minutes: Optional[int] = None
    """空闲自动停止时间（分钟）"""

    auto_pause: bool = False
    """空闲时自动暂停而非停止"""

    auto_archive_minutes: Optional[int] = None
    """自动归档时间（分钟）"""

    auto_delete_minutes: Optional[int] = None
    """自动删除时间（分钟）"""

    # 元数据
    labels: Optional[Dict[str, str]] = None
    """标签"""

    metadata: Optional[Dict[str, str]] = None
    """元数据"""

    # 高级配置
    ephemeral: bool = False
    """是否为临时 Sandbox"""

    entrypoint: Optional[List[str]] = None
    """启动命令"""

    args: Optional[List[str]] = None
    """启动参数"""

    target: Optional[str] = None
    """目标地区/集群"""
```

### 2.2 SandboxInfo

Sandbox 信息。

```typescript
/**
 * Sandbox 信息
 */
interface SandboxInfo {
  // ==================== 标识信息 ====================

  /**
   * Sandbox 唯一标识符
   * @example "sbx-abc123def456"
   */
  id: string

  /**
   * Sandbox 名称
   */
  name: string

  /**
   * 组织 ID
   */
  organizationId: string

  /**
   * 模板 ID
   */
  templateId?: string

  /**
   * 快照 ID（如果从快照创建）
   */
  snapshotId?: string

  // ==================== 状态信息 ====================

  /**
   * 当前状态
   * @see SandboxState
   */
  state: SandboxState

  /**
   * 错误原因（如果状态为 error）
   */
  errorReason?: string

  /**
   * 是否可恢复（错误状态下）
   */
  recoverable?: boolean

  // ==================== 资源信息 ====================

  /**
   * 分配的 CPU 核心数
   */
  cpu: number

  /**
   * 分配的内存（MB）
   */
  memory: number

  /**
   * 分配的磁盘（GB）
   */
  disk: number

  /**
   * 分配的 GPU 数量
   */
  gpu: number

  // ==================== 配置信息 ====================

  /**
   * 默认工作目录
   */
  workDir: string

  /**
   * 运行用户
   */
  user: string

  /**
   * 环境变量
   */
  env: Record<string, string>

  // ==================== 时间信息 ====================

  /**
   * 创建时间
   */
  createdAt: Date

  /**
   * 启动时间
   */
  startedAt?: Date

  /**
   * 停止时间
   */
  stoppedAt?: Date

  /**
   * 过期时间（运行超时）
   */
  expiresAt?: Date

  /**
   * 最后活动时间
   */
  lastActivityAt?: Date

  // ==================== 生命周期配置 ====================

  /**
   * 空闲自动停止时间（分钟）
   */
  autoStopMinutes?: number

  /**
   * 空闲自动暂停
   */
  autoPause?: boolean

  /**
   * 自动归档时间（分钟）
   */
  autoArchiveMinutes?: number

  /**
   * 自动删除时间（分钟）
   */
  autoDeleteMinutes?: number

  // ==================== 网络信息 ====================

  /**
   * 是否阻止所有网络访问
   */
  networkBlockAll: boolean

  /**
   * 网络白名单
   */
  networkAllowList?: string[]

  /**
   * 暴露的端口
   */
  exposedPorts?: number[]

  /**
   * Sandbox 域名
   */
  domain?: string

  // ==================== 元数据 ====================

  /**
   * 标签
   */
  labels: Record<string, string>

  /**
   * 元数据
   */
  metadata: Record<string, string>

  /**
   * 目标地区
   */
  target?: string
}
```

### 2.3 SandboxMetrics

Sandbox 性能指标。

```typescript
/**
 * Sandbox 性能指标
 */
interface SandboxMetrics {
  /**
   * 采集时间
   */
  timestamp: Date

  /**
   * CPU 使用率 (0-100)
   */
  cpuUsagePercent: number

  /**
   * CPU 核心数
   */
  cpuCount: number

  /**
   * 已使用内存（字节）
   */
  memoryUsed: number

  /**
   * 总内存（字节）
   */
  memoryTotal: number

  /**
   * 内存使用率 (0-100)
   */
  memoryUsagePercent: number

  /**
   * 已使用磁盘（字节）
   */
  diskUsed: number

  /**
   * 总磁盘（字节）
   */
  diskTotal: number

  /**
   * 磁盘使用率 (0-100)
   */
  diskUsagePercent: number

  /**
   * 网络接收字节数
   */
  networkRxBytes: number

  /**
   * 网络发送字节数
   */
  networkTxBytes: number

  /**
   * 运行中的进程数
   */
  processCount: number
}
```

### 2.4 ListSandboxParams

列表查询参数。

```typescript
/**
 * Sandbox 列表查询参数
 */
interface ListSandboxParams extends PaginationParams {
  /**
   * 按状态筛选
   * @example ["running", "paused"]
   */
  state?: SandboxState[]

  /**
   * 按标签筛选（AND 逻辑）
   * @example {"project": "demo"}
   */
  labels?: Record<string, string>

  /**
   * 按名称模糊搜索
   */
  search?: string

  /**
   * 按模板筛选
   */
  templateId?: string

  /**
   * 按创建时间筛选（起始）
   */
  createdAfter?: Date

  /**
   * 按创建时间筛选（结束）
   */
  createdBefore?: Date

  /**
   * 按目标地区筛选
   */
  target?: string
}
```

### 2.5 HealthCheckConfig

健康检查配置。

```typescript
/**
 * 健康检查配置
 */
interface HealthCheckConfig {
  /**
   * 检查类型
   */
  type: 'http' | 'tcp' | 'exec'

  /**
   * HTTP 检查路径（type=http 时必填）
   * @example "/health"
   */
  path?: string

  /**
   * 检查端口（type=http/tcp 时必填）
   */
  port?: number

  /**
   * 执行命令（type=exec 时必填）
   * @example ["curl", "-f", "http://localhost:8080/health"]
   */
  command?: string[]

  /**
   * 检查间隔（秒）
   * @default 30
   */
  intervalSeconds?: number

  /**
   * 超时时间（秒）
   * @default 10
   */
  timeoutSeconds?: number

  /**
   * 失败阈值
   * @default 3
   */
  failureThreshold?: number

  /**
   * 成功阈值
   * @default 1
   */
  successThreshold?: number

  /**
   * 启动延迟（秒）
   * @default 0
   */
  initialDelaySeconds?: number
}
```

---

## 3. SandboxService 接口

### 3.1 create

创建新的 Sandbox。

```typescript
/**
 * 创建 Sandbox
 *
 * @param params - 创建参数
 * @param options - 请求选项
 * @returns 创建的 Sandbox 实例
 * @throws {SandboxLimitExceededError} 超过 Sandbox 数量限制
 * @throws {ResourceInsufficientError} 资源不足
 * @throws {TemplateNotFoundError} 模板不存在
 * @throws {SnapshotNotFoundError} 快照不存在
 *
 * @example
 * // 使用默认配置创建
 * const sandbox = await client.sandbox.create()
 *
 * @example
 * // 指定模板和资源
 * const sandbox = await client.sandbox.create({
 *   name: 'my-sandbox',
 *   template: 'python:3.11',
 *   resources: { cpu: 2, memory: 4096 },
 *   env: { DEBUG: 'true' }
 * })
 *
 * @example
 * // 从快照创建
 * const sandbox = await client.sandbox.create({
 *   snapshot: 'snap-abc123',
 *   labels: { restored: 'true' }
 * })
 */
create(params?: CreateSandboxParams, options?: RequestOptions): Promise<Sandbox>
```

```python
def create(
    self,
    params: Optional[CreateSandboxParams] = None,
    *,
    timeout: Optional[float] = None
) -> Sandbox:
    """
    创建 Sandbox

    Args:
        params: 创建参数，None 时使用默认配置
        timeout: 请求超时时间（秒）

    Returns:
        创建的 Sandbox 实例

    Raises:
        SandboxLimitExceededError: 超过 Sandbox 数量限制
        ResourceInsufficientError: 资源不足
        TemplateNotFoundError: 模板不存在
        SnapshotNotFoundError: 快照不存在

    Example:
        >>> sandbox = client.sandbox.create()
        >>> sandbox = client.sandbox.create(CreateSandboxParams(
        ...     name='my-sandbox',
        ...     template='python:3.11',
        ...     resources=Resources(cpu=2, memory=4096)
        ... ))
    """
```

### 3.2 get

获取 Sandbox 实例。

```typescript
/**
 * 获取 Sandbox 实例
 *
 * @param idOrName - Sandbox ID 或名称
 * @param options - 请求选项
 * @returns Sandbox 实例
 * @throws {SandboxNotFoundError} Sandbox 不存在
 *
 * @example
 * const sandbox = await client.sandbox.get('sbx-abc123')
 * const sandbox = await client.sandbox.get('my-sandbox')
 */
get(idOrName: string, options?: RequestOptions): Promise<Sandbox>
```

```python
def get(
    self,
    id_or_name: str,
    *,
    timeout: Optional[float] = None
) -> Sandbox:
    """
    获取 Sandbox 实例

    Args:
        id_or_name: Sandbox ID 或名称
        timeout: 请求超时时间（秒）

    Returns:
        Sandbox 实例

    Raises:
        SandboxNotFoundError: Sandbox 不存在
    """
```

### 3.3 list

列出 Sandbox。

```typescript
/**
 * 列出 Sandbox
 *
 * @param params - 查询参数
 * @param options - 请求选项
 * @returns 分页结果
 *
 * @example
 * // 获取所有运行中的 Sandbox
 * const result = await client.sandbox.list({
 *   state: ['running'],
 *   limit: 10
 * })
 *
 * @example
 * // 按标签筛选
 * const result = await client.sandbox.list({
 *   labels: { project: 'demo' }
 * })
 */
list(params?: ListSandboxParams, options?: RequestOptions): Promise<PaginatedResult<SandboxInfo>>
```

```python
def list(
    self,
    *,
    state: Optional[List[SandboxState]] = None,
    labels: Optional[Dict[str, str]] = None,
    search: Optional[str] = None,
    page: int = 1,
    limit: int = 20,
    sort_by: str = 'createdAt',
    sort_order: str = 'desc',
    timeout: Optional[float] = None
) -> PaginatedResult[SandboxInfo]:
    """
    列出 Sandbox

    Args:
        state: 按状态筛选
        labels: 按标签筛选
        search: 按名称模糊搜索
        page: 页码（从 1 开始）
        limit: 每页数量
        sort_by: 排序字段
        sort_order: 排序方向 ('asc' | 'desc')
        timeout: 请求超时时间（秒）

    Returns:
        分页结果
    """
```

### 3.4 start

启动 Sandbox。

```typescript
/**
 * 启动 Sandbox
 *
 * @param idOrName - Sandbox ID 或名称
 * @param options - 选项
 * @param options.waitForReady - 是否等待 Sandbox 就绪
 * @param options.timeoutMs - 等待超时时间（毫秒）
 * @returns 启动后的 Sandbox 信息
 * @throws {SandboxNotFoundError} Sandbox 不存在
 * @throws {SandboxAlreadyRunningError} Sandbox 已经���运行
 * @throws {SandboxStartFailedError} 启动失败
 * @throws {TimeoutError} 等待超时
 *
 * @example
 * await client.sandbox.start('sbx-abc123')
 *
 * @example
 * // 等待就绪
 * await client.sandbox.start('sbx-abc123', {
 *   waitForReady: true,
 *   timeoutMs: 60000
 * })
 */
start(
  idOrName: string,
  options?: {
    waitForReady?: boolean
    timeoutMs?: number
  } & RequestOptions
): Promise<SandboxInfo>
```

```python
def start(
    self,
    id_or_name: str,
    *,
    wait_for_ready: bool = True,
    timeout: Optional[float] = 60
) -> SandboxInfo:
    """
    启动 Sandbox

    Args:
        id_or_name: Sandbox ID 或名称
        wait_for_ready: 是否等待 Sandbox 就绪
        timeout: 等待超时时间（秒）

    Returns:
        启动后的 Sandbox 信息

    Raises:
        SandboxNotFoundError: Sandbox 不存在
        SandboxAlreadyRunningError: Sandbox 已经在运行
        SandboxStartFailedError: 启动失败
        TimeoutError: 等待超时
    """
```

### 3.5 stop

停止 Sandbox。

```typescript
/**
 * 停止 Sandbox
 *
 * @param idOrName - Sandbox ID 或名称
 * @param options - 选项
 * @param options.force - 是否强制停止（不等待进程退出）
 * @param options.timeoutMs - 等待停止的超时时间（毫秒）
 * @returns 停止后的 Sandbox 信息
 * @throws {SandboxNotFoundError} Sandbox 不存在
 * @throws {SandboxNotRunningError} Sandbox 未在运行
 * @throws {TimeoutError} 等待超时
 *
 * @example
 * await client.sandbox.stop('sbx-abc123')
 *
 * @example
 * // 强制停止
 * await client.sandbox.stop('sbx-abc123', { force: true })
 */
stop(
  idOrName: string,
  options?: {
    force?: boolean
    timeoutMs?: number
  } & RequestOptions
): Promise<SandboxInfo>
```

```python
def stop(
    self,
    id_or_name: str,
    *,
    force: bool = False,
    timeout: Optional[float] = 30
) -> SandboxInfo:
    """
    停止 Sandbox

    Args:
        id_or_name: Sandbox ID 或名称
        force: 是否强制停止
        timeout: 等待超时时间（秒）

    Returns:
        停止后的 Sandbox 信息

    Raises:
        SandboxNotFoundError: Sandbox 不存在
        SandboxNotRunningError: Sandbox 未在运行
        TimeoutError: 等待超时
    """
```

### 3.6 pause

暂停 Sandbox。

```typescript
/**
 * 暂停 Sandbox
 *
 * 暂停会保存当前运行状态，可以快速恢复。
 * 暂停的 Sandbox 不消耗计算资源，但仍占用存储。
 *
 * @param idOrName - Sandbox ID 或名称
 * @param options - 请求选项
 * @returns 暂停后的 Sandbox 信息
 * @throws {SandboxNotFoundError} Sandbox 不存在
 * @throws {SandboxNotRunningError} Sandbox 未在运行
 * @throws {PauseNotSupportedError} 当前配置不支持暂停
 *
 * @example
 * await client.sandbox.pause('sbx-abc123')
 */
pause(idOrName: string, options?: RequestOptions): Promise<SandboxInfo>
```

### 3.7 resume

恢复暂停的 Sandbox。

```typescript
/**
 * 恢复暂停的 Sandbox
 *
 * @param idOrName - Sandbox ID 或名称
 * @param options - 选项
 * @param options.waitForReady - 是否等待 Sandbox 就绪
 * @param options.timeoutMs - 等待超时时间（毫秒）
 * @returns 恢复后的 Sandbox 信息
 * @throws {SandboxNotFoundError} Sandbox 不存在
 * @throws {SandboxNotPausedError} Sandbox 未处于暂停状态
 * @throws {TimeoutError} 等待超时
 *
 * @example
 * await client.sandbox.resume('sbx-abc123')
 */
resume(
  idOrName: string,
  options?: {
    waitForReady?: boolean
    timeoutMs?: number
  } & RequestOptions
): Promise<SandboxInfo>
```

### 3.8 delete

删除 Sandbox。

```typescript
/**
 * 删除 Sandbox
 *
 * @param idOrName - Sandbox ID 或名称
 * @param options - 选项
 * @param options.force - 是否强制删除（即使正在运行）
 * @throws {SandboxNotFoundError} Sandbox 不存在
 * @throws {SandboxRunningError} Sandbox 正在运行（未使用 force）
 *
 * @example
 * await client.sandbox.delete('sbx-abc123')
 *
 * @example
 * // 强制删除运行中的 Sandbox
 * await client.sandbox.delete('sbx-abc123', { force: true })
 */
delete(
  idOrName: string,
  options?: {
    force?: boolean
  } & RequestOptions
): Promise<void>
```

### 3.9 getInfo

获取 Sandbox 信息。

```typescript
/**
 * 获取 Sandbox 详细信息
 *
 * @param idOrName - Sandbox ID 或名称
 * @param options - 请求选项
 * @returns Sandbox 信息
 * @throws {SandboxNotFoundError} Sandbox 不存在
 *
 * @example
 * const info = await client.sandbox.getInfo('sbx-abc123')
 * console.log(`State: ${info.state}, CPU: ${info.cpu}`)
 */
getInfo(idOrName: string, options?: RequestOptions): Promise<SandboxInfo>
```

### 3.10 getMetrics

获取 Sandbox 性能指标。

```typescript
/**
 * 获取 Sandbox 性能指标
 *
 * @param idOrName - Sandbox ID 或名称
 * @param options - 选项
 * @param options.start - 开始时间
 * @param options.end - 结束时间
 * @param options.step - 采样间隔（秒）
 * @returns 指标数组
 * @throws {SandboxNotFoundError} Sandbox 不存在
 *
 * @example
 * // 获取最近 1 小时的指标
 * const metrics = await client.sandbox.getMetrics('sbx-abc123', {
 *   start: new Date(Date.now() - 3600000),
 *   end: new Date()
 * })
 */
getMetrics(
  idOrName: string,
  options?: {
    start?: Date
    end?: Date
    step?: number
  } & RequestOptions
): Promise<SandboxMetrics[]>
```

### 3.11 isRunning

检查 Sandbox 是否正在运行。

```typescript
/**
 * 检查 Sandbox 是否正在运行
 *
 * @param idOrName - Sandbox ID 或名称
 * @param options - 请求选项
 * @returns 是否正在运行
 * @throws {SandboxNotFoundError} Sandbox 不存在
 *
 * @example
 * if (await client.sandbox.isRunning('sbx-abc123')) {
 *   console.log('Sandbox is running')
 * }
 */
isRunning(idOrName: string, options?: RequestOptions): Promise<boolean>
```

### 3.12 setTimeout

设置 Sandbox 超时时间。

```typescript
/**
 * 设置 Sandbox 超时时间
 *
 * @param idOrName - Sandbox ID 或名称
 * @param timeoutMs - 新的超时时间（毫秒），0 表示不限制
 * @param options - 请求选项
 * @throws {SandboxNotFoundError} Sandbox 不存在
 *
 * @example
 * // 延长超时时间到 2 小时
 * await client.sandbox.setTimeout('sbx-abc123', 7200000)
 */
setTimeout(idOrName: string, timeoutMs: number, options?: RequestOptions): Promise<void>
```

### 3.13 setLabels

设置 Sandbox 标签。

```typescript
/**
 * 设置 Sandbox 标签
 *
 * @param idOrName - Sandbox ID 或名称
 * @param labels - 新的标签（完全替换）
 * @param options - 请求选项
 * @returns 更新后的标签
 * @throws {SandboxNotFoundError} Sandbox 不存在
 *
 * @example
 * await client.sandbox.setLabels('sbx-abc123', {
 *   project: 'demo',
 *   env: 'production'
 * })
 */
setLabels(
  idOrName: string,
  labels: Record<string, string>,
  options?: RequestOptions
): Promise<Record<string, string>>
```

### 3.14 getHost

获取 Sandbox 服务的访问地址。

```typescript
/**
 * 获取 Sandbox 内服务的访问主机地址
 *
 * @param idOrName - Sandbox ID 或名称
 * @param port - 端口号
 * @param options - 请求选项
 * @returns 访问地址（host:port 或域名）
 * @throws {SandboxNotFoundError} Sandbox 不存在
 * @throws {SandboxNotRunningError} Sandbox 未在运行
 *
 * @example
 * const host = await client.sandbox.getHost('sbx-abc123', 8080)
 * // 返回: "sbx-abc123-8080.sandbox.example.com"
 */
getHost(idOrName: string, port: number, options?: RequestOptions): Promise<string>
```

### 3.15 getPreviewUrl

获取 Sandbox 服务的预览 URL。

```typescript
/**
 * 获取 Sandbox 内服务的预览 URL
 *
 * @param idOrName - Sandbox ID 或名称
 * @param port - 端口号
 * @param options - 选项
 * @param options.protocol - 协议（http/https）
 * @param options.path - 路径
 * @returns 完整的预览 URL
 * @throws {SandboxNotFoundError} Sandbox 不存在
 * @throws {SandboxNotRunningError} Sandbox 未在运行
 *
 * @example
 * const url = await client.sandbox.getPreviewUrl('sbx-abc123', 3000)
 * // 返回: "https://sbx-abc123-3000.sandbox.example.com"
 *
 * @example
 * const url = await client.sandbox.getPreviewUrl('sbx-abc123', 3000, {
 *   path: '/api/health'
 * })
 * // 返回: "https://sbx-abc123-3000.sandbox.example.com/api/health"
 */
getPreviewUrl(
  idOrName: string,
  port: number,
  options?: {
    protocol?: 'http' | 'https'
    path?: string
  } & RequestOptions
): Promise<string>
```

---

## 4. Sandbox 实例接口

通过 `client.sandbox.create()` 或 `client.sandbox.get()` 获取的 Sandbox 实例。

### 4.1 属性

```typescript
class Sandbox {
  /**
   * Sandbox ID
   */
  readonly id: string

  /**
   * Sandbox 名称
   */
  readonly name: string

  /**
   * Sandbox 信息（创建时的快照，调用 refresh() 更新）
   */
  readonly info: SandboxInfo

  /**
   * 文件系统服务
   * @see FileSystemService
   */
  readonly fs: FileSystemService

  /**
   * 进程执行服务
   * @see ProcessService
   */
  readonly process: ProcessService

  /**
   * Git 服务
   * @see GitService
   */
  readonly git: GitService

  /**
   * LSP 服务
   * @see LSPService
   */
  readonly lsp: LSPService

  /**
   * PTY 服务
   * @see PTYService
   */
  readonly pty: PTYService
}
```

### 4.2 生命周期方法

```typescript
class Sandbox {
  /**
   * 启动 Sandbox
   */
  start(options?: { waitForReady?: boolean; timeoutMs?: number }): Promise<void>

  /**
   * 停止 Sandbox
   */
  stop(options?: { force?: boolean; timeoutMs?: number }): Promise<void>

  /**
   * 暂停 Sandbox
   */
  pause(): Promise<void>

  /**
   * 恢复 Sandbox
   */
  resume(options?: { waitForReady?: boolean; timeoutMs?: number }): Promise<void>

  /**
   * 删除 Sandbox
   */
  delete(options?: { force?: boolean }): Promise<void>

  /**
   * 等待 Sandbox 启动完成
   */
  waitUntilReady(timeoutMs?: number): Promise<void>

  /**
   * 等待 Sandbox 停止
   */
  waitUntilStopped(timeoutMs?: number): Promise<void>
}
```

### 4.3 信息方法

```typescript
class Sandbox {
  /**
   * 刷新 Sandbox 信息
   */
  refresh(): Promise<void>

  /**
   * 获取性能指标
   */
  getMetrics(options?: { start?: Date; end?: Date }): Promise<SandboxMetrics[]>

  /**
   * 刷新活动计时器（重置自动停止倒计时）
   */
  refreshActivity(): Promise<void>

  /**
   * 检查是否正在运行
   */
  isRunning(): Promise<boolean>

  /**
   * 获取用户主目录
   */
  getUserHomeDir(): Promise<string>

  /**
   * 获取工作目录
   */
  getWorkDir(): Promise<string>
}
```

### 4.4 配置方法

```typescript
class Sandbox {
  /**
   * 设置标签
   */
  setLabels(labels: Record<string, string>): Promise<void>

  /**
   * 设置超时时间
   */
  setTimeout(timeoutMs: number): Promise<void>

  /**
   * 设置自动停止时间
   */
  setAutoStopInterval(minutes: number): Promise<void>

  /**
   * 设置自动归档时间
   */
  setAutoArchiveInterval(minutes: number): Promise<void>

  /**
   * 设置自动删除时间
   */
  setAutoDeleteInterval(minutes: number): Promise<void>
}
```

### 4.5 网络方法

```typescript
class Sandbox {
  /**
   * 获取服务主机地址
   */
  getHost(port: number): string

  /**
   * 获取预览 URL
   */
  getPreviewUrl(port: number, options?: { protocol?: string; path?: string }): Promise<string>

  /**
   * 获取文件下载 URL
   */
  getDownloadUrl(path: string, options?: { expiresIn?: number }): Promise<string>

  /**
   * 获取文件上传 URL
   */
  getUploadUrl(path?: string, options?: { expiresIn?: number }): Promise<string>
}
```

### 4.6 Context Manager (Python)

```python
class Sandbox:
    """Sandbox 实例"""

    def __enter__(self) -> 'Sandbox':
        """进入上下文时确保 Sandbox 运行"""
        if self.info.state != SandboxState.RUNNING:
            self.start()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        """退出上下文时停止 Sandbox"""
        if self.info.state == SandboxState.RUNNING:
            self.stop()


class AsyncSandbox:
    """异步 Sandbox 实例"""

    async def __aenter__(self) -> 'AsyncSandbox':
        if self.info.state != SandboxState.RUNNING:
            await self.start()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        if self.info.state == SandboxState.RUNNING:
            await self.stop()
```

---

## 5. REST API

### 5.1 端点列表

| 方法 | 端点 | 描述 |
|------|------|------|
| POST | `/api/v1/sandboxes` | 创建 Sandbox |
| GET | `/api/v1/sandboxes` | 列出 Sandbox |
| GET | `/api/v1/sandboxes/{id}` | 获取 Sandbox |
| DELETE | `/api/v1/sandboxes/{id}` | 删除 Sandbox |
| POST | `/api/v1/sandboxes/{id}/start` | 启动 Sandbox |
| POST | `/api/v1/sandboxes/{id}/stop` | 停止 Sandbox |
| POST | `/api/v1/sandboxes/{id}/pause` | 暂停 Sandbox |
| POST | `/api/v1/sandboxes/{id}/resume` | 恢复 Sandbox |
| GET | `/api/v1/sandboxes/{id}/metrics` | 获取指标 |
| PATCH | `/api/v1/sandboxes/{id}/labels` | 更新标签 |
| PATCH | `/api/v1/sandboxes/{id}/timeout` | 更新超时 |

### 5.2 创建 Sandbox

**请求**:

```http
POST /api/v1/sandboxes
Content-Type: application/json
Authorization: Bearer <token>

{
  "name": "my-sandbox",
  "template": "python:3.11",
  "resources": {
    "cpu": 2,
    "memory": 4096,
    "disk": 20
  },
  "env": {
    "DEBUG": "true"
  },
  "network": {
    "allowInternet": true,
    "exposedPorts": [8080, 3000]
  },
  "timeoutMs": 3600000,
  "autoStopMinutes": 30,
  "labels": {
    "project": "demo"
  }
}
```

**响应** (201 Created):

```json
{
  "id": "sbx-abc123def456",
  "name": "my-sandbox",
  "organizationId": "org-xyz789",
  "templateId": "python:3.11",
  "state": "running",
  "cpu": 2,
  "memory": 4096,
  "disk": 20,
  "gpu": 0,
  "workDir": "/home/user",
  "user": "user",
  "env": {
    "DEBUG": "true"
  },
  "createdAt": "2024-01-15T10:30:00Z",
  "startedAt": "2024-01-15T10:30:05Z",
  "expiresAt": "2024-01-15T11:30:00Z",
  "autoStopMinutes": 30,
  "networkBlockAll": false,
  "exposedPorts": [8080, 3000],
  "domain": "sbx-abc123def456.sandbox.example.com",
  "labels": {
    "project": "demo"
  },
  "metadata": {}
}
```

### 5.3 列出 Sandbox

**请求**:

```http
GET /api/v1/sandboxes?state=running&labels[project]=demo&page=1&limit=20
Authorization: Bearer <token>
```

**响应** (200 OK):

```json
{
  "items": [
    {
      "id": "sbx-abc123",
      "name": "my-sandbox",
      "state": "running",
      "cpu": 2,
      "memory": 4096,
      "createdAt": "2024-01-15T10:30:00Z",
      "labels": {"project": "demo"}
    }
  ],
  "total": 1,
  "page": 1,
  "limit": 20,
  "totalPages": 1,
  "hasNext": false,
  "hasPrev": false
}
```

### 5.4 获取 Sandbox

**请求**:

```http
GET /api/v1/sandboxes/sbx-abc123
Authorization: Bearer <token>
```

**响应** (200 OK):

```json
{
  "id": "sbx-abc123def456",
  "name": "my-sandbox",
  "state": "running",
  ...
}
```

### 5.5 启动 Sandbox

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/start
Content-Type: application/json
Authorization: Bearer <token>

{
  "waitForReady": true,
  "timeoutMs": 60000
}
```

**响应** (200 OK):

```json
{
  "id": "sbx-abc123",
  "state": "running",
  "startedAt": "2024-01-15T10:35:00Z",
  ...
}
```

### 5.6 停止 Sandbox

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/stop
Content-Type: application/json
Authorization: Bearer <token>

{
  "force": false,
  "timeoutMs": 30000
}
```

**响应** (200 OK):

```json
{
  "id": "sbx-abc123",
  "state": "stopped",
  "stoppedAt": "2024-01-15T11:00:00Z",
  ...
}
```

### 5.7 删除 Sandbox

**请求**:

```http
DELETE /api/v1/sandboxes/sbx-abc123?force=true
Authorization: Bearer <token>
```

**响应** (204 No Content)

### 5.8 获取指标

**请求**:

```http
GET /api/v1/sandboxes/sbx-abc123/metrics?start=2024-01-15T09:00:00Z&end=2024-01-15T10:00:00Z&step=60
Authorization: Bearer <token>
```

**响应** (200 OK):

```json
{
  "metrics": [
    {
      "timestamp": "2024-01-15T09:00:00Z",
      "cpuUsagePercent": 25.5,
      "cpuCount": 2,
      "memoryUsed": 1073741824,
      "memoryTotal": 4294967296,
      "memoryUsagePercent": 25.0,
      "diskUsed": 5368709120,
      "diskTotal": 21474836480,
      "diskUsagePercent": 25.0,
      "networkRxBytes": 10485760,
      "networkTxBytes": 5242880,
      "processCount": 15
    }
  ]
}
```

---

## 6. 使用示例

### 6.1 TypeScript 示例

```typescript
import { WorkspaceClient } from '@workspace-sdk/typescript'

async function main() {
  // 创建客户端
  const client = new WorkspaceClient({
    apiUrl: 'https://api.workspace.example.com',
    apiKey: process.env.WORKSPACE_API_KEY
  })

  // 创建 Sandbox
  const sandbox = await client.sandbox.create({
    name: 'dev-environment',
    template: 'python:3.11',
    resources: { cpu: 2, memory: 4096 },
    env: { DEBUG: 'true' },
    labels: { project: 'demo' }
  })

  console.log(`Created sandbox: ${sandbox.id}`)

  try {
    // 执行命令
    const result = await sandbox.process.run('python --version')
    console.log(`Python version: ${result.stdout}`)

    // 创建文件
    await sandbox.fs.write('/app/hello.py', `
print("Hello, World!")
`)

    // 执行 Python 脚本
    const output = await sandbox.process.run('python /app/hello.py')
    console.log(`Output: ${output.stdout}`)

    // 获取性能指标
    const metrics = await sandbox.getMetrics()
    console.log(`CPU usage: ${metrics[0]?.cpuUsagePercent}%`)

  } finally {
    // 删除 Sandbox
    await sandbox.delete()
    console.log('Sandbox deleted')
  }
}

main().catch(console.error)
```

### 6.2 Python 示例

```python
from workspace_sdk import WorkspaceClient, CreateSandboxParams, Resources
import os

def main():
    # 创建客户端
    client = WorkspaceClient(api_key=os.environ.get('WORKSPACE_API_KEY'))

    # 使用 context manager 自动管理生命周期
    with client.sandbox.create(CreateSandboxParams(
        name='dev-environment',
        template='python:3.11',
        resources=Resources(cpu=2, memory=4096),
        env={'DEBUG': 'true'},
        labels={'project': 'demo'}
    )) as sandbox:
        print(f"Created sandbox: {sandbox.id}")

        # 执行命令
        result = sandbox.process.run('python --version')
        print(f"Python version: {result.stdout}")

        # 创建文件
        sandbox.fs.write('/app/hello.py', 'print("Hello, World!")')

        # 执行 Python 脚本
        output = sandbox.process.run('python /app/hello.py')
        print(f"Output: {output.stdout}")

        # 获取性能指标
        metrics = sandbox.get_metrics()
        if metrics:
            print(f"CPU usage: {metrics[0].cpu_usage_percent}%")

    # Sandbox 自动停止和删除
    print("Sandbox cleaned up")

if __name__ == '__main__':
    main()
```

### 6.3 异步 Python 示例

```python
import asyncio
from workspace_sdk import AsyncWorkspaceClient, CreateSandboxParams

async def main():
    client = AsyncWorkspaceClient()

    async with await client.sandbox.create(CreateSandboxParams(
        template='node:20',
        resources={'cpu': 1, 'memory': 2048}
    )) as sandbox:
        # 并发执行多个命令
        results = await asyncio.gather(
            sandbox.process.run('node --version'),
            sandbox.process.run('npm --version'),
            sandbox.process.run('npx --version')
        )

        for result in results:
            print(result.stdout.strip())

asyncio.run(main())
```

---

## 7. 错误处理

### 7.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 2000 | SANDBOX_NOT_FOUND | 404 | Sandbox 不存在 |
| 2001 | SANDBOX_ALREADY_EXISTS | 409 | Sandbox 名称已存在 |
| 2002 | SANDBOX_NOT_RUNNING | 400 | Sandbox 未在运行 |
| 2003 | SANDBOX_ALREADY_RUNNING | 400 | Sandbox 已经在运行 |
| 2004 | SANDBOX_START_FAILED | 500 | Sandbox 启动失败 |
| 2005 | SANDBOX_STOP_FAILED | 500 | Sandbox 停止失败 |
| 2006 | SANDBOX_LIMIT_EXCEEDED | 429 | 超过 Sandbox 数量限制 |
| 2007 | SANDBOX_RESOURCE_INSUFFICIENT | 503 | 资源不足 |
| 2008 | SANDBOX_TIMEOUT | 408 | 操作超时 |

### 7.2 错误处理示例

```typescript
import {
  WorkspaceError,
  SandboxNotFoundError,
  SandboxAlreadyRunningError,
  TimeoutError
} from '@workspace-sdk/typescript'

async function handleSandbox(client: WorkspaceClient, id: string) {
  try {
    const sandbox = await client.sandbox.get(id)
    await sandbox.start({ timeoutMs: 30000 })
  } catch (error) {
    if (error instanceof SandboxNotFoundError) {
      console.log(`Sandbox ${id} not found, creating new one...`)
      return await client.sandbox.create({ name: id })
    }

    if (error instanceof SandboxAlreadyRunningError) {
      console.log('Sandbox already running')
      return await client.sandbox.get(id)
    }

    if (error instanceof TimeoutError) {
      console.log('Start timeout, retrying...')
      return await client.sandbox.start(id, { timeoutMs: 60000 })
    }

    // 其他错误
    throw error
  }
}
```

```python
from workspace_sdk import (
    WorkspaceError,
    SandboxNotFoundError,
    SandboxAlreadyRunningError,
    TimeoutError
)

def handle_sandbox(client, id: str):
    try:
        sandbox = client.sandbox.get(id)
        sandbox.start(timeout=30)
    except SandboxNotFoundError:
        print(f"Sandbox {id} not found, creating new one...")
        return client.sandbox.create(name=id)
    except SandboxAlreadyRunningError:
        print("Sandbox already running")
        return client.sandbox.get(id)
    except TimeoutError:
        print("Start timeout, retrying...")
        return client.sandbox.start(id, timeout=60)
    except WorkspaceError as e:
        print(f"Error: {e.message} (code: {e.code})")
        raise
```

---

## 附录

### A. 状态转换表

| 当前状态 | 可执行操作 | 目标状态 |
|---------|-----------|---------|
| pending | (自动) | starting |
| starting | (自动成功) | running |
| starting | (自动失败) | error |
| running | stop | stopping → stopped |
| running | pause | paused |
| running | (超时) | stopped |
| running | delete(force) | (deleted) |
| stopped | start | starting → running |
| stopped | delete | (deleted) |
| paused | resume | running |
| paused | delete | (deleted) |
| error | start (recoverable) | starting |
| error | delete | (deleted) |

### B. 资源限制参考

| 资源 | 最小值 | 最大值 | 默认值 |
|------|-------|-------|-------|
| CPU | 0.5 核 | 96 核 | 1 核 |
| 内存 | 256 MB | 384 GB | 1 GB |
| 磁盘 | 1 GB | 2 TB | 10 GB |
| GPU | 0 | 8 | 0 |
| 最大运行时间 | - | 24 小时 | 1 小时 |
| 自动停止时间 | 1 分钟 | 24 小时 | 30 分钟 |
