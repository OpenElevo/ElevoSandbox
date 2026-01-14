# Snapshot 和 Volume 服务接口文档

本文档描述 Snapshot（快照）和 Volume（卷）服务的接口设计，用于 Sandbox 状态保存和持久化存储。

---

## 目录

- [1. Snapshot 服务](#1-snapshot-服务)
- [2. Volume 服务](#2-volume-服务)
- [3. REST API](#3-rest-api)
- [4. 使用示例](#4-使用示例)
- [5. 错误处理](#5-错误处理)

---

## 1. Snapshot 服务

### 1.1 概述

Snapshot 服务用于保存 Sandbox 的完整状态，包括文件系统、环境配置等，可用于快速恢复或创建新的 Sandbox。

| 功能 | 描述 |
|------|------|
| create | 创建快照 |
| get | 获取快照信息 |
| list | 列出快照 |
| delete | 删除快照 |

### 1.2 数据类型

#### SnapshotInfo

```typescript
/**
 * 快照信息
 */
interface SnapshotInfo {
  /**
   * 快照 ID
   * @example "snap-abc123"
   */
  id: string

  /**
   * 快照名称
   */
  name: string

  /**
   * 描述
   */
  description?: string

  /**
   * 来源 Sandbox ID
   */
  sandboxId: string

  /**
   * 来源 Sandbox 模板
   */
  templateId?: string

  /**
   * 快照大小（字节）
   */
  size: number

  /**
   * 快照状态
   */
  state: SnapshotState

  /**
   * 创建时间
   */
  createdAt: Date

  /**
   * 完成时间
   */
  completedAt?: Date

  /**
   * 过期时间
   */
  expiresAt?: Date

  /**
   * 标签
   */
  labels: Record<string, string>

  /**
   * 元数据
   */
  metadata: Record<string, string>

  /**
   * 错误信息（如果创建失败）
   */
  errorMessage?: string
}

/**
 * 快照状态
 */
enum SnapshotState {
  /** 创建中 */
  CREATING = 'creating',
  /** 可用 */
  AVAILABLE = 'available',
  /** 删除中 */
  DELETING = 'deleting',
  /** 失败 */
  FAILED = 'failed'
}
```

```python
from dataclasses import dataclass
from datetime import datetime
from typing import Optional, Dict
from enum import Enum

class SnapshotState(str, Enum):
    CREATING = "creating"
    AVAILABLE = "available"
    DELETING = "deleting"
    FAILED = "failed"

@dataclass
class SnapshotInfo:
    """快照信息"""
    id: str
    name: str
    sandbox_id: str
    size: int
    state: SnapshotState
    created_at: datetime
    description: Optional[str] = None
    template_id: Optional[str] = None
    completed_at: Optional[datetime] = None
    expires_at: Optional[datetime] = None
    labels: Dict[str, str] = None
    metadata: Dict[str, str] = None
    error_message: Optional[str] = None
```

#### CreateSnapshotParams

```typescript
/**
 * 创建快照参数
 */
interface CreateSnapshotParams {
  /**
   * 快照名称
   * @pattern ^[a-z0-9][a-z0-9-]*[a-z0-9]$
   * @minLength 3
   * @maxLength 63
   */
  name: string

  /**
   * 来源 Sandbox ID
   */
  sandboxId: string

  /**
   * 描述
   * @maxLength 1000
   */
  description?: string

  /**
   * 标签
   */
  labels?: Record<string, string>

  /**
   * 元数据
   */
  metadata?: Record<string, string>

  /**
   * 过期时间（小时）
   * 0 表示永不过期
   * @default 0
   */
  expiresInHours?: number

  /**
   * 是否等待完成
   * @default true
   */
  wait?: boolean
}
```

### 1.3 SnapshotService 接口

#### create

```typescript
/**
 * 创建快照
 *
 * @param params - 创建参数
 * @returns 快照信息
 * @throws {SandboxNotFoundError} Sandbox 不存在
 * @throws {SandboxNotRunningError} Sandbox 未运行
 * @throws {SnapshotLimitExceededError} 超过快照数量限制
 *
 * @example
 * const snapshot = await client.snapshot.create({
 *   name: 'my-snapshot',
 *   sandboxId: 'sbx-abc123',
 *   description: 'Before major refactoring'
 * })
 *
 * @example
 * // 设置过期时间
 * const snapshot = await client.snapshot.create({
 *   name: 'temp-snapshot',
 *   sandboxId: 'sbx-abc123',
 *   expiresInHours: 24
 * })
 */
create(params: CreateSnapshotParams): Promise<SnapshotInfo>
```

```python
def create(
    self,
    name: str,
    sandbox_id: str,
    *,
    description: Optional[str] = None,
    labels: Optional[Dict[str, str]] = None,
    metadata: Optional[Dict[str, str]] = None,
    expires_in_hours: int = 0,
    wait: bool = True,
    timeout: Optional[float] = None
) -> SnapshotInfo:
    """
    创建快照

    Args:
        name: 快照名称
        sandbox_id: 来源 Sandbox ID
        description: 描述
        labels: 标签
        metadata: 元数据
        expires_in_hours: 过期时间（小时），0 表示永不过期
        wait: 是否等待完成
        timeout: 超时时间

    Returns:
        快照信息

    Raises:
        SandboxNotFoundError: Sandbox 不存在
        SandboxNotRunningError: Sandbox 未运行
        SnapshotLimitExceededError: 超过快照数量限制
    """
```

#### get

```typescript
/**
 * 获取快照信息
 *
 * @param idOrName - 快照 ID 或名称
 * @returns 快照信息
 * @throws {SnapshotNotFoundError} 快照不存在
 *
 * @example
 * const snapshot = await client.snapshot.get('snap-abc123')
 * const snapshot = await client.snapshot.get('my-snapshot')
 */
get(idOrName: string): Promise<SnapshotInfo>
```

#### list

```typescript
/**
 * 列出快照
 *
 * @param params - 查询参数
 * @returns 分页结果
 *
 * @example
 * const result = await client.snapshot.list({
 *   labels: { project: 'demo' },
 *   limit: 20
 * })
 *
 * @example
 * // 列出指定 Sandbox 的快照
 * const result = await client.snapshot.list({
 *   sandboxId: 'sbx-abc123'
 * })
 */
list(params?: {
  /**
   * 按标签筛选
   */
  labels?: Record<string, string>

  /**
   * 按来源 Sandbox 筛选
   */
  sandboxId?: string

  /**
   * 按状态筛选
   */
  state?: SnapshotState[]

  /**
   * 按名称搜索
   */
  search?: string

  /**
   * 页码
   * @default 1
   */
  page?: number

  /**
   * 每页数量
   * @default 20
   */
  limit?: number

  /**
   * 排序字段
   * @default "createdAt"
   */
  sortBy?: 'name' | 'createdAt' | 'size'

  /**
   * 排序方向
   * @default "desc"
   */
  sortOrder?: 'asc' | 'desc'
}): Promise<PaginatedResult<SnapshotInfo>>
```

#### delete

```typescript
/**
 * 删除快照
 *
 * @param idOrName - 快照 ID 或名称
 * @throws {SnapshotNotFoundError} 快照不存在
 * @throws {SnapshotInUseError} 快照正在使用中
 *
 * @example
 * await client.snapshot.delete('snap-abc123')
 */
delete(idOrName: string): Promise<void>
```

---

## 2. Volume 服务

### 2.1 概述

Volume 服务用于管理持久化存储卷，可以挂载到 Sandbox 中实现数据持久化。

| 功能 | 描述 |
|------|------|
| create | 创建卷 |
| get | 获取卷信息 |
| getOrCreate | 获取或创建卷 |
| list | 列出卷 |
| delete | 删除卷 |
| resize | 调整卷大小 |

### 2.2 数据类型

#### VolumeInfo

```typescript
/**
 * 卷信息
 */
interface VolumeInfo {
  /**
   * 卷 ID
   * @example "vol-abc123"
   */
  id: string

  /**
   * 卷名称
   */
  name: string

  /**
   * 卷大小（字节）
   */
  size: number

  /**
   * 已使用大小（字节）
   */
  usedSize: number

  /**
   * 卷状态
   */
  state: VolumeState

  /**
   * 文件系统类型
   * @example "ext4"
   */
  filesystem: string

  /**
   * 创建时间
   */
  createdAt: Date

  /**
   * 最后挂载时间
   */
  lastMountedAt?: Date

  /**
   * 当前挂载的 Sandbox
   */
  mountedTo?: string[]

  /**
   * 标签
   */
  labels: Record<string, string>

  /**
   * 元数据
   */
  metadata: Record<string, string>
}

/**
 * 卷状态
 */
enum VolumeState {
  /** 创建中 */
  CREATING = 'creating',
  /** 可用 */
  AVAILABLE = 'available',
  /** 使用中 */
  IN_USE = 'in_use',
  /** 删除中 */
  DELETING = 'deleting',
  /** 错误 */
  ERROR = 'error'
}
```

```python
from dataclasses import dataclass
from datetime import datetime
from typing import Optional, Dict, List
from enum import Enum

class VolumeState(str, Enum):
    CREATING = "creating"
    AVAILABLE = "available"
    IN_USE = "in_use"
    DELETING = "deleting"
    ERROR = "error"

@dataclass
class VolumeInfo:
    """卷信息"""
    id: str
    name: str
    size: int
    used_size: int
    state: VolumeState
    filesystem: str
    created_at: datetime
    last_mounted_at: Optional[datetime] = None
    mounted_to: Optional[List[str]] = None
    labels: Dict[str, str] = None
    metadata: Dict[str, str] = None
```

#### CreateVolumeParams

```typescript
/**
 * 创建卷参数
 */
interface CreateVolumeParams {
  /**
   * 卷名称
   * @pattern ^[a-z0-9][a-z0-9-]*[a-z0-9]$
   * @minLength 3
   * @maxLength 63
   */
  name: string

  /**
   * 卷大小（GB）
   * @minimum 1
   * @maximum 1000
   * @default 10
   */
  sizeGB?: number

  /**
   * 文件系统类型
   * @default "ext4"
   */
  filesystem?: 'ext4' | 'xfs'

  /**
   * 标签
   */
  labels?: Record<string, string>

  /**
   * 元数据
   */
  metadata?: Record<string, string>
}
```

### 2.3 VolumeService 接口

#### create

```typescript
/**
 * 创建卷
 *
 * @param params - 创建参数
 * @returns 卷信息
 * @throws {VolumeExistsError} 卷已存在
 * @throws {VolumeLimitExceededError} 超过卷数量限制
 * @throws {StorageInsufficientError} 存储空间不足
 *
 * @example
 * const volume = await client.volume.create({
 *   name: 'my-data',
 *   sizeGB: 50
 * })
 */
create(params: CreateVolumeParams): Promise<VolumeInfo>
```

```python
def create(
    self,
    name: str,
    *,
    size_gb: int = 10,
    filesystem: str = "ext4",
    labels: Optional[Dict[str, str]] = None,
    metadata: Optional[Dict[str, str]] = None,
    timeout: Optional[float] = None
) -> VolumeInfo:
    """
    创建卷

    Args:
        name: 卷名称
        size_gb: 卷大小（GB）
        filesystem: 文件系统类型
        labels: 标签
        metadata: 元数据
        timeout: 超时时间

    Returns:
        卷信息

    Raises:
        VolumeExistsError: 卷已存在
        VolumeLimitExceededError: 超过卷数量限制
        StorageInsufficientError: 存储空间不足
    """
```

#### get

```typescript
/**
 * 获取卷信息
 *
 * @param idOrName - 卷 ID 或名称
 * @returns 卷信息
 * @throws {VolumeNotFoundError} 卷不存在
 *
 * @example
 * const volume = await client.volume.get('vol-abc123')
 * const volume = await client.volume.get('my-data')
 */
get(idOrName: string): Promise<VolumeInfo>
```

#### getOrCreate

```typescript
/**
 * 获取或创建卷
 *
 * 如果卷存在则返回，否则创建新卷。
 *
 * @param name - 卷名称
 * @param params - 创建参数（仅在创建时使用）
 * @returns 卷信息
 *
 * @example
 * // 确保卷存在
 * const volume = await client.volume.getOrCreate('my-data', {
 *   sizeGB: 50
 * })
 */
getOrCreate(name: string, params?: Omit<CreateVolumeParams, 'name'>): Promise<VolumeInfo>
```

#### list

```typescript
/**
 * 列出卷
 *
 * @param params - 查询参数
 * @returns 卷信息列表
 *
 * @example
 * const volumes = await client.volume.list()
 *
 * @example
 * // 按标签筛选
 * const volumes = await client.volume.list({
 *   labels: { project: 'demo' }
 * })
 */
list(params?: {
  /**
   * 按标签筛选
   */
  labels?: Record<string, string>

  /**
   * 按状态筛选
   */
  state?: VolumeState[]

  /**
   * 按名称搜索
   */
  search?: string
}): Promise<VolumeInfo[]>
```

#### delete

```typescript
/**
 * 删除卷
 *
 * @param idOrName - 卷 ID 或名称
 * @param options - 删除选项
 * @throws {VolumeNotFoundError} 卷不存在
 * @throws {VolumeInUseError} 卷正在使用中
 *
 * @example
 * await client.volume.delete('vol-abc123')
 *
 * @example
 * // 强制删除（即使正在使用）
 * await client.volume.delete('vol-abc123', { force: true })
 */
delete(
  idOrName: string,
  options?: {
    /**
     * 是否强制删除
     * @default false
     */
    force?: boolean
  }
): Promise<void>
```

#### resize

```typescript
/**
 * 调整卷大小
 *
 * 只能扩大，不能缩小。
 *
 * @param idOrName - 卷 ID 或名称
 * @param newSizeGB - 新大小（GB）
 * @returns 更新后的卷信息
 * @throws {VolumeNotFoundError} 卷不存在
 * @throws {InvalidSizeError} 新大小小于当前大小
 *
 * @example
 * const volume = await client.volume.resize('vol-abc123', 100)
 */
resize(idOrName: string, newSizeGB: number): Promise<VolumeInfo>
```

---

## 3. REST API

### 3.1 Snapshot 端点

| 方法 | 端点 | 描述 |
|------|------|------|
| POST | `/api/v1/snapshots` | 创建快照 |
| GET | `/api/v1/snapshots` | 列出快照 |
| GET | `/api/v1/snapshots/{id}` | 获取快照 |
| DELETE | `/api/v1/snapshots/{id}` | 删除快照 |

#### 创建快照

**请求**:

```http
POST /api/v1/snapshots
Content-Type: application/json
Authorization: Bearer <token>

{
  "name": "my-snapshot",
  "sandboxId": "sbx-abc123",
  "description": "Before major refactoring",
  "labels": {
    "project": "demo"
  }
}
```

**响应** (201 Created):

```json
{
  "id": "snap-xyz789",
  "name": "my-snapshot",
  "sandboxId": "sbx-abc123",
  "description": "Before major refactoring",
  "size": 1073741824,
  "state": "available",
  "createdAt": "2024-01-15T10:30:00Z",
  "completedAt": "2024-01-15T10:31:00Z",
  "labels": {
    "project": "demo"
  },
  "metadata": {}
}
```

### 3.2 Volume 端点

| 方法 | 端点 | 描述 |
|------|------|------|
| POST | `/api/v1/volumes` | 创建卷 |
| GET | `/api/v1/volumes` | 列出卷 |
| GET | `/api/v1/volumes/{id}` | 获取卷 |
| DELETE | `/api/v1/volumes/{id}` | 删除卷 |
| POST | `/api/v1/volumes/{id}/resize` | 调整大小 |

#### 创建卷

**请求**:

```http
POST /api/v1/volumes
Content-Type: application/json
Authorization: Bearer <token>

{
  "name": "my-data",
  "sizeGB": 50,
  "labels": {
    "project": "demo"
  }
}
```

**响应** (201 Created):

```json
{
  "id": "vol-abc123",
  "name": "my-data",
  "size": 53687091200,
  "usedSize": 0,
  "state": "available",
  "filesystem": "ext4",
  "createdAt": "2024-01-15T10:30:00Z",
  "labels": {
    "project": "demo"
  },
  "metadata": {}
}
```

#### 调整卷大小

**请求**:

```http
POST /api/v1/volumes/vol-abc123/resize
Content-Type: application/json
Authorization: Bearer <token>

{
  "sizeGB": 100
}
```

**响应** (200 OK):

```json
{
  "id": "vol-abc123",
  "name": "my-data",
  "size": 107374182400,
  "usedSize": 1073741824,
  "state": "available",
  "filesystem": "ext4",
  "createdAt": "2024-01-15T10:30:00Z"
}
```

---

## 4. 使用示例

### 4.1 TypeScript 示例

```typescript
import { WorkspaceClient } from '@workspace-sdk/typescript'

async function storageExample() {
  const client = new WorkspaceClient({ apiUrl: 'https://api.example.com' })

  // ========== Volume 使用 ==========
  console.log('=== Volume Example ===')

  // 创建或获取卷
  const volume = await client.volume.getOrCreate('project-data', {
    sizeGB: 50,
    labels: { project: 'demo' }
  })
  console.log(`Volume: ${volume.name} (${volume.size / 1024 / 1024 / 1024} GB)`)

  // 创建 Sandbox 并挂载卷
  const sandbox = await client.sandbox.create({
    template: 'python:3.11',
    volumes: [
      {
        volumeId: volume.id,
        mountPath: '/data',
        readOnly: false
      }
    ]
  })

  try {
    // 写入数据到卷
    await sandbox.fs.write('/data/config.json', JSON.stringify({
      version: '1.0.0',
      createdAt: new Date().toISOString()
    }))

    // 数据会持久化到卷中
    console.log('Data written to volume')

    // ========== Snapshot 使用 ==========
    console.log('\n=== Snapshot Example ===')

    // 创建快照
    const snapshot = await client.snapshot.create({
      name: `backup-${Date.now()}`,
      sandboxId: sandbox.id,
      description: 'Before experiments'
    })
    console.log(`Created snapshot: ${snapshot.name}`)

    // 执行一些可能破坏性的操作
    await sandbox.process.run('rm -rf /app/src/*')

    // 如果出错，可以从快照恢复
    const restoredSandbox = await client.sandbox.create({
      snapshot: snapshot.id,
      name: 'restored-sandbox'
    })
    console.log(`Restored from snapshot: ${restoredSandbox.id}`)

    // 清理
    await restoredSandbox.delete()

    // 列出所有快照
    const snapshots = await client.snapshot.list({
      labels: { project: 'demo' }
    })
    console.log(`Total snapshots: ${snapshots.total}`)

  } finally {
    await sandbox.delete()
  }

  // 卷不会被删除，数据持久化
  const volumeInfo = await client.volume.get(volume.id)
  console.log(`Volume used: ${volumeInfo.usedSize / 1024 / 1024} MB`)
}

storageExample().catch(console.error)
```

### 4.2 Python 示例

```python
from workspace_sdk import WorkspaceClient, VolumeMount
import json

def storage_example():
    client = WorkspaceClient()

    # ========== Volume 使用 ==========
    print('=== Volume Example ===')

    # 创建或获取卷
    volume = client.volume.get_or_create('project-data', size_gb=50)
    print(f'Volume: {volume.name} ({volume.size / 1024 / 1024 / 1024:.1f} GB)')

    # 创建 Sandbox 并挂载卷
    with client.sandbox.create(
        template='python:3.11',
        volumes=[VolumeMount(
            volume_id=volume.id,
            mount_path='/data',
            read_only=False
        )]
    ) as sandbox:
        # 写入数据到卷
        sandbox.fs.write('/data/config.json', json.dumps({
            'version': '1.0.0'
        }))
        print('Data written to volume')

        # ========== Snapshot 使用 ==========
        print('\n=== Snapshot Example ===')

        # 创建快照
        snapshot = client.snapshot.create(
            name=f'backup-{int(time.time())}',
            sandbox_id=sandbox.id,
            description='Before experiments'
        )
        print(f'Created snapshot: {snapshot.name}')

        # 从快照创建新 Sandbox
        with client.sandbox.create(snapshot=snapshot.id) as restored:
            # 验证数据存在
            content = restored.fs.read('/data/config.json')
            print(f'Restored data: {content}')

    # 查看卷使用情况
    volume_info = client.volume.get(volume.id)
    print(f'Volume used: {volume_info.used_size / 1024 / 1024:.1f} MB')

if __name__ == '__main__':
    storage_example()
```

### 4.3 卷共享示例

```typescript
// 多个 Sandbox 共享同一个卷（只读）
async function sharedVolumeExample() {
  const client = new WorkspaceClient({ apiUrl: 'https://api.example.com' })

  // 创建数据卷
  const dataVolume = await client.volume.create({
    name: 'shared-models',
    sizeGB: 100
  })

  // 主 Sandbox 写入数据
  const writer = await client.sandbox.create({
    template: 'python:3.11',
    volumes: [{
      volumeId: dataVolume.id,
      mountPath: '/models',
      readOnly: false
    }]
  })

  await writer.process.run('wget -O /models/model.bin https://example.com/model.bin')
  await writer.stop()

  // 多个 Sandbox 以只读方式共享
  const readers = await Promise.all([
    client.sandbox.create({
      template: 'python:3.11',
      volumes: [{
        volumeId: dataVolume.id,
        mountPath: '/models',
        readOnly: true
      }]
    }),
    client.sandbox.create({
      template: 'python:3.11',
      volumes: [{
        volumeId: dataVolume.id,
        mountPath: '/models',
        readOnly: true
      }]
    })
  ])

  // 所有 reader 都可以访问 /models/model.bin
  for (const reader of readers) {
    const result = await reader.process.run('ls -la /models')
    console.log(result.stdout)
    await reader.delete()
  }

  await writer.delete()
}
```

---

## 5. 错误处理

### 5.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 7000 | SNAPSHOT_NOT_FOUND | 404 | 快照不存在 |
| 7001 | SNAPSHOT_CREATE_FAILED | 500 | 快照创建失败 |
| 7002 | SNAPSHOT_IN_USE | 409 | 快照正在使用中 |
| 7003 | SNAPSHOT_LIMIT_EXCEEDED | 429 | 超过快照数量限制 |
| 7100 | VOLUME_NOT_FOUND | 404 | 卷不存在 |
| 7101 | VOLUME_IN_USE | 409 | 卷正在使用中 |
| 7102 | VOLUME_CREATE_FAILED | 500 | 卷创建失败 |
| 7103 | VOLUME_EXISTS | 409 | 卷已存在 |
| 7104 | VOLUME_LIMIT_EXCEEDED | 429 | 超过卷数量限制 |
| 7105 | INVALID_SIZE | 400 | 无效的大小 |
| 7106 | STORAGE_INSUFFICIENT | 507 | 存储空间不足 |

### 5.2 错误处理示例

```typescript
import {
  VolumeNotFoundError,
  VolumeInUseError,
  SnapshotNotFoundError
} from '@workspace-sdk/typescript'

async function safeVolumeDelete(client: WorkspaceClient, volumeName: string) {
  try {
    await client.volume.delete(volumeName)
    console.log(`Volume ${volumeName} deleted`)
  } catch (error) {
    if (error instanceof VolumeNotFoundError) {
      console.log(`Volume ${volumeName} not found, skipping`)
      return
    }

    if (error instanceof VolumeInUseError) {
      console.log(`Volume ${volumeName} is in use, force deleting...`)
      await client.volume.delete(volumeName, { force: true })
      return
    }

    throw error
  }
}
```

---

## 附录

### A. 资源限制

| 资源 | 默认限制 |
|------|---------|
| 每个组织的快照数 | 100 |
| 每个组织的卷数 | 50 |
| 单个快照最大大小 | 100 GB |
| 单个卷最大大小 | 1000 GB |
| 快照默认保留时间 | 30 天 |
| 卷最小大小 | 1 GB |

### B. 定价考虑

| 资源类型 | 计费方式 |
|---------|---------|
| 快照存储 | 按实际大小计费 |
| 卷存储 | 按分配大小计费 |
| 快照创建 | 一次性费用 |
| 卷 I/O | 按操作次数计费（可选） |

### C. 最佳实践

1. **快照使用**:
   - 在重要操作前创建快照
   - 定期清理不需要的快照
   - 使用标签组织快照

2. **卷使用**:
   - 合理规划卷大小
   - 使用只读挂载共享数据
   - 监控卷使用情况
