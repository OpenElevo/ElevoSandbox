# 统一 Workspace/Sandbox 服务需求文档

> 版本: 1.4.0
> 创建日期: 2025-01-14
> 更新日期: 2025-01-14
> 状态: 草案

---

## 目录

- [1. 项目概述](#1-项目概述)
- [2. 核心需求](#2-核心需求)
- [3. 系统架构](#3-系统架构)
- [4. 功能需求](#4-功能需求)
- [5. 非功能需求](#5-非功能需求)
- [6. API 设计](#6-api-设计)
- [7. SDK 设计](#7-sdk-设计)
- [8. 部署方案](#8-部署方案)
- [9. 安全考虑](#9-安全考虑)
- [10. 风险与待定事项](#10-风险与待定事项)
- [11. 开发路线图](#11-开发路线图)
- [12. 测试策略](#12-测试策略)
- [13. 运维指南](#13-运维指南)

---

## 1. 项目概述

### 1.1 背景

为企业内部 AI Agent 开发运行提供统一的 Workspace 和 Sandbox 服务。Agent 需要安全隔离的代码执行环境来运行用户代码、执行命令等。

### 1.2 目标

1. **统一接口**: 提供标准化的 SDK 接口 (TypeScript + Python)，方便团队对接
2. **安全隔离**: 基于 Docker 容器实现代码执行环境隔离
3. **内置 NFS**: 服务内置 NFS Server，外部可通过 NFS 挂载 workspace 操作文件
4. **可扩展**: 架构设计支持未来扩展到 Kubernetes、Firecracker 等运行时

### 1.3 参考项目

| 项目 | 架构特点 | 借鉴点 |
|-----|---------|--------|
| **E2B** | gRPC Connect (protobuf) + REST/HTTP | 流式操作、envd agent 架构 |
| **Daytona** | REST API + WebSocket | Runner/Toolbox 分层、Docker 集成 |

### 1.4 技术选型

| 组件 | 选型 | 说明 |
|------|------|------|
| **后端语言** | Rust | 高性能、内存安全、优秀的异步支持 |
| **容器运行时** | Docker | 成熟稳定、内部已有基础设施 |
| **API 协议** | REST + WebSocket + gRPC | REST 用于常规操作、WebSocket 用于 PTY、gRPC 用于 Agent 通信 |
| **NFS 实现** | nfsserve / 系统 NFS | 支持内置或系统原生 NFS，按配置切换 |
| **状态存储** | SQLite | 轻量级嵌入式数据库，存储 Sandbox 元数据 |
| **SDK 语言** | TypeScript + Python | 覆盖主流 Agent 开发语言 |

### 1.5 关键设计决策

| 决策 | 说明 |
|------|------|
| **无 FileSystem API** | 文件操作通过 NFS 直接访问，不走 HTTP API |
| **NFS 双模式** | 支持内置 (nfsserve) 和系统原生 (nfs-kernel-server) 两种模式 |
| **Agent 主动连接** | 容器内 Agent 通过 gRPC 主动连接 Server，避免网络穿透问题 |
| **流式命令输出** | Process.run 支持实时流式输出，类似 CLI 体验 |

---

## 2. 核心需求

### 2.1 功能范围 (MVP)

MVP 包含 **3 个服务**、**8 个方法**:

| 服务 | 方法数 | 方法列表 |
|------|--------|---------|
| Sandbox | 4 | create, get, list, delete |
| Process | 1 | run |
| PTY | 3 | create, resize, kill |

### 2.2 文件操作方式

- **不提供 FileSystem HTTP API**
- 外部使用方通过 **NFS 挂载** workspace 目录直接操作文件
- 每个 Sandbox 创建时分配独立的 workspace 目录
- Sandbox 容器内通过 bind mount 访问同一目录

### 2.3 用户角色

| 角色 | 描述 | 使用方式 |
|------|------|---------|
| Agent 开发者 | 创建 Sandbox、执行命令 | SDK + NFS 挂载 |
| 运维人员 | 部署和维护服务 | Docker Compose / K8s |
| 平台管理员 | 管理模板、监控资源 | REST API |

---

## 3. 系统架构

### 3.1 整体架构图

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          外部使用方 (Agent)                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   ┌──────────────────┐           ┌──────────────────┐                   │
│   │  TypeScript SDK   │           │    Python SDK    │                   │
│   └────────┬─────────┘           └────────┬─────────┘                   │
│            │                              │                              │
│            └──────────────┬───────────────┘                              │
│                           │                                              │
│              ┌────────────┴────────────┐                                 │
│              │                         │                                 │
│         REST/WebSocket            NFS Mount                              │
│         (API 操作)              (文件操作)                               │
│              │                         │                                 │
├──────────────┼─────────────────────────┼────────────────────────────────┤
│              ▼                         ▼                                 │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │                    Workspace Server (Rust)                       │   │
│   ├─────────────────────────────────────────────────────────────────┤   │
│   │                                                                  │   │
│   │   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │   │
│   │   │   HTTP API   │  │  WebSocket   │  │  NFS Server  │         │   │
│   │   │   (axum)     │  │   (PTY)      │  │  (nfsserve)  │         │   │
│   │   │   :8080      │  │   :8080/ws   │  │   :2049      │         │   │
│   │   └──────────────┘  └──────────────┘  └──────────────┘         │   │
│   │                                                                  │   │
│   │   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │   │
│   │   │   Sandbox    │  │  Container   │  │  Workspace   │         │   │
│   │   │   Manager    │  │   Manager    │  │   Manager    │         │   │
│   │   └──────────────┘  └──────────────┘  └──────────────┘         │   │
│   │                                                                  │   │
│   └─────────────────────────────────────────────────────────────────┘   │
│                           │                                              │
│           ┌───────────────┼───────────────┐                              │
│           │               │               │                              │
│           ▼               ▼               ▼                              │
│   ┌──────────────┐ ┌──────────────┐ ┌──────────────┐                    │
│   │   Docker     │ │   Docker     │ │   Docker     │                    │
│   │  Container 1 │ │  Container 2 │ │  Container N │                    │
│   │              │ │              │ │              │                    │
│   │ ┌──────────┐ │ │ ┌──────────┐ │ │ ┌──────────┐ │                    │
│   │ │  Agent   │ │ │ │  Agent   │ │ │ │  Agent   │ │                    │
│   │ │(Process+ │ │ │ │(Process+ │ │ │ │(Process+ │ │                    │
│   │ │  PTY)    │ │ │ │  PTY)    │ │ │ │  PTY)    │ │                    │
│   │ └──────────┘ │ │ └──────────┘ │ │ └──────────┘ │                    │
│   │      │       │ │      │       │ │      │       │                    │
│   │ ┌────┴─────┐ │ │ ┌────┴─────┐ │ │ ┌────┴─────┐ │                    │
│   │ │  Bind    │ │ │ │  Bind    │ │ │ │  Bind    │ │                    │
│   │ │  Mount   │ │ │ │  Mount   │ │ │ │  Mount   │ │                    │
│   │ │/workspace│ │ │ │/workspace│ │ │ │/workspace│ │                    │
│   │ └──────────┘ │ │ └──────────┘ │ │ └──────────┘ │                    │
│   └──────────────┘ └──────────────┘ └──────────────┘                    │
│           │               │               │                              │
│           └───────────────┼───────────────┘                              │
│                           │                                              │
│                   ┌───────▼───────┐                                      │
│                   │ /data/workspaces (本地存储)                          │
│                   │  ├── sbx-001/                                        │
│                   │  ├── sbx-002/                                        │
│                   │  └── sbx-xxx/                                        │
│                   └───────────────┘                                      │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 组件说明

#### 3.2.1 Workspace Server (Rust)

单一进程，包含多个子系统:

| 子系统 | 职责 | 端口 |
|--------|------|------|
| HTTP API | REST API 处理 | 8080 |
| WebSocket | PTY 双向通信 | 8080 (升级) |
| NFS Server | 文件系统访问 | 2049 |
| Sandbox Manager | Sandbox 生命周期 | - |
| Container Manager | Docker 容器操作 | - |
| Workspace Manager | 目录管理 | - |

#### 3.2.2 Container Agent

运行在每个容器内的轻量级 agent (Rust 编译的单二进制):

| 功能 | 说明 |
|------|------|
| gRPC 连接 | 启动后主动连接 Server，建立双向通信通道 |
| Process 执行 | 执行命令、流式返回输出 |
| PTY 管理 | 创建/管理伪终端 |
| 健康检查 | 通过 gRPC 心跳上报存活状态 |

**Agent 主动连接机制:**

```
┌─────────────────────────────────────────────────────────────────┐
│                    Server (gRPC Server :50051)                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                   Agent Connection Pool                  │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐              │    │
│  │  │ sbx-001  │  │ sbx-002  │  │ sbx-003  │              │    │
│  │  │ (stream) │  │ (stream) │  │ (stream) │              │    │
│  │  └────▲─────┘  └────▲─────┘  └────▲─────┘              │    │
│  └───────┼─────────────┼─────────────┼─────────────────────┘    │
└──────────┼─────────────┼─────────────┼──────────────────────────┘
           │             │             │
     gRPC Connect   gRPC Connect  gRPC Connect
     (主动连接)     (主动连接)    (主动连接)
           │             │             │
┌──────────┴──┐  ┌───────┴────┐  ┌─────┴──────┐
│  Container  │  │  Container  │  │  Container  │
│   Agent 1   │  │   Agent 2   │  │   Agent 3   │
└─────────────┘  └─────────────┘  └─────────────┘
```

**通信流程:**

1. Container 启动后，Agent 读取环境变量获取 Server 地址
2. Agent 主动发起 gRPC 连接到 Server (携带 sandbox_id)
3. Server 验证 sandbox_id，将连接加入连接池
4. Server 通过已建立的 gRPC stream 下发命令
5. Agent 执行命令，通过 stream 返回结果
6. 连接断开时 Server 标记 Sandbox 状态为异常

**注意**: Agent 不处理文件操作，文件通过 NFS 直接访问。

#### 3.2.3 NFS Server (双模式)

支持两种 NFS 实现模式，通过配置切换：

**模式 A: 内置模式 (embedded)**

使用 `nfsserve` Rust crate 实现的 NFSv3 服务器：

| 特性 | 说明 |
|------|------|
| 协议 | NFSv3 |
| 端口 | 2049 (单端口，含 Mount) |
| 优点 | 无外部依赖，部署简单，跨平台 |
| 缺点 | 性能略低于内核实现，功能受限 |
| 适用场景 | 容器化部署、快速测试、macOS/Windows 开发环境 |

**模式 B: 系统模式 (system)**

使用 Linux 系统原生的 `nfs-kernel-server`：

| 特性 | 说明 |
|------|------|
| 协议 | NFSv3 / NFSv4 |
| 端口 | 2049 (NFS) + 111 (portmapper) |
| 优点 | 内核级性能，稳定成熟，功能完整 |
| 缺点 | 需要 root 权限，需安装系统包，仅 Linux |
| 适用场景 | 生产环境、大文件操作、高性能需求 |

**系统模式依赖:**

```bash
# Ubuntu/Debian
apt-get install nfs-kernel-server

# CentOS/RHEL
yum install nfs-utils
```

**共同特性:**

- 导出 `/data/workspaces` 目录
- 每个 Sandbox 有独立子目录
- 支持读写操作
- 外部客户端通过标准 NFS 挂载

### 3.3 数据流

#### 3.3.1 Sandbox 创建流程

```
SDK                    Server                 Docker              Agent
 │                        │                      │                   │
 │  POST /sandboxes       │                      │                   │
 │───────────────────────>│                      │                   │
 │                        │                      │                   │
 │                        │  mkdir workspace     │                   │
 │                        │─────────────────────>│                   │
 │                        │                      │                   │
 │                        │  create container    │                   │
 │                        │─────────────────────>│                   │
 │                        │                      │                   │
 │                        │  start container     │                   │
 │                        │─────────────────────>│                   │
 │                        │                      │  start agent      │
 │                        │                      │──────────────────>│
 │                        │                      │                   │
 │                        │  wait for ready      │   health check    │
 │                        │<─────────────────────│<──────────────────│
 │                        │                      │                   │
 │  Sandbox (id, nfs_path)│                      │                   │
 │<───────────────────────│                      │                   │
 │                        │                      │                   │
```

#### 3.3.2 命令执行流程 (gRPC 双向流)

```
SDK                    Server                 Agent (gRPC stream)
 │                        │                      │
 │  POST /process/run     │                      │
 │───────────────────────>│                      │
 │                        │                      │
 │                        │  RunCommandRequest   │
 │                        │  (via gRPC stream)   │
 │                        │─────────────────────>│
 │                        │                      │
 │                        │  CommandOutput       │
 │                        │  (stdout/stderr)     │
 │                        │<─────────────────────│
 │                        │        ...           │
 │                        │  CommandOutput       │
 │                        │  (exit)              │
 │                        │<─────────────────────│
 │                        │                      │
 │    CommandResult       │                      │
 │    (或 SSE 流)         │                      │
 │<───────────────────────│                      │
 │                        │                      │
```

**说明**: Agent 启动后主动连接 Server 建立 gRPC 双向流，Server 通过该流下发命令，Agent 通过同一流返回输出。

#### 3.3.3 文件操作流程

```
SDK/User               NFS Server            Filesystem
   │                        │                      │
   │  mount nfs://...       │                      │
   │───────────────────────>│                      │
   │                        │                      │
   │  read/write file       │                      │
   │───────────────────────>│                      │
   │                        │  read/write          │
   │                        │─────────────────────>│
   │                        │                      │
   │        data            │        data          │
   │<───────────────────────│<─────────────────────│
   │                        │                      │
```

### 3.4 状态持久化

服务使用 SQLite 存储 Sandbox 元数据，确保服务重启后能恢复状态。

#### 3.4.1 数据模型

```sql
-- Sandbox 表
CREATE TABLE sandboxes (
    id TEXT PRIMARY KEY,
    name TEXT,
    state TEXT NOT NULL DEFAULT 'starting',
    template TEXT NOT NULL,
    container_id TEXT,
    workspace_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    config_json TEXT,  -- 资源配置、环境变量等
    updated_at INTEGER NOT NULL
);

-- 索引
CREATE INDEX idx_sandboxes_state ON sandboxes(state);
CREATE INDEX idx_sandboxes_expires_at ON sandboxes(expires_at);
```

#### 3.4.2 服务启动恢复流程

```
Server 启动
    │
    ▼
读取 SQLite 中所有 Sandbox
    │
    ▼
┌───────────────────────────────────────────┐
│  遍历每个 Sandbox                          │
│  ┌─────────────────────────────────────┐  │
│  │ 检查 Docker 容器状态                  │  │
│  │   - 容器存在且运行中 → 等待 Agent 重连 │  │
│  │   - 容器存在但停止 → 标记为 stopped   │  │
│  │   - 容器不存在 → 标记为 error        │  │
│  └─────────────────────────────────────┘  │
└───────────────────────────────────────────┘
    │
    ▼
启动超时清理定时任务
    │
    ▼
启动 API/NFS/gRPC 服务
```

### 3.5 超时清理机制

#### 3.5.1 自动清理

后台定时任务每分钟检查一次过期的 Sandbox：

```
┌─────────────────────────────────────────┐
│           超时清理定时任务                │
│                                          │
│  每 60 秒执行:                           │
│  1. 查询 expires_at < now() 的 Sandbox   │
│  2. 对每个过期 Sandbox:                   │
│     - 发送即将过期事件 (如有 webhook)     │
│     - 停止并删除 Docker 容器             │
│     - 移除 NFS 导出                      │
│     - 删除 workspace 目录               │
│     - 更新数据库状态                     │
│  3. 记录清理日志                         │
└─────────────────────────────────────────┘
```

#### 3.5.2 延长超时 API

```
POST /api/v1/sandboxes/{id}/extend
{
  "seconds": 3600  // 延长的秒数
}
```

**限制:**
- 单次最大延长: 3600 秒 (1 小时)
- 累计最大存活时间: 可配置，默认 24 小时

### 3.6 Webhook 事件通知

支持配置 Webhook URL，在关键事件发生时发送 HTTP POST 通知。

#### 3.6.1 配置

| 配置项 | 说明 |
|--------|------|
| `WEBHOOK_URL` | Webhook 接收地址 |
| `WEBHOOK_SECRET` | HMAC 签名密钥 (可选) |
| `WEBHOOK_TIMEOUT` | 请求超时，默认 5s |
| `WEBHOOK_RETRY` | 重试次数，默认 3 |

#### 3.6.2 事件类型

| 事件 | 触发时机 | 说明 |
|------|---------|------|
| `sandbox.created` | Sandbox 创建完成 | 包含 Sandbox 完整信息 |
| `sandbox.starting` | Sandbox 开始启动 | 状态变更为 starting |
| `sandbox.running` | Sandbox 运行中 | Agent 已连接就绪 |
| `sandbox.expiring` | 即将过期 | 过期前 5 分钟触发 |
| `sandbox.expired` | 已过期 | 自动清理前触发 |
| `sandbox.deleted` | Sandbox 已删除 | 主动删除或过期清理 |
| `sandbox.error` | Sandbox 异常 | Agent 连接断开、容器崩溃等 |
| `agent.connected` | Agent 已连接 | Agent gRPC 连接建立 |
| `agent.disconnected` | Agent 已断开 | Agent gRPC 连接断开 |

#### 3.6.3 Webhook 请求格式

```http
POST {WEBHOOK_URL}
Content-Type: application/json
X-Webhook-Event: sandbox.created
X-Webhook-Timestamp: 1705312345678
X-Webhook-Signature: sha256=abc123...  # 如果配置了 WEBHOOK_SECRET

{
  "event": "sandbox.created",
  "timestamp": "2024-01-15T10:30:00.123Z",
  "data": {
    "sandboxId": "sbx-abc123",
    "name": "my-sandbox",
    "state": "running",
    "template": "python:3.11",
    "createdAt": "2024-01-15T10:30:00.000Z",
    "expiresAt": "2024-01-15T11:30:00.000Z"
  }
}
```

**签名验证 (HMAC-SHA256):**

```python
import hmac
import hashlib

def verify_signature(payload: bytes, signature: str, secret: str) -> bool:
    expected = hmac.new(
        secret.encode(),
        payload,
        hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(f"sha256={expected}", signature)
```

---

## 4. 功能需求

### 4.1 Sandbox 服务

#### 4.1.1 create

创建新的 Sandbox 实例。

**输入参数:**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| template | string | 是 | 模板/镜像名称，如 "python:3.11" |
| name | string | 否 | Sandbox 名称，用于标识 |
| envs | map<string, string> | 否 | 环境变量 |
| timeout_seconds | int | 否 | 自动销毁时间，默认 3600 |
| resources | ResourceConfig | 否 | 资源配置 |

**ResourceConfig:**

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| cpu | string | 否 | "2" | CPU 核数，如 "1", "2", "0.5" |
| memory | string | 否 | "2g" | 内存大小，如 "512m", "2g", "4g" |
| disk | string | 否 | "10g" | Workspace 磁盘限制 |

**输出:**

```typescript
interface Sandbox {
  id: string              // 唯一标识，如 "sbx-abc123"
  name?: string           // 名称
  state: SandboxState     // 状态
  createdAt: Date
  expiresAt: Date         // 过期时间

  // NFS 挂载信息
  nfs: {
    host: string          // NFS 服务器地址
    port: number          // NFS 端口 (默认 2049)
    path: string          // 导出路径，如 "/sbx-abc123"
    mountCommand: string  // 完整挂载命令示例
  }

  // 子服务
  process: ProcessService
  pty: PTYService
}

// Sandbox 状态
type SandboxState =
  | 'starting'    // 正在启动
  | 'running'     // 运行中
  | 'stopping'    // 正在停止
  | 'stopped'     // 已停止
  | 'error'       // 异常
```

**处理流程:**

1. 验证 template 是否存在
2. 生成唯一 sandbox_id (如 `sbx-{uuid}`)
3. 创建 workspace 目录: `/data/workspaces/{sandbox_id}/`
4. 创建 Docker 容器:
   - Bind mount workspace 目录到容器 `/workspace`
   - 设置环境变量
   - 启动 Agent 进程
5. 等待 Agent 就绪 (健康检查)
6. 返回 Sandbox 信息 (包含 NFS 挂载路径)

**Docker 容器配置:**

```yaml
container:
  image: ${template}
  working_dir: /workspace
  volumes:
    - /data/workspaces/${sandbox_id}:/workspace
  environment:
    - WORKSPACE_SERVER=${SERVER_HOST}:50051  # gRPC 连接地址
    - SANDBOX_ID=${sandbox_id}
    - ${user_envs}
  command: ["/usr/local/bin/workspace-agent"]
  network_mode: bridge
  # 资源限制
  resources:
    limits:
      cpus: '2'
      memory: 2g
```

**网络配置说明:**

Agent 需要能够访问 Server 的 gRPC 端口 (50051)。根据部署方式不同：

| 部署方式 | `WORKSPACE_SERVER` 值 | 说明 |
|---------|----------------------|------|
| Server 宿主机部署 | `host.docker.internal:50051` | Docker Desktop 方式 |
| Server 宿主机部署 (Linux) | `172.17.0.1:50051` | Docker 网桥网关 IP |
| Server 容器部署 | `workspace-server:50051` | 同一 Docker 网络 |

#### 4.1.2 get

获取指定 Sandbox 信息。

**输入:** sandbox_id (string)
**输出:** Sandbox 对象

#### 4.1.3 list

列出 Sandbox，支持分页和过滤。

**输入参数 (Query):**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| page | int | 否 | 页码，默认 1 |
| limit | int | 否 | 每页数量，默认 20，最大 100 |
| state | string | 否 | 状态过滤: running, stopped, error |
| name_prefix | string | 否 | 名称前缀过滤 |

**输出:**

```typescript
interface ListResponse {
  items: Sandbox[]
  total: number
  page: number
  limit: number
}
```

#### 4.1.4 delete

删除指定 Sandbox。

**处理流程:**

1. 停止并删除 Docker 容器
2. (可选) 删除 workspace 目录
3. 清理内部状态

**参数:**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| sandbox_id | string | 是 | Sandbox ID |
| keep_workspace | bool | 否 | 是否保留 workspace 目录，默认 false |

#### 4.1.5 batchDelete

批量删除 Sandbox。

**输入:**

```typescript
interface BatchDeleteRequest {
  ids: string[]           // Sandbox ID 列表，最多 50 个
  keep_workspace?: bool   // 是否保留 workspace 目录
}
```

**输出:**

```typescript
interface BatchDeleteResponse {
  succeeded: string[]     // 成功删除的 ID
  failed: Array<{
    id: string
    error: string
  }>
}
```

#### 4.1.6 logs

获取 Sandbox 容器日志。

**输入参数 (Query):**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| tail | int | 否 | 返回最后 N 行，默认 100 |
| since | string | 否 | 起始时间 (ISO8601) |
| until | string | 否 | 结束时间 (ISO8601) |

**输出:**

```typescript
interface LogsResponse {
  logs: string            // 日志内容
  truncated: boolean      // 是否被截断
}
```

#### 4.1.7 extend

延长 Sandbox 超时时间。

**输入:**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| seconds | int | 是 | 延长的秒数，最大 3600 |

**输出:** 更新后的 Sandbox 对象

#### 4.1.8 stats

获取单个 Sandbox 的资源使用统计。

**输入:** sandbox_id (path parameter)

**输出:**

```typescript
interface SandboxStats {
  sandboxId: string
  cpu: {
    usage: number         // 当前 CPU 使用率 (0-100)
    limit: string         // CPU 限制，如 "2"
  }
  memory: {
    usage: number         // 当前内存使用 (bytes)
    limit: number         // 内存限制 (bytes)
    percent: number       // 使用率 (0-100)
  }
  disk: {
    usage: number         // workspace 目录大小 (bytes)
    limit?: number        // 磁盘限制 (bytes)，无限制时为空
  }
  network: {
    rxBytes: number       // 接收字节数
    txBytes: number       // 发送字节数
  }
  processes: number       // 进程数量
  updatedAt: Date         // 统计时间
}
```

### 4.2 Process 服务

#### 4.2.1 run

执行命令，支持两种模式：同步等待和流式输出。

**输入:**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| command | string | 是 | 命令字符串 |
| cwd | string | 否 | 工作目录，默认 /workspace |
| envs | map<string, string> | 否 | 环境变量 |
| timeout | int | 否 | 超时时间 (ms)，默认 60000 |
| stream | bool | 否 | 是否流式输出，默认 false |
| maxOutputSize | int | 否 | 最大输出大小 (bytes)，默认 1MB |

**输出 (同步模式, stream=false):**

```typescript
interface CommandResult {
  commandId: string     // 命令 ID，可用于 kill
  exitCode: number
  stdout: string
  stderr: string
  truncated: boolean    // 输出是否被截断
}
```

**输出 (流式模式, stream=true):**

返回 Server-Sent Events (SSE) 流或 WebSocket 连接：

```typescript
// StreamEvent
interface StreamEvent {
  type: 'stdout' | 'stderr' | 'exit' | 'error'
  commandId: string      // 命令 ID
  data?: string          // 输出内容 (stdout/stderr)
  exitCode?: number      // 退出码 (exit)
  message?: string       // 错误信息 (error)
  timestamp: number      // 时间戳 (ms)
}

// 示例 SSE 流
// event: stdout
// data: {"type":"stdout","data":"Building project...\\n","commandId":"cmd-123","timestamp":1705312345678}
//
// event: stdout
// data: {"type":"stdout","data":"[1/10] Compiling...\\n","commandId":"cmd-123","timestamp":1705312345700}
//
// event: stderr
// data: {"type":"stderr","data":"warning: unused variable\\n","commandId":"cmd-123","timestamp":1705312345800}
//
// event: exit
// data: {"type":"exit","exitCode":0,"commandId":"cmd-123","timestamp":1705312350000}
```

**REST API 端点:**

```
# 同步模式
POST /api/v1/sandboxes/{id}/process/run
Content-Type: application/json
{"command": "python main.py", "stream": false}

# 流式模式 (SSE)
POST /api/v1/sandboxes/{id}/process/run
Content-Type: application/json
Accept: text/event-stream
{"command": "python main.py", "stream": true}
```

**执行方式:** 通过 `/bin/sh -c` 执行，支持管道、重定向等 shell 特性。

**注意:** 命令在容器内执行，可以访问 `/workspace` 目录下的文件 (即 NFS 导出的文件)。

#### 4.2.2 kill

终止正在执行的命令。

**输入:**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| commandId | string | 是 | 命令 ID (从 run 返回或 SSE 事件中获取) |
| signal | int | 否 | 信号，默认 15 (SIGTERM)，可选 9 (SIGKILL) |

**输出:** 无

**REST API 端点:**

```
POST /api/v1/sandboxes/{id}/process/{commandId}/kill
Content-Type: application/json
{"signal": 15}
```

### 4.3 PTY 服务

提供交互式终端能力。

#### 4.3.1 create

创建 PTY 会话。

**输入:**

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| cols | int | 是 | 终端列数 |
| rows | int | 是 | 终端行数 |
| shell | string | 否 | Shell 程序，默认 "/bin/bash" |
| cwd | string | 否 | 工作目录，默认 /workspace |
| envs | map<string, string> | 否 | 环境变量 |

**输出:**

```typescript
interface PTYSession {
  id: string              // PTY 会话 ID
  websocketUrl: string    // WebSocket 连接 URL
}
```

**WebSocket 消息格式:**

```typescript
// 客户端 -> 服务器
interface ClientMessage {
  type: 'input' | 'resize' | 'ping'
  data?: string        // base64 encoded (for input)
  cols?: number        // (for resize)
  rows?: number        // (for resize)
}

// 服务器 -> 客户端
interface ServerMessage {
  type: 'output' | 'exit' | 'pong' | 'error'
  data?: string        // base64 encoded (for output)
  exitCode?: number    // (for exit)
  message?: string     // (for error)
}
```

**WebSocket 心跳机制:**

| 参数 | 值 | 说明 |
|------|-----|------|
| 心跳间隔 | 30s | 客户端发送 ping |
| 超时时间 | 90s | 无心跳则断开连接 |
| 重连策略 | 指数退避 | 1s, 2s, 4s, 8s... 最大 30s |

#### 4.3.2 resize

调整 PTY 大小。

#### 4.3.3 kill

关闭 PTY 会话。

### 4.4 模板管理

模板是预配置的 Docker 镜像，包含 Agent 二进制和运行环境。

#### 4.4.1 模板规范

每个模板镜像必须满足：

| 要求 | 说明 |
|------|------|
| Agent 二进制 | 包含 `/usr/local/bin/workspace-agent` |
| 工作目录 | 支持 `/workspace` 作为工作目录 |
| Shell | 包含 `/bin/sh`，推荐包含 `/bin/bash` |
| 用户 | 支持非 root 用户运行 |

**基础镜像 Dockerfile 示例:**

```dockerfile
FROM ubuntu:22.04

# 安装基础工具
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# 复制 Agent 二进制
COPY workspace-agent /usr/local/bin/workspace-agent
RUN chmod +x /usr/local/bin/workspace-agent

# 创建非 root 用户
RUN useradd -m -s /bin/bash sandbox
USER sandbox
WORKDIR /workspace

# 启动 Agent
ENTRYPOINT ["/usr/local/bin/workspace-agent"]
```

#### 4.4.2 list (模板列表)

列出可用模板。

**输出:**

```typescript
interface Template {
  name: string          // 如 "python:3.11"
  description?: string  // 描述
  tags: string[]        // 标签，如 ["python", "ml"]
  size: number          // 镜像大小 (bytes)
  createdAt: Date
}
```

#### 4.4.3 内置模板

| 模板名 | 基础镜像 | 说明 |
|--------|---------|------|
| `base` | ubuntu:22.04 | 基础环境 |
| `python:3.11` | python:3.11-slim | Python 开发 |
| `python:3.12` | python:3.12-slim | Python 开发 |
| `node:18` | node:18-slim | Node.js 开发 |
| `node:20` | node:20-slim | Node.js 开发 |
| `go:1.21` | golang:1.21-alpine | Go 开发 |

### 4.5 系统管理 API

#### 4.5.1 stats

获取系统统计信息。

**输出:**

```typescript
interface SystemStats {
  sandboxes: {
    total: number
    running: number
    stopped: number
    error: number
  }
  resources: {
    cpuUsage: number      // 0-100
    memoryUsage: number   // 0-100
    diskUsage: number     // 0-100
  }
  nfs: {
    mode: 'embedded' | 'system'
    connections: number
  }
  uptime: number          // 秒
}
```

#### 4.5.2 gc

手动触发垃圾回收。

清理过期的 Sandbox 和孤立的 workspace 目录。

**输出:**

```typescript
interface GCResult {
  cleanedSandboxes: number
  cleanedWorkspaces: number
  freedSpace: number      // bytes
}
```

---

## 5. 非功能需求

### 5.1 性能要求

| 指标 | 目标值 | 说明 |
|------|--------|------|
| Sandbox 创建延迟 | < 5s | 从 API 调用到可用 |
| API 响应延迟 (P99) | < 200ms | 不含命令执行 |
| 单实例并发 Sandbox | 100+ | 取决于硬件资源 |
| NFS 吞吐量 | 100MB/s+ | 取决于磁盘性能 |

### 5.2 可靠性要求

| 需求 | 说明 |
|------|------|
| 服务可用性 | 99.9% (单点部署情况下) |
| 容器隔离 | 一个 Sandbox 崩溃不影响其他 Sandbox |
| 状态恢复 | 服务重启后能恢复运行中的 Sandbox 信息 |
| 超时保护 | 命令执行超时自动终止 |

### 5.3 可观测性

| 类型 | 实现 |
|------|------|
| 日志 | 结构化日志 (JSON)，支持日志级别 |
| 指标 | Prometheus 格式指标导出 |
| 健康检查 | /health 端点 |

**关键指标:**

- `sandbox_total{state}`: Sandbox 总数
- `sandbox_create_duration_seconds`: 创建耗时直方图
- `api_request_duration_seconds{endpoint}`: API 请求耗时
- `api_request_total{endpoint,status}`: API 请求数
- `nfs_operations_total{op}`: NFS 操作数
- `nfs_bytes_total{direction}`: NFS 传输字节数

### 5.4 资源限制

| 资源 | 默认限制 | 可配置 |
|------|---------|--------|
| 单用户 Sandbox 数量 | 10 | 是 |
| 单 Sandbox CPU | 2 核 | 是 |
| 单 Sandbox 内存 | 2GB | 是 |
| Sandbox 最大存活时间 | 1 小时 | 是 |
| 命令执行超时 | 60s | 是 |
| 单 Sandbox PTY 数量 | 5 | 是 |
| Workspace 目录大小 | 无限制 | 可配置 |

---

## 6. API 设计

### 6.1 REST API 端点

| 方法 | 路径 | 说明 |
|------|------|------|
| **Sandbox** |
| POST | /api/v1/sandboxes | 创建 Sandbox |
| GET | /api/v1/sandboxes | 列出 Sandbox (支持分页) |
| GET | /api/v1/sandboxes/{id} | 获取 Sandbox |
| DELETE | /api/v1/sandboxes/{id} | 删除 Sandbox |
| POST | /api/v1/sandboxes/batch-delete | 批量删除 |
| GET | /api/v1/sandboxes/{id}/logs | 获取容器日志 |
| GET | /api/v1/sandboxes/{id}/stats | 获取 Sandbox 资源统计 |
| POST | /api/v1/sandboxes/{id}/extend | 延长超时 |
| **Process** |
| POST | /api/v1/sandboxes/{id}/process/run | 执行命令 (支持流式) |
| POST | /api/v1/sandboxes/{id}/process/{cmdId}/kill | 终止命令 |
| **PTY** |
| POST | /api/v1/sandboxes/{id}/pty | 创建 PTY |
| POST | /api/v1/sandboxes/{id}/pty/{ptyId}/resize | 调整大小 |
| DELETE | /api/v1/sandboxes/{id}/pty/{ptyId} | 关闭 PTY |
| WS | /api/v1/sandboxes/{id}/pty/{ptyId}/ws | WebSocket 连接 |
| **模板** |
| GET | /api/v1/templates | 列出可用模板 |
| **系统** |
| GET | /health | 健康检查 |
| GET | /ready | 就绪检查 |
| GET | /metrics | Prometheus 指标 |
| GET | /api/v1/stats | 系统统计 |
| POST | /api/v1/gc | 手动垃圾回收 |

### 6.2 gRPC API (内部)

Server 与 Agent 之间的 gRPC 接口：

```protobuf
syntax = "proto3";
package workspace.agent.v1;

service AgentService {
  // Agent 注册并建立双向流
  rpc Connect(stream AgentMessage) returns (stream ServerMessage);
}

// ==================== Agent -> Server ====================

message AgentMessage {
  string sandbox_id = 1;
  oneof payload {
    RegisterRequest register = 2;
    Heartbeat heartbeat = 3;
    CommandOutput command_output = 4;
    PTYOutput pty_output = 5;
  }
}

message RegisterRequest {
  string sandbox_id = 1;
  string agent_version = 2;
  map<string, string> capabilities = 3;  // Agent 能力，如 {"pty": "true"}
}

message Heartbeat {
  int64 timestamp = 1;
  ResourceUsage resource_usage = 2;  // 可选：上报资源使用情况
}

message ResourceUsage {
  double cpu_percent = 1;
  uint64 memory_bytes = 2;
  uint64 disk_bytes = 3;
}

message CommandOutput {
  string command_id = 1;
  OutputType type = 2;
  bytes data = 3;
  int32 exit_code = 4;  // 仅 EXIT 时有效
  int64 timestamp = 5;
}

enum OutputType {
  OUTPUT_TYPE_UNSPECIFIED = 0;
  OUTPUT_TYPE_STDOUT = 1;
  OUTPUT_TYPE_STDERR = 2;
  OUTPUT_TYPE_EXIT = 3;
  OUTPUT_TYPE_ERROR = 4;
}

message PTYOutput {
  string pty_id = 1;
  oneof payload {
    bytes data = 2;          // 终端输出
    int32 exit_code = 3;     // PTY 退出
    string error = 4;        // 错误信息
  }
}

// ==================== Server -> Agent ====================

message ServerMessage {
  oneof payload {
    RegisterResponse register_response = 1;
    RunCommandRequest run_command = 2;
    KillCommandRequest kill_command = 3;
    CreatePTYRequest create_pty = 4;
    PTYInput pty_input = 5;
    ResizePTYRequest resize_pty = 6;
    KillPTYRequest kill_pty = 7;
  }
}

message RegisterResponse {
  bool success = 1;
  string message = 2;
  ServerConfig config = 3;
}

message ServerConfig {
  int32 heartbeat_interval_seconds = 1;  // 心跳间隔，默认 30
  int32 max_output_buffer_size = 2;      // 输出缓冲区大小
}

message RunCommandRequest {
  string command_id = 1;
  string command = 2;
  string cwd = 3;
  map<string, string> envs = 4;
  int32 timeout_ms = 5;
}

message KillCommandRequest {
  string command_id = 1;
  int32 signal = 2;  // SIGTERM=15, SIGKILL=9
}

message CreatePTYRequest {
  string pty_id = 1;
  int32 cols = 2;
  int32 rows = 3;
  string shell = 4;
  string cwd = 5;
  map<string, string> envs = 6;
}

message PTYInput {
  string pty_id = 1;
  bytes data = 2;
}

message ResizePTYRequest {
  string pty_id = 1;
  int32 cols = 2;
  int32 rows = 3;
}

message KillPTYRequest {
  string pty_id = 1;
}
```

### 6.3 NFS 访问

| 属性 | 值 |
|------|-----|
| 协议 | NFSv3 |
| 端口 | 2049 (默认) |
| 导出路径 | /{sandbox_id} |

**挂载示例 (Linux):**

```bash
mount -t nfs -o vers=3,tcp,nolock ${SERVER_IP}:/${SANDBOX_ID} /local/path
```

**挂载示例 (macOS):**

```bash
mount_nfs -o vers=3,tcp,nolock ${SERVER_IP}:/${SANDBOX_ID} /local/path
```

### 6.3 认证方式

**HTTP API:**

```
Authorization: Bearer <api_key>
```

或:

```
X-API-Key: <api_key>
```

**NFS:**

- MVP 阶段: 基于 IP 白名单
- 后续可考虑: Kerberos 认证

### 6.4 错误响应格式

```json
{
  "error": {
    "code": 2001,
    "name": "SANDBOX_NOT_FOUND",
    "message": "Sandbox 'sbx-abc123' not found",
    "details": {
      "sandboxId": "sbx-abc123"
    },
    "requestId": "req-xyz789"
  }
}
```

### 6.5 错误码定义

| 范围 | 类别 | 常见错误 |
|-----|------|---------|
| 1000-1999 | 认证 | UNAUTHORIZED, FORBIDDEN |
| 2000-2999 | Sandbox | SANDBOX_NOT_FOUND, TEMPLATE_NOT_FOUND |
| 4000-4099 | Process | PROCESS_TIMEOUT, COMMAND_FAILED |
| 4100-4199 | PTY | PTY_NOT_FOUND, PTY_LIMIT_EXCEEDED |
| 9000-9999 | 系统 | INTERNAL_ERROR, RATE_LIMITED |

---

## 7. SDK 设计

### 7.1 TypeScript SDK

**包名:** `@elevo/workspace-sdk`

**客户端配置:**

```typescript
interface ClientConfig {
  apiUrl: string              // API 服务地址
  apiKey: string              // API Key
  timeout?: number            // 请求超时 (ms)，默认 30000
  retries?: number            // 重试次数，默认 3
  retryDelay?: number         // 重试间隔 (ms)，默认 1000
  retryBackoff?: number       // 退避系数，默认 2
}
```

**使用示例:**

```typescript
import { WorkspaceClient } from '@elevo/workspace-sdk'

const client = new WorkspaceClient({
  apiUrl: 'http://workspace-server:8080',
  apiKey: process.env.WORKSPACE_API_KEY,
  timeout: 30000,
  retries: 3
})

// 创建 Sandbox
const sandbox = await client.sandbox.create({
  template: 'python:3.11',
  name: 'my-sandbox'
})

console.log('Sandbox created:', sandbox.id)
console.log('NFS mount command:', sandbox.nfs.mountCommand)

// 挂载 NFS (示例，实际使用时可能需要 sudo)
// execSync(sandbox.nfs.mountCommand)

// 或者让用户自行挂载后操作文件
// fs.writeFileSync('/mnt/workspace/main.py', 'print("Hello")')

// 执行命令
const result = await sandbox.process.run('python /workspace/main.py')
console.log('Output:', result.stdout)

// 清理
await client.sandbox.delete(sandbox.id)
```

**SDK 结构:**

```typescript
interface WorkspaceClient {
  sandbox: SandboxService
  templates: TemplateService
}

interface SandboxService {
  create(params: CreateSandboxParams): Promise<Sandbox>
  get(id: string): Promise<Sandbox>
  list(params?: ListParams): Promise<ListResponse<Sandbox>>
  delete(id: string, options?: DeleteOptions): Promise<void>
  batchDelete(ids: string[], options?: DeleteOptions): Promise<BatchDeleteResponse>
  logs(id: string, options?: LogsOptions): Promise<LogsResponse>
  stats(id: string): Promise<SandboxStats>
  extend(id: string, seconds: number): Promise<Sandbox>
}

interface Sandbox {
  id: string
  name?: string
  state: SandboxState
  createdAt: Date
  expiresAt: Date
  nfs: NFSInfo

  // 子服务绑定到此 Sandbox
  process: ProcessService
  pty: PTYService
}

interface ProcessService {
  // 同步执行
  run(command: string, options?: RunOptions): Promise<CommandResult>

  // 流式执行
  runStream(command: string, options?: RunOptions): AsyncIterable<StreamEvent>

  // 终止命令
  kill(commandId: string, signal?: number): Promise<void>
}

interface PTYService {
  create(options: PTYOptions): Promise<PTYHandle>
  resize(ptyId: string, cols: number, rows: number): Promise<void>
  kill(ptyId: string): Promise<void>
}

interface TemplateService {
  list(): Promise<Template[]>
}

// 错误类型
export class WorkspaceError extends Error {
  code: number
  name: string
}
export class SandboxNotFoundError extends WorkspaceError {}
export class TemplateNotFoundError extends WorkspaceError {}
export class SandboxLimitExceededError extends WorkspaceError {}
export class ProcessTimeoutError extends WorkspaceError {}
export class CommandFailedError extends WorkspaceError {}
export class PTYNotFoundError extends WorkspaceError {}
export class PTYLimitExceededError extends WorkspaceError {}
export class UnauthorizedError extends WorkspaceError {}
export class RateLimitedError extends WorkspaceError {}
```

**资源管理模式 (自动清理):**

```typescript
// 方式 1: 使用 using 声明 (TypeScript 5.2+, Node.js 18+)
import { WorkspaceClient } from '@elevo/workspace-sdk'

async function main() {
  const client = new WorkspaceClient({...})

  await using sandbox = await client.sandbox.create({
    template: 'python:3.11'
  })

  // 使用 sandbox...
  const result = await sandbox.process.run('python --version')
  console.log(result.stdout)

  // 退出作用域时自动调用 sandbox[Symbol.asyncDispose]() 删除
}

// 方式 2: 使用辅助函数 (兼容旧版本)
import { withSandbox } from '@elevo/workspace-sdk'

await withSandbox(client, { template: 'python:3.11' }, async (sandbox) => {
  const result = await sandbox.process.run('python --version')
  console.log(result.stdout)
  // 回调结束后自动删除
})

// 方式 3: try-finally (传统方式)
const sandbox = await client.sandbox.create({ template: 'python:3.11' })
try {
  const result = await sandbox.process.run('python --version')
  console.log(result.stdout)
} finally {
  await client.sandbox.delete(sandbox.id)
}
```

**流式输出示例:**

```typescript
// 流式执行命令
const stream = sandbox.process.runStream('npm install')

for await (const event of stream) {
  switch (event.type) {
    case 'stdout':
      process.stdout.write(event.data)
      break
    case 'stderr':
      process.stderr.write(event.data)
      break
    case 'exit':
      console.log(`\nExit code: ${event.exitCode}`)
      break
  }
}
```

### 7.2 Python SDK

**包名:** `elevo-workspace-sdk`

**客户端配置:**

```python
@dataclass
class ClientConfig:
    api_url: str                    # API 服务地址
    api_key: str                    # API Key
    timeout: float = 30.0           # 请求超时 (秒)
    retries: int = 3                # 重试次数
    retry_delay: float = 1.0        # 重试间隔 (秒)
    retry_backoff: float = 2.0      # 退避系数
```

**使用示例:**

```python
import asyncio
from workspace_sdk import WorkspaceClient, CreateSandboxParams

async def main():
    client = WorkspaceClient(
        api_url="http://workspace-server:8080",
        api_key=os.environ["WORKSPACE_API_KEY"],
        timeout=30.0,
        retries=3
    )

    # 创建 Sandbox
    sandbox = await client.sandbox.create(CreateSandboxParams(
        template="python:3.11",
        name="my-sandbox"
    ))

    print(f"Sandbox created: {sandbox.id}")
    print(f"NFS mount: {sandbox.nfs.mount_command}")

    # 用户挂载 NFS 后操作文件...
    # with open("/mnt/workspace/main.py", "w") as f:
    #     f.write('print("Hello")')

    # 执行命令 (同步)
    result = await sandbox.process.run("python /workspace/main.py")
    print(f"Output: {result.stdout}")

    # 清理
    await client.sandbox.delete(sandbox.id)

asyncio.run(main())
```

**流式输出示例:**

```python
import asyncio
from workspace_sdk import WorkspaceClient, CreateSandboxParams

async def main():
    client = WorkspaceClient()
    sandbox = await client.sandbox.create(CreateSandboxParams(template="python:3.11"))

    # 流式执行命令
    async for event in sandbox.process.run_stream("pip install numpy pandas"):
        if event.type == "stdout":
            print(event.data, end="")
        elif event.type == "stderr":
            print(event.data, end="", file=sys.stderr)
        elif event.type == "exit":
            print(f"\nExit code: {event.exit_code}")

    await client.sandbox.delete(sandbox.id)

asyncio.run(main())
```

**同步版本:**

```python
from workspace_sdk.sync import WorkspaceClient, CreateSandboxParams

client = WorkspaceClient()
sandbox = client.sandbox.create(CreateSandboxParams(template="python:3.11"))

# 同步执行
result = sandbox.process.run("echo Hello")
print(result.stdout)

# 流式执行 (同步版)
for event in sandbox.process.run_stream("pip install requests"):
    if event.type == "stdout":
        print(event.data, end="")

client.sandbox.delete(sandbox.id)
```

**错误处理:**

```python
from workspace_sdk.errors import (
    WorkspaceError,           # 基类
    SandboxNotFoundError,
    TemplateNotFoundError,
    SandboxLimitExceededError,
    ProcessTimeoutError,
    CommandFailedError,
    PTYNotFoundError,
    PTYLimitExceededError,
    UnauthorizedError,
    RateLimitedError,
)

try:
    sandbox = await client.sandbox.get("sbx-not-exist")
except SandboxNotFoundError as e:
    print(f"Sandbox not found: {e}")
except WorkspaceError as e:
    print(f"Workspace error [{e.code}]: {e}")
```

---

## 8. 部署方案

### 8.1 单机部署架构

```
┌─────────────────────────────────────────────────────┐
│                    部署节点                          │
├─────────────────────────────────────────────────────┤
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │          workspace-server (容器)              │   │
│  │                                               │   │
│  │  Port 8080:  HTTP API + WebSocket             │   │
│  │  Port 50051: gRPC (Agent 通信)                │   │
│  │  Port 2049:  NFS Server                       │   │
│  │  Port 9090:  Metrics                          │   │
│  │                                               │   │
│  │  Mounts:                                      │   │
│  │    - /var/run/docker.sock                    │   │
│  │    - /data/workspaces                        │   │
│  └──────────────────────────────────────────────┘   │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │         /data/workspaces (本地存储)           │   │
│  │  ├── sbx-abc123/                              │   │
│  │  ├── sbx-def456/                              │   │
│  │  └── ...                                      │   │
│  └──────────────────────────────────────────────┘   │
│                                                      │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐               │
│  │Sandbox 1│ │Sandbox 2│ │Sandbox N│  (动态创建)   │
│  └─────────┘ └─────────┘ └─────────┘               │
│                                                      │
└─────────────────────────────────────────────────────┘
```

### 8.2 Docker Compose

```yaml
version: '3.8'

services:
  workspace-server:
    image: elevo/workspace-server:latest
    container_name: workspace-server
    ports:
      - "8080:8080"    # HTTP API
      - "50051:50051"  # gRPC (Agent 通信)
      - "2049:2049"    # NFS Server
      - "9090:9090"    # Metrics
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - workspace-data:/data/workspaces
    environment:
      - RUST_LOG=info
      - API_PORT=8080
      - GRPC_PORT=50051
      - NFS_PORT=2049
      - METRICS_PORT=9090
      - WORKSPACE_ROOT=/data/workspaces
      - API_KEY=${API_KEY}
      - DOCKER_NETWORK=workspace-net
    networks:
      - workspace-net
    restart: unless-stopped
    # 需要 privileged 或特定 capabilities 来运行 NFS
    cap_add:
      - SYS_ADMIN
    # 或者使用 privileged (不推荐生产环境)
    # privileged: true

volumes:
  workspace-data:
    driver: local

networks:
  workspace-net:
    driver: bridge
```

### 8.3 配置项

| 变量 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| API_PORT | 否 | 8080 | HTTP API 端口 |
| GRPC_PORT | 否 | 50051 | Agent gRPC 通信端口 |
| NFS_MODE | 否 | embedded | NFS 模式: `embedded` (内置) 或 `system` (系统) |
| NFS_PORT | 否 | 2049 | NFS 服务端口 |
| NFS_ALLOWED_IPS | 否 | * | NFS 客户端 IP 白名单，如 `10.0.0.0/8,172.16.0.0/12`，`*` 表示允许所有 |
| METRICS_PORT | 否 | 9090 | 指标端口 |
| API_KEY | 是 | - | API 认证密钥 |
| WORKSPACE_ROOT | 否 | /data/workspaces | Workspace 根目录 |
| DOCKER_HOST | 否 | unix:///var/run/docker.sock | Docker 地址 |
| DOCKER_NETWORK | 否 | bridge | Sandbox 容器网络 |
| DEFAULT_SANDBOX_TIMEOUT | 否 | 3600 | 默认超时 (秒) |
| MAX_SANDBOXES | 否 | 100 | 最大 Sandbox 数量 |
| MAX_CONCURRENT_CREATIONS | 否 | 10 | 最大并发创建 Sandbox 数量 |
| RUST_LOG | 否 | info | 日志级别 |

### 8.4 NFS 模式配置

#### 8.4.1 内置模式 (embedded)

`nfsserve` 使用单端口模式，不需要 portmapper (端口 111):

| 端口 | 用途 |
|------|------|
| 2049 | NFS + Mount Protocol |

**客户端挂载命令:**

```bash
mount -t nfs -o vers=3,tcp,nolock,port=2049,mountport=2049 server:/${SANDBOX_ID} /mnt
```

**Docker Compose 配置:**

```yaml
services:
  workspace-server:
    environment:
      - NFS_MODE=embedded
    cap_add:
      - SYS_ADMIN  # nfsserve 可能需要
```

#### 8.4.2 系统模式 (system)

使用 Linux 内核 NFS 服务，需要额外配置:

| 端口 | 用途 |
|------|------|
| 111 | portmapper (rpcbind) |
| 2049 | NFS |
| 动态 | mountd, statd 等 |

**前置条件:**

```bash
# 安装 NFS 服务
apt-get install nfs-kernel-server  # Debian/Ubuntu
yum install nfs-utils              # CentOS/RHEL

# 启动服务
systemctl enable --now nfs-server
systemctl enable --now rpcbind
```

**客户端挂载命令:**

```bash
# NFSv3
mount -t nfs -o vers=3 server:/${SANDBOX_ID} /mnt

# NFSv4 (如果启用)
mount -t nfs4 server:/${SANDBOX_ID} /mnt
```

**Docker Compose 配置 (系统模式):**

```yaml
services:
  workspace-server:
    environment:
      - NFS_MODE=system
      - NFS_EXPORTS_FILE=/etc/exports.d/workspace.exports
    volumes:
      - /etc/exports.d:/etc/exports.d
    # 系统模式需要 host 网络或额外端口映射
    network_mode: host
    # 需要能够调用 exportfs 命令
    privileged: true
```

**exports 文件管理:**

系统模式下，服务会动态管理 `/etc/exports.d/workspace.exports`:

```bash
# 创建 Sandbox 时添加
/data/workspaces/sbx-abc123  *(rw,sync,no_subtree_check,no_root_squash)

# 删除 Sandbox 时移除
# 然后执行 exportfs -ra 刷新
```

#### 8.4.3 模式对比

| 特性 | 内置模式 (embedded) | 系统模式 (system) |
|------|---------------------|-------------------|
| 部署复杂度 | 低 | 中 |
| 性能 | 中 | 高 |
| 协议支持 | NFSv3 | NFSv3/v4 |
| 跨平台 | 是 | 仅 Linux |
| 容器化部署 | 友好 | 需要特权模式 |
| 生产推荐 | 开发/测试 | 生产环境 |

---

## 9. 安全考虑

### 9.1 容器隔离

| 措施 | 说明 |
|------|------|
| Namespace 隔离 | Docker 默认提供 |
| 资源限制 | CPU/内存限制 |
| 只读 rootfs | 容器根文件系统只读 (可选) |
| 非 root 用户 | Agent 以非 root 运行 |
| Seccomp | 限制系统调用 (可选) |
| AppArmor/SELinux | 强制访问控制 (可选) |

### 9.2 NFS 安全

| 措施 | MVP | 后续 |
|------|-----|------|
| IP 白名单 | ✅ | ✅ |
| 路径隔离 | ✅ 每个 Sandbox 独立目录 | ✅ |
| 路径遍历防护 | ✅ | ✅ |
| Kerberos 认证 | ❌ | 可选 |
| TLS 加密 | ❌ | 可选 (NFSv4) |

**路径遍历防护:**

内置模式 (nfsserve) 中，VFS 实现必须严格验证路径：

```rust
impl LocalVFS {
    /// 验证并规范化路径，防止路径遍历攻击
    fn validate_path(&self, sandbox_id: &str, path: &Path) -> Result<PathBuf, NfsError> {
        // 1. 构建完整路径
        let full_path = self.workspace_root
            .join(sandbox_id)
            .join(path.strip_prefix("/").unwrap_or(path));

        // 2. 规范化路径 (解析 . 和 ..)
        let canonical = full_path.canonicalize()?;

        // 3. 验证路径在 sandbox 目录内
        let sandbox_root = self.workspace_root.join(sandbox_id).canonicalize()?;
        if !canonical.starts_with(&sandbox_root) {
            return Err(NfsError::PathTraversal);
        }

        Ok(canonical)
    }
}
```

系统模式下，使用 `no_subtree_check` 选项但需确保每个 Sandbox 只能访问自己的导出目录。

### 9.3 API 安全

| 措施 | 说明 |
|------|------|
| API Key 认证 | 必须提供有效 API Key |
| HTTPS | 生产环境强制 TLS |
| 速率限制 | 防止滥用 |
| 输入验证 | 严格验证所有输入 |
| 命令注入防护 | 命令参数转义 |

**速率限制配置:**

| 端点 | 限制 |
|------|------|
| POST /sandboxes | 10/分钟 |
| POST /process/run | 100/分钟 |
| 其他 | 1000/分钟 |

### 9.4 审计日志

记录以下操作:
- Sandbox 创建/删除
- 命令执行 (记录命令内容，可配置脱敏)
- PTY 会话创建/关闭
- NFS 挂载事件 (可选)
- API 认证失败
- 异常操作

**日志格式:**

```json
{
  "timestamp": "2024-01-15T10:30:00.123Z",
  "level": "info",
  "event": "sandbox.created",
  "sandbox_id": "sbx-abc123",
  "user": "api_key_hash",
  "template": "python:3.11",
  "request_id": "req-xyz789",
  "duration_ms": 2345
}
```

### 9.5 优雅停机

服务收到 SIGTERM 信号后：

1. **停止接受新请求** - 返回 503 Service Unavailable
2. **等待进行中的操作完成** (最长 30 秒)
   - HTTP 请求
   - 流式命令输出
   - PTY 会话 (发送断开通知)
3. **保存状态** - 更新数据库中 Sandbox 状态
4. **关闭连接** - 断开所有 Agent gRPC 连接
5. **退出**

---

## 10. 风险与待定事项

### 10.1 待明确事项

| 事项 | 说明 | 负责人 | 截止日期 |
|------|------|--------|---------|
| 镜像仓库地址 | 模板镜像存储位置 | - | - |
| 用户认证对接 | 是否需要对接内部 SSO | - | - |
| NFS 客户端部署 | Agent 运行环境是否已安装 NFS 客户端 | - | - |
| 存储空间 | Workspace 存储容量规划 | - | - |

### 10.2 技术风险

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| nfsserve 稳定性 | NFS 服务不可用 | 充分测试，准备 fallback 方案 |
| NFS 性能 | 大文件操作慢 | 使用 SSD，调优参数 |
| 容器启动慢 | 用户体验差 | 镜像预热，精简镜像 |
| Docker 单点 | 所有 Sandbox 不可用 | 后续支持多 Docker host |

### 10.3 nfsserve 局限性

| 局限 | 影响 | 处理方式 |
|------|------|---------|
| 仅支持 NFSv3 | 不支持 NFSv4 特性 | 可接受，v3 功能足够 |
| Windows 支持有限 | Windows 客户端可能有问题 | 优先支持 Linux/Mac |
| 无 ACL 支持 | 细粒度权限控制受限 | 依赖 Unix 权限 |

### 10.4 扩展考虑

MVP 后可考虑的功能:

- [ ] 支持 Kubernetes 作为运行时
- [ ] NFSv4 支持
- [ ] Git 服务集成
- [ ] 快照和恢复
- [ ] 资源指标采集
- [ ] Web IDE 集成
- [ ] 多租户隔离

---

## 11. 开发路线图

### Phase 1: 基础框架 (2 周)

- [ ] Rust 项目初始化 (workspace-server)
- [ ] axum HTTP 框架搭建
- [ ] Docker 集成 (bollard crate)
- [ ] 基本配置管理
- [ ] 日志系统

### Phase 2: gRPC 通信 (1 周)

- [ ] 定义 proto 文件
- [ ] 实现 Server 端 gRPC 服务
- [ ] Agent 连接管理
- [ ] 双向流通信

### Phase 3: NFS 服务 (1.5 周)

- [ ] 集成 nfsserve crate (内置模式)
- [ ] 实现本地文件系统 VFS
- [ ] 路径隔离和安全验证
- [ ] 系统模式支持
- [ ] NFS 导出配置

### Phase 4: Sandbox 管理 (2 周)

- [ ] Sandbox 创建/删除/查询
- [ ] Container 生命周期管理
- [ ] Container Agent 实现
- [ ] 健康检查机制
- [ ] 状态持久化 (SQLite)
- [ ] 超时清理定时任务

### Phase 5: Process 与 PTY (1.5 周)

- [ ] Process.run 实现 (同步)
- [ ] 流式输出 (SSE)
- [ ] PTY 服务实现
- [ ] WebSocket + 心跳
- [ ] 超时处理

### Phase 6: SDK 开发 (2 周)

- [ ] TypeScript SDK (含流式)
- [ ] Python SDK (async + sync)
- [ ] SDK 测试
- [ ] 使用示例

### Phase 7: 部署与文档 (1 周)

- [ ] Docker 镜像构建
- [ ] Docker Compose 配置
- [ ] 模板镜像制作
- [ ] 运维文档
- [ ] 用户文档

**总计: 约 11 周**

---

## 12. 测试策略

### 12.1 测试分层

```
┌─────────────────────────────────────────────────┐
│                 E2E 测试                         │
│         (完整流程：创建→执行→删除)                │
├─────────────────────────────────────────────────┤
│              集成测试                            │
│    (API + Docker + NFS 联合测试)                 │
├─────────────────────────────────────────────────┤
│              单元测试                            │
│   (各模块独立测试，Mock 外部依赖)                 │
└─────────────────────────────────────────────────┘
```

### 12.2 单元测试

| 模块 | 测试内容 |
|------|---------|
| Sandbox Manager | 状态机转换、并发创建 |
| NFS VFS | 路径验证、权限检查、路径遍历防护 |
| gRPC 连接池 | 连接管理、重连、超时 |
| 配置解析 | 参数验证、默认值 |

### 12.3 集成测试

| 场景 | 测试内容 |
|------|---------|
| Sandbox 生命周期 | 创建→查询→删除 |
| 命令执行 | 同步/流式、超时、大输出 |
| PTY | 创建→输入→输出→关闭 |
| NFS | 挂载→读写→卸载 |
| 状态恢复 | 服务重启后状态恢复 |

### 12.4 E2E 测试

```python
# 典型 E2E 测试用例
async def test_full_workflow():
    client = WorkspaceClient(...)

    # 1. 创建 Sandbox
    sandbox = await client.sandbox.create(CreateSandboxParams(
        template="python:3.11"
    ))
    assert sandbox.state == "running"

    # 2. 通过 NFS 写入文件
    mount_and_write(sandbox.nfs, "/app/main.py", "print('hello')")

    # 3. 执行命令
    result = await sandbox.process.run("python /app/main.py")
    assert result.exit_code == 0
    assert "hello" in result.stdout

    # 4. 流式执行
    output = []
    async for event in sandbox.process.run_stream("pip install requests"):
        output.append(event)
    assert any(e.type == "exit" and e.exit_code == 0 for e in output)

    # 5. 清理
    await client.sandbox.delete(sandbox.id)
```

### 12.5 性能测试

| 测试项 | 目标 | 方法 |
|--------|------|------|
| Sandbox 创建延迟 | < 5s | 测量 100 次创建 P99 |
| 并发创建 | 50 并发 | 同时创建 50 个 Sandbox |
| NFS 吞吐 | > 100MB/s | dd 写入/读取测试 |
| 命令执行 QPS | > 100/s | 压测简单命令 |

### 12.6 安全测试

| 测试项 | 方法 |
|--------|------|
| 路径遍历 | 尝试访问 `/../../../etc/passwd` |
| 命令注入 | 测试特殊字符 `; | & $()` |
| 资源耗尽 | Fork 炸弹、内存耗尽 |
| 未授权访问 | 无 API Key、错误 Sandbox ID |

---

## 13. 运维指南

### 13.1 监控告警

**关键告警规则:**

| 告警 | 条件 | 级别 |
|------|------|------|
| 服务不可用 | /health 失败 > 1 分钟 | 严重 |
| Sandbox 创建失败率高 | 失败率 > 10% | 警告 |
| Agent 连接断开 | 断开 > 5 分钟未重连 | 警告 |
| 磁盘空间不足 | 使用率 > 80% | 警告 |
| 内存使用过高 | 使用率 > 90% | 警告 |

**Prometheus 告警规则示例:**

```yaml
groups:
  - name: workspace-server
    rules:
      - alert: WorkspaceServerDown
        expr: up{job="workspace-server"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Workspace Server is down"

      - alert: HighSandboxCreationFailure
        expr: rate(sandbox_creation_failures_total[5m]) / rate(sandbox_creation_total[5m]) > 0.1
        for: 5m
        labels:
          severity: warning
```

### 13.2 日志查看

```bash
# 查看服务日志
docker logs workspace-server -f --tail 100

# 查看特定 Sandbox 日志
curl -H "Authorization: Bearer $API_KEY" \
  "http://localhost:8080/api/v1/sandboxes/sbx-abc123/logs?tail=50"

# 按时间范围查询
curl "http://localhost:8080/api/v1/sandboxes/sbx-abc123/logs?since=2024-01-15T10:00:00Z"
```

### 13.3 故障排查

**常见问题:**

| 问题 | 可能原因 | 排查方法 |
|------|---------|---------|
| Sandbox 创建超时 | Docker 拉取镜像慢 | 检查网络，预热镜像 |
| Agent 连接失败 | 网络不通 | 检查 Docker 网络配置 |
| NFS 挂载失败 | 防火墙/端口 | 检查 2049 端口 |
| 命令执行无响应 | Agent 卡死 | 查看容器日志 |

**诊断命令:**

```bash
# 检查服务状态
curl http://localhost:8080/health
curl http://localhost:8080/ready
curl http://localhost:8080/api/v1/stats

# 检查 Docker 容器
docker ps -a | grep sbx-

# 检查 NFS 导出
showmount -e localhost

# 手动触发 GC
curl -X POST -H "Authorization: Bearer $API_KEY" \
  http://localhost:8080/api/v1/gc
```

### 13.4 容量规划

| 规格 | 预估容量 | 说明 |
|------|---------|------|
| 4C8G | 20 并发 Sandbox | 开发环境 |
| 8C16G | 50 并发 Sandbox | 测试环境 |
| 16C32G | 100 并发 Sandbox | 生产环境 |

**存储估算:**
- 每个 Workspace: 平均 500MB
- 100 个 Sandbox: ~50GB
- 建议预留 2x 空间用于峰值

### 13.5 备份恢复

**需要备份的数据:**

| 数据 | 位置 | 备份方式 |
|------|------|---------|
| SQLite 数据库 | /data/workspace.db | 定期复制 |
| Workspace 目录 | /data/workspaces/ | 可选，按需备份 |
| 配置文件 | /etc/workspace/ | 版本控制 |

**恢复流程:**

1. 停止服务
2. 恢复 SQLite 数据库
3. 启动服务 (自动恢复状态)
4. 清理孤立容器: `docker ps -a | grep sbx- | xargs docker rm -f`

---

## 附录

### A. Container Agent

Agent 是运行在每个容器内的轻量级进程，通过 gRPC 主动连接 Server。

**启动参数 (环境变量):**

| 变量 | 说明 |
|------|------|
| WORKSPACE_SERVER | Server gRPC 地址，如 `server:50051` |
| SANDBOX_ID | Sandbox 标识 |
| AGENT_LOG_LEVEL | 日志级别，默认 `info` |

**Agent 行为:**

1. 启动后立即连接 Server gRPC 端点
2. 发送 Register 消息，包含 sandbox_id
3. 保持 gRPC stream 连接，接收 Server 命令
4. 每 30 秒发送心跳
5. 连接断开后自动重连 (指数退避)

**Agent 二进制规格:**

| 属性 | 值 |
|------|-----|
| 大小 | < 10MB (静态链接) |
| 内存占用 | < 20MB |
| CPU 占用 | 空闲时 < 1% |

### B. 目录结构

```
unified-workspace-sdk/
├── proto/                       # gRPC 协议定义
│   └── workspace/
│       └── agent/
│           └── v1/
│               └── agent.proto
│
├── server/                      # Rust 服务端
│   ├── Cargo.toml
│   ├── build.rs                 # proto 编译
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs           # 配置
│   │   ├── db.rs               # SQLite 状态存储
│   │   ├── api/                # HTTP API
│   │   │   ├── mod.rs
│   │   │   ├── sandbox.rs
│   │   │   ├── process.rs
│   │   │   ├── pty.rs
│   │   │   └── templates.rs
│   │   ├── grpc/               # gRPC 服务
│   │   │   ├── mod.rs
│   │   │   └── agent_service.rs
│   │   ├── docker/             # Docker 集成
│   │   │   ├── mod.rs
│   │   │   └── manager.rs
│   │   ├── nfs/                # NFS 服务 (双模式)
│   │   │   ├── mod.rs
│   │   │   ├── provider.rs     # NFS Provider trait
│   │   │   ├── embedded.rs     # 内置模式 (nfsserve)
│   │   │   └── system.rs       # 系统模式 (nfs-kernel-server)
│   │   ├── scheduler/          # 定时任务
│   │   │   └── mod.rs          # 超时清理等
│   │   └── error.rs
│   └── tests/
│       ├── unit/
│       └── integration/
│
├── agent/                       # Rust 容器内 Agent
│   ├── Cargo.toml
│   ├── build.rs
│   ├── src/
│   │   ├── main.rs
│   │   ├── grpc_client.rs      # gRPC 客户端
│   │   ├── process.rs
│   │   └── pty.rs
│   └── Dockerfile
│
├── sdk-typescript/              # TypeScript SDK
│   ├── package.json
│   ├── src/
│   │   ├── index.ts
│   │   ├── client.ts
│   │   ├── sandbox.ts
│   │   ├── process.ts
│   │   ├── pty.ts
│   │   ├── streaming.ts        # SSE 流式支持
│   │   └── types.ts
│   └── tests/
│
├── sdk-python/                  # Python SDK
│   ├── pyproject.toml
│   ├── src/
│   │   └── workspace_sdk/
│   │       ├── __init__.py
│   │       ├── client.py
│   │       ├── sandbox.py
│   │       ├── process.py
│   │       ├── pty.py
│   │       ├── streaming.py      # 流式支持
│   │       ├── types.py
│   │       ├── errors.py
│   │       └── sync/           # 同步版本
│   └── tests/
│
├── images/                      # 模板镜像
│   ├── base/
│   │   └── Dockerfile
│   ├── python/
│   │   └── Dockerfile
│   └── node/
│       └── Dockerfile
│
├── docker/
│   ├── docker-compose.yml
│   ├── docker-compose.dev.yml  # 开发环境
│   └── Dockerfile.server
│
├── scripts/                     # 运维脚本
│   ├── build-images.sh
│   ├── healthcheck.sh
│   └── backup.sh
│
├── tests/                       # E2E 测试
│   ├── e2e/
│   └── benchmark/
│
├── docs/
│   ├── requirements.md         # 本文档
│   ├── api.md                  # API 文档
│   ├── deployment.md           # 部署文档
│   └── minimal/                # 接口文档
│
└── examples/
    ├── typescript/
    └── python/
```

### C. NFS Provider 接口设计

```rust
use async_trait::async_trait;
use std::path::PathBuf;

/// NFS 提供者统一接口
#[async_trait]
pub trait NfsProvider: Send + Sync {
    /// 启动 NFS 服务
    async fn start(&self) -> Result<(), NfsError>;

    /// 停止 NFS 服务
    async fn stop(&self) -> Result<(), NfsError>;

    /// 添加导出路径 (创建 Sandbox 时调用)
    async fn add_export(&self, sandbox_id: &str, path: &PathBuf) -> Result<NfsExportInfo, NfsError>;

    /// 移除导出路径 (删除 Sandbox 时调用)
    async fn remove_export(&self, sandbox_id: &str) -> Result<(), NfsError>;

    /// 获取 NFS 服务状态
    fn is_running(&self) -> bool;

    /// 获取 NFS 服务地址信息
    fn get_server_info(&self) -> NfsServerInfo;
}

/// NFS 导出信息
pub struct NfsExportInfo {
    pub host: String,
    pub port: u16,
    pub path: String,
    pub mount_command: String,
}

/// NFS 服务器信息
pub struct NfsServerInfo {
    pub mode: NfsMode,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Copy)]
pub enum NfsMode {
    Embedded,  // 使用 nfsserve
    System,    // 使用 nfs-kernel-server
}
```

### D. 内置模式实现示例 (nfsserve)

```rust
use nfsserve::vfs::{NFSFileSystem, VFSCapabilities};
use nfsserve::nfs::{fattr3, fileid3, filename3, nfsstat3};
use async_trait::async_trait;
use std::path::PathBuf;
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct EmbeddedNfsProvider {
    config: NfsConfig,
    vfs: Arc<LocalVFS>,
    server_handle: Option<JoinHandle<()>>,
}

impl EmbeddedNfsProvider {
    pub fn new(config: NfsConfig) -> Self {
        Self {
            config,
            vfs: Arc::new(LocalVFS::new(config.workspace_root.clone())),
            server_handle: None,
        }
    }
}

#[async_trait]
impl NfsProvider for EmbeddedNfsProvider {
    async fn start(&self) -> Result<(), NfsError> {
        let listener = NFSTcpListener::bind(&format!("0.0.0.0:{}", self.config.port)).await?;
        let vfs = self.vfs.clone();
        self.server_handle = Some(tokio::spawn(async move {
            listener.handle_forever(vfs).await;
        }));
        Ok(())
    }

    async fn add_export(&self, sandbox_id: &str, path: &PathBuf) -> Result<NfsExportInfo, NfsError> {
        // nfsserve 直接通过 VFS 访问，无需额外配置
        Ok(NfsExportInfo {
            host: self.config.host.clone(),
            port: self.config.port,
            path: format!("/{}", sandbox_id),
            mount_command: format!(
                "mount -t nfs -o vers=3,tcp,nolock,port={port},mountport={port} {host}:/{id} /mnt",
                port = self.config.port,
                host = self.config.host,
                id = sandbox_id
            ),
        })
    }

    // ... 其他方法实现
}
```

### E. 系统模式实现示例 (nfs-kernel-server)

```rust
use std::process::Command;
use tokio::fs;

pub struct SystemNfsProvider {
    config: NfsConfig,
    exports_file: PathBuf,
}

impl SystemNfsProvider {
    pub fn new(config: NfsConfig) -> Self {
        Self {
            config,
            exports_file: PathBuf::from("/etc/exports.d/workspace.exports"),
        }
    }

    /// 刷新 exports 配置
    async fn refresh_exports(&self) -> Result<(), NfsError> {
        let output = Command::new("exportfs")
            .arg("-ra")
            .output()?;

        if !output.status.success() {
            return Err(NfsError::ExportFailed(
                String::from_utf8_lossy(&output.stderr).to_string()
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl NfsProvider for SystemNfsProvider {
    async fn start(&self) -> Result<(), NfsError> {
        // 检查 nfs-server 服务状态
        let status = Command::new("systemctl")
            .args(["is-active", "nfs-server"])
            .output()?;

        if !status.status.success() {
            return Err(NfsError::ServiceNotRunning);
        }
        Ok(())
    }

    async fn add_export(&self, sandbox_id: &str, path: &PathBuf) -> Result<NfsExportInfo, NfsError> {
        // 读取现有 exports
        let mut exports = fs::read_to_string(&self.exports_file)
            .await
            .unwrap_or_default();

        // 添加新的导出行
        let export_line = format!(
            "{path}  *(rw,sync,no_subtree_check,no_root_squash)\n",
            path = path.display()
        );
        exports.push_str(&export_line);

        // 写回文件
        fs::write(&self.exports_file, exports).await?;

        // 刷新 exports
        self.refresh_exports().await?;

        Ok(NfsExportInfo {
            host: self.config.host.clone(),
            port: 2049,
            path: path.to_string_lossy().to_string(),
            mount_command: format!(
                "mount -t nfs -o vers=3 {host}:{path} /mnt",
                host = self.config.host,
                path = path.display()
            ),
        })
    }

    async fn remove_export(&self, sandbox_id: &str) -> Result<(), NfsError> {
        // 读取 exports 文件，移除对应行
        let exports = fs::read_to_string(&self.exports_file).await?;
        let workspace_path = format!("{}/{}", self.config.workspace_root.display(), sandbox_id);

        let new_exports: String = exports
            .lines()
            .filter(|line| !line.contains(&workspace_path))
            .collect::<Vec<_>>()
            .join("\n");

        fs::write(&self.exports_file, new_exports).await?;
        self.refresh_exports().await?;

        Ok(())
    }

    // ... 其他方法实现
}
```

### F. 参考资料

- nfsserve: https://github.com/xetdata/nfsserve
- bollard (Rust Docker client): https://crates.io/crates/bollard
- axum (Rust Web framework): https://crates.io/crates/axum
- tokio-tungstenite (WebSocket): https://crates.io/crates/tokio-tungstenite
- E2B SDK: https://github.com/e2b-dev/E2B
- Daytona: https://github.com/daytonaio/daytona
