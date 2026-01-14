# 统一 Workspace SDK 技术方案设计

> 版本: 1.1.0
> 基于需求文档: v1.4.0
> 更新日期: 2025-01-14

---

## 1. 系统架构

### 1.1 整体架构

系统采用三层架构：**接入层** → **服务层** → **基础设施层**

```
┌─────────────────────────────────────────────────────────────────────┐
│                           外部使用方                                 │
│                    (Agent 应用 / SDK 调用方)                         │
└───────────────────────────────┬─────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        │                       │                       │
   REST API              NFS 挂载               WebSocket
   (Sandbox/Process)     (文件操作)              (PTY)
        │                       │                       │
┌───────┴───────────────────────┴───────────────────────┴─────────────┐
│                        Workspace Server                              │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │                         接入层                                  │ │
│  │   HTTP Router │ WebSocket Handler │ NFS Server │ gRPC Server   │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                │                                     │
│  ┌────────────────────────────┴───────────────────────────────────┐ │
│  │                         服务层                                  │ │
│  │     SandboxService  │  ProcessService  │  PTYService           │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                │                                     │
│  ┌────────────────────────────┴───────────────────────────────────┐ │
│  │                      基础设施层                                 │ │
│  │  DockerManager │ AgentConnPool │ StateStore │ NfsProvider      │ │
│  └────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │
                           gRPC 双向流
                                  │
┌─────────────────────────────────┴───────────────────────────────────┐
│                        Docker 容器集群                               │
│   ┌─────────────┐   ┌─────────────┐   ┌─────────────┐              │
│   │  Sandbox A  │   │  Sandbox B  │   │  Sandbox C  │              │
│   │   (Agent)   │   │   (Agent)   │   │   (Agent)   │              │
│   │ /workspace ←┼───┼─────────────┼───┼─→ NFS 共享存储             │
│   └─────────────┘   └─────────────┘   └─────────────┘              │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.2 核心设计决策

| 决策点 | 选择 | 原因 |
|--------|------|------|
| 文件访问方式 | NFS 而非 HTTP API | 大文件传输效率高，支持标准文件系统语义 |
| Agent 通信 | gRPC 双向流 | Agent 主动连接避免网络穿透，支持实时推送 |
| 命令输出 | SSE 流式 | 实时输出反馈，类似 CLI 体验 |
| 状态存储 | SQLite | 轻量嵌入式，服务重启可恢复状态 |

---

## 2. 模块划分

### 2.1 Server 模块结构

```
workspace-server/
├── 接入层 (api/)
│   ├── HTTP 路由与 Handler
│   ├── WebSocket 处理
│   ├── 认证中间件
│   └── 请求/响应 DTO
│
├── gRPC 层 (grpc/)
│   └── Agent 通信服务
│
├── 服务层 (service/)
│   ├── SandboxService     # Sandbox 生命周期
│   ├── ProcessService     # 命令执行
│   ├── PTYService         # 交互终端
│   ├── TemplateService    # 模板管理
│   ├── SystemService      # 系统管理 (stats/gc)
│   └── WebhookService     # Webhook 事件通知
│
├── 基础设施层 (infra/)
│   ├── DockerManager      # 容器操作
│   ├── AgentConnPool      # Agent 连接池
│   ├── StateStore         # 状态持久化
│   ├── NfsProvider        # NFS 服务
│   └── WebhookDispatcher  # Webhook 发送器
│
└── 调度器 (scheduler/)
    ├── 超时清理任务
    └── Webhook expiring 预警任务
```

### 2.2 模块职责

| 模块 | 职责 | 依赖 |
|------|------|------|
| **SandboxService** | Sandbox 创建、查询、删除、超时管理、资源统计 | DockerManager, StateStore, NfsProvider, WebhookService |
| **ProcessService** | 命令执行、输出���转、命令终止 | AgentConnPool |
| **PTYService** | PTY 创建、WebSocket 桥接、resize/kill | AgentConnPool |
| **TemplateService** | 模板列表查询、镜像存在性验证 | DockerManager |
| **SystemService** | 系统统计、手动 GC、健康检查 | StateStore, DockerManager, NfsProvider |
| **WebhookService** | 事件通知触发、事件构造 | WebhookDispatcher, StateStore |
| **DockerManager** | 容器 CRUD、日志获取、状态检查、stats 获取 | Docker API (bollard) |
| **AgentConnPool** | Agent 连接注册、命令下发、输出接收、命令路由 | gRPC Stream |
| **StateStore** | Sandbox 元数据持久化、查询 | SQLite |
| **NfsProvider** | NFS 服务启停、导出管理 | nfsserve / 系统 NFS |
| **WebhookDispatcher** | HTTP 请求发送、HMAC 签名、重试机制 | HTTP Client |

### 2.3 Agent 模块结构

```
workspace-agent/
├── gRPC 客户端      # 连接 Server、收发消息
├── ProcessExecutor  # 命令执行、输出采集
├── PTYManager       # PTY 创建、输入输出
└── ResourceMonitor  # 资源使用上报
```

---

## 3. 核心数据流

### 3.1 Sandbox 创建流程

```
┌──────┐       ┌──────────────┐      ┌────────────┐      ┌───────┐
│Client│       │SandboxService│      │DockerManager│      │ Agent │
└──┬───┘       └──────┬───────┘      └─────┬──────┘      └───┬───┘
   │                  │                    │                 │
   │ 1. POST /sandboxes                    │                 │
   │ ─────────────────>                    │                 │
   │                  │                    │                 │
   │                  │ 2. 验证模板存在     │                 │
   │                  │ 3. 生成 sandbox_id │                 │
   │                  │ 4. 写入 DB(starting)                 │
   │                  │                    │                 │
   │                  │ 5. 创建 workspace 目录                │
   │                  │ ───────────────────>                 │
   │                  │                    │                 │
   │                  │ 6. 创建容器        │                 │
   │                  │ ───────────────────>                 │
   │                  │                    │                 │
   │                  │ 7. 启动容器        │                 │
   │                  │ ───────────────────>                 │
   │                  │                    │ 8. Agent 启动   │
   │                  │                    │ ───────────────>│
   │                  │                    │                 │
   │                  │<────────────────────────────────────│
   │                  │ 9. gRPC Connect + Register           │
   │                  │                    │                 │
   │                  │ 10. 更新 DB(running)                 │
   │                  │                    │                 │
   │<─────────────────│                    │                 │
   │ 11. 返回 Sandbox │                    │                 │
   │    (含 NFS 信息) │                    │                 │
```

**关键点：**
- Agent 启动后**主动连接** Server，解决容器网络穿透问题
- 创建过程分两阶段：容器启动 → Agent 就绪
- DB 状态变更：starting → running

### 3.2 命令执行流程 (流式)

```
┌──────┐       ┌──────────────┐     ┌─────────────┐      ┌───────┐
│Client│       │ProcessService│     │AgentConnPool│      │ Agent │
└──┬───┘       └──────┬───────┘     └──────┬──────┘      └───┬───┘
   │                  │                    │                 │
   │ 1. POST /process/run (stream=true)    │                 │
   │ ─────────────────>                    │                 │
   │                  │                    │                 │
   │                  │ 2. 获取 Agent 连接 │                 │
   │                  │ ───────────────────>                 │
   │                  │                    │                 │
   │                  │ 3. 发送 RunCommandRequest            │
   │                  │ ─────────────────────────────────────>
   │                  │                    │                 │
   │                  │                    │   4. 执行命令   │
   │                  │                    │   fork + exec   │
   │                  │                    │                 │
   │<─────────────────────────────────────────────────────────
   │                  │ 5. CommandOutput(stdout) 实时返回    │
   │  SSE: stdout     │                    │                 │
   │                  │                    │                 │
   │<─────────────────────────────────────────────────────────
   │                  │ 6. CommandOutput(stderr)             │
   │  SSE: stderr     │                    │                 │
   │                  │                    │                 │
   │<─────────────────────────────────────────────────────────
   │                  │ 7. CommandOutput(exit, code=0)       │
   │  SSE: exit       │                    │                 │
```

**关键点：**
- 输出通过 gRPC 双向流实时传递
- Server 将 gRPC 流转换为 SSE 推送给客户端
- 支持命令超时自动终止

### 3.3 PTY 交互流程

```
┌──────┐       ┌──────────────┐     ┌─────────────┐      ┌───────┐
│Client│       │  PTYService  │     │AgentConnPool│      │ Agent │
└──┬───┘       └──────┬───────┘     └──────┬──────┘      └───┬───┘
   │                  │                    │                 │
   │ 1. POST /pty (创建)                   │                 │
   │ ─────────────────>                    │                 │
   │                  │ 2. CreatePTYRequest│                 │
   │                  │ ─────────────────────────────────────>
   │                  │                    │  3. 创建 PTY    │
   │<─────────────────│                    │                 │
   │ 4. 返回 ws_url   │                    │                 │
   │                  │                    │                 │
   │ 5. WebSocket 连接 │                    │                 │
   │ ═════════════════>                    │                 │
   │                  │                    │                 │
   │ 6. WS: input     │ 7. PTYInput        │                 │
   │ ─────────────────│────────────────────│─────────────────>
   │                  │                    │                 │
   │<─────────────────│────────────────────│─────────────────│
   │ 8. WS: output    │ 9. PTYOutput       │                 │
   │                  │                    │                 │
   │ 10. WS: resize   │ 11. ResizePTYRequest                 │
   │ ─────────────────│────────────────────│─────────────────>
```

**关键点：**
- 创建 PTY 返回 WebSocket URL
- WebSocket 作为客户端与 Server 的桥接
- Server 与 Agent 通过 gRPC 流传递 PTY 数据

### 3.4 文件访问流程 (NFS)

```
┌──────┐              ┌───────────┐           ┌──────────────┐
│Client│              │NFS Server │           │  Filesystem  │
└──┬───┘              └─────┬─────┘           └──────┬───────┘
   │                        │                        │
   │ 1. mount -t nfs server:/sbx-123 /mnt           │
   │ ───────────────────────>                        │
   │                        │                        │
   │ 2. 读写文件 /mnt/xxx   │                        │
   │ ───────────────────────>                        │
   │                        │ 3. 读写 /data/workspaces/sbx-123/xxx
   │                        │ ───────────────────────>
   │                        │                        │
   │<───────────────────────│<───────────────────────│
   │ 4. 数据返回            │                        │
```

**关键点：**
- 客户端使用标准 NFS 挂载
- 每个 Sandbox 隔离到独立目录
- 容器内通过 bind mount 访问同一目录

### 3.5 命令终止流程 (Process.kill)

```
┌──────┐       ┌──────────────┐     ┌─────────────┐      ┌───────┐
│Client│       │ProcessService│     │AgentConnPool│      │ Agent │
└──┬───┘       └──────┬───────┘     └──────┬──────┘      └───┬───┘
   │                  │                    │                 │
   │ 1. POST /process/{cmdId}/kill         │                 │
   │ ─────────────────>                    │                 │
   │    {signal: 15}  │                    │                 │
   │                  │                    │                 │
   │                  │ 2. 验证 commandId 存在               │
   │                  │ 3. 获取 Agent 连接 │                 │
   │                  │ ───────────────────>                 │
   │                  │                    │                 │
   │                  │ 4. 发送 KillCommandRequest           │
   │                  │    {command_id, signal}              │
   │                  │ ─────────────────────────────────────>
   │                  │                    │                 │
   │                  │                    │ 5. kill(pid, signal)
   │                  │                    │                 │
   │<─────────────────│                    │                 │
   │ 6. 200 OK        │                    │                 │
   │                  │                    │                 │
   │                  │<─────────────────────────────────────│
   │                  │ 7. CommandOutput(exit, code=-15)     │
   │  SSE: exit       │                    │                 │
   │<─────────────────│                    │                 │
```

**关键点：**
- 支持 SIGTERM (15) 和 SIGKILL (9) 信号
- kill 操作是异步的，立即返回 200
- 命令实际退出后通过 SSE 流返回 exit 事件

### 3.6 资源统计流程 (Sandbox.stats)

```
┌──────┐       ┌──────────────┐     ┌─────────────┐      ┌───────┐
│Client│       │SandboxService│     │DockerManager│      │ Agent │
└──┬───┘       └──────┬───────┘     └──────┬──────┘      └───┬───┘
   │                  │                    │                 │
   │ 1. GET /sandboxes/{id}/stats          │                 │
   │ ─────────────────>                    │                 │
   │                  │                    │                 │
   │                  │ 2. 获取容器 stats  │                 │
   │                  │ ───────────────────>                 │
   │                  │                    │ 3. Docker API   │
   │                  │                    │    stats        │
   │                  │<───────────────────│                 │
   │                  │ 4. CPU/Memory/Network                │
   │                  │                    │                 │
   │                  │ 5. 获取最近心跳数据 │                 │
   │                  │   (含 Agent 上报的资源)              │
   │                  │                    │                 │
   │                  │ 6. 计算 workspace 目录大小            │
   │                  │                    │                 │
   │<─────────────────│                    │                 │
   │ 7. SandboxStats  │                    │                 │
   │   {cpu, memory,  │                    │                 │
   │    disk, network}│                    │                 │
```

**关键点：**
- CPU/Memory/Network 数据来自 Docker stats API
- Disk 使用量通过计算 workspace 目录大小获取
- 可结合 Agent 心跳中的 ResourceUsage 数据补充

### 3.7 Webhook 事件通知流程

```
┌──────────────┐     ┌──────────────┐    ┌───────────────────┐    ┌──────────┐
│SandboxService│     │WebhookService│    │WebhookDispatcher  │    │ 外部接收 │
└──────┬───────┘     └──────┬───────┘    └─────────┬─────────┘    └────┬─────┘
       │                    │                      │                   │
       │ 1. 状态变更事件    │                      │                   │
       │   (sandbox.created)│                      │                   │
       │ ───────────────────>                      │                   │
       │                    │                      │                   │
       │                    │ 2. 构造 Webhook 请求 │                   │
       │                    │   {event, timestamp, │                   │
       │                    │    data}             │                   │
       │                    │ ─────────────────────>                   │
       │                    │                      │                   │
       │                    │                      │ 3. HMAC-SHA256 签名
       │                    │                      │                   │
       │                    │                      │ 4. HTTP POST      │
       │                    │                      │ ─────────────────>│
       │                    │                      │                   │
       │                    │                      │<──────────────────│
       │                    │                      │ 5. 200 OK         │
       │                    │                      │   (或重试)        │
```

**事件类型：**

| 事件 | 触发时机 |
|------|---------|
| `sandbox.created` | Sandbox 创建完成 |
| `sandbox.starting` | 状态变为 starting |
| `sandbox.running` | Agent 连接就绪 |
| `sandbox.expiring` | 过期前 5 分钟 |
| `sandbox.expired` | 已过期，即将清理 |
| `sandbox.deleted` | Sandbox 已删除 |
| `sandbox.error` | Sandbox 异常 |
| `agent.connected` | Agent 连接建立 |
| `agent.disconnected` | Agent 连接断开 |

**重试机制：**
- 重试次数: 3 次
- 重试间隔: 指数退避 (1s, 2s, 4s)
- 超时时间: 5s

### 3.8 批量删除流程 (batchDelete)

```
┌──────┐       ┌──────────────┐     ┌────────────┐      ┌───────┐
│Client│       │SandboxService│     │DockerManager│     │ Agent │
└──┬───┘       └──────┬───────┘     └─────┬──────┘     └───┬───┘
   │                  │                   │                │
   │ 1. POST /sandboxes/batch-delete      │                │
   │    {ids: ["sbx-1", "sbx-2", ...]}    │                │
   │ ─────────────────>                   │                │
   │                  │                   │                │
   │                  │ 2. 并发处理每个 Sandbox            │
   │                  │ ┌─────────────────────────────────┐│
   │                  │ │  for each sandbox_id:           ││
   │                  │ │    - 断开 Agent 连接            ││
   │                  │ │    - 删除容器                   ││
   │                  │ │    - 移除 NFS 导出              ││
   │                  │ │    - 删除 workspace 目录        ││
   │                  │ │    - 更新 DB                    ││
   │                  │ │    - 发送 webhook               ││
   │                  │ └─────────────────────────────────┘│
   │                  │                   │                │
   │<─────────────────│                   │                │
   │ 3. BatchDeleteResponse               │                │
   │    {succeeded: [...],                │                │
   │     failed: [{id, error}]}           │                │
```

**关键点：**
- 最多支持 50 个 ID 批量删除
- 并发执行删除操作，提高效率
- 返回成功和失败的详细结果

---

## 4. 状态管理

### 4.1 Sandbox 状态机

```
                    ┌──────────┐
                    │ (创建)   │
                    └────┬─────┘
                         │
                         ▼
                   ┌──────────┐
            ┌──────│ starting │──────┐
            │      └────┬─────┘      │
            │           │            │
        创建失败    Agent 连接    超时未连接
            │           │            │
            ▼           ▼            ▼
       ┌────────┐  ┌─────────┐  ┌────────┐
       │ error  │  │ running │  │ error  │
       └────────┘  └────┬────┘  └────────┘
                        │
             ┌──────────┼──────────┐
             │          │          │
         用户删除   超时过期    Agent断开
             │          │          │
             ▼          ▼          ▼
       ┌──────────┐  ┌────────┐  ┌────────┐
       │ stopping │  │ stopped│  │ error  │
       └────┬─────┘  └────────┘  └────────┘
            │
       ┌────┴────┐
       │         │
    删除成功  删除失败
       │         │
       ▼         ▼
  ┌──────────┐ ┌────────┐
  │ (已删除) │ │ error  │
  └──────────┘ └────────┘
```

**状态转换触发条件：**

| 当前状态 | 目标状态 | 触发条件 |
|---------|---------|---------|
| - | starting | 调用 create API |
| starting | running | Agent 注册成功 |
| starting | error | 创建失败 / Agent 连接超时 |
| running | stopping | 调用 delete API |
| running | stopped | 超时自动清理 |
| running | error | Agent 连接断开 |
| stopping | (已删除) | 删除操作完成 |
| stopping | error | 删除操作失败 |

### 4.2 数据库 Schema

```sql
-- sandboxes 表
CREATE TABLE sandboxes (
    id              TEXT PRIMARY KEY,      -- sbx-{uuid}
    name            TEXT,                  -- 可选名称
    state           TEXT NOT NULL,         -- starting/running/stopping/stopped/error
    template        TEXT NOT NULL,         -- 模板名
    container_id    TEXT,                  -- Docker 容器 ID
    workspace_path  TEXT NOT NULL,         -- 工作目录路径
    created_at      INTEGER NOT NULL,      -- 创建时间戳 (ms)
    expires_at      INTEGER NOT NULL,      -- 过期时间戳 (ms)
    updated_at      INTEGER NOT NULL,      -- 更新时间戳 (ms)
    config_json     TEXT                   -- 资源配置、环境变量 (JSON)
);

-- 索引
CREATE INDEX idx_state ON sandboxes(state);
CREATE INDEX idx_expires_at ON sandboxes(expires_at);
```

**config_json 字段结构：**

```json
{
  "resources": {
    "cpu": "2",
    "memory": "2g",
    "disk": "10g"
  },
  "envs": {
    "KEY1": "value1",
    "KEY2": "value2"
  },
  "timeout_seconds": 3600
}
```

**updated_at 更新时机：**
- 状态变更时
- extend 延长超时时
- 任何配置变更时

### 4.3 服务重启恢复

```
服务启动
    │
    ▼
读取 DB 中所有 Sandbox
    │
    ▼
遍历每个 Sandbox
    │
    ├─ 检查 Docker 容器状态
    │     │
    │     ├─ 容器运行中 → 等待 Agent 重新连接 (30s 超时)
    │     │                    │
    │     │                    ├─ 连接成功 → 恢复为 running
    │     │                    └─ 连接超时 → 标记为 error
    │     │
    │     ├─ 容器已停止 → 标记为 stopped
    │     │
    │     └─ 容器不存在 → 标记为 error
    │
    ▼
启动定时清理任务
    │
    ▼
启动 API/gRPC/NFS 服务
```

---

## 5. 接口设计

### 5.1 REST API 完整列表

#### Sandbox 服务

| 方法 | 端点 | 功能 | 说明 |
|------|------|------|------|
| POST | `/api/v1/sandboxes` | 创建 Sandbox | 返回 Sandbox 对象 (含 NFS 信息) |
| GET | `/api/v1/sandboxes` | 列表查询 | 支持分页、状态过滤 |
| GET | `/api/v1/sandboxes/{id}` | 获取详情 | 返回单个 Sandbox |
| DELETE | `/api/v1/sandboxes/{id}` | 删除 Sandbox | 可选保留 workspace |
| POST | `/api/v1/sandboxes/batch-delete` | 批量删除 | 最多 50 个 |
| POST | `/api/v1/sandboxes/{id}/extend` | 延长超时 | 最多延长 3600s |
| GET | `/api/v1/sandboxes/{id}/logs` | 容器日志 | 支持 tail/since/until |
| GET | `/api/v1/sandboxes/{id}/stats` | 资源统计 | CPU/Memory/Disk/Network |

#### Process 服务

| 方法 | 端点 | 功能 | 说明 |
|------|------|------|------|
| POST | `/api/v1/sandboxes/{id}/process/run` | 执行命令 | stream=true 返回 SSE |
| POST | `/api/v1/sandboxes/{id}/process/{cmdId}/kill` | 终止命令 | 支持 SIGTERM/SIGKILL |

#### PTY 服务

| 方法 | 端点 | 功能 | 说明 |
|------|------|------|------|
| POST | `/api/v1/sandboxes/{id}/pty` | 创建 PTY | 返回 WebSocket URL |
| POST | `/api/v1/sandboxes/{id}/pty/{ptyId}/resize` | 调整大小 | cols, rows |
| DELETE | `/api/v1/sandboxes/{id}/pty/{ptyId}` | 关闭 PTY | - |
| WS | `/api/v1/sandboxes/{id}/pty/{ptyId}/ws` | PTY WebSocket | 双向通信 |

#### 模板服务

| 方法 | 端点 | 功能 | 说明 |
|------|------|------|------|
| GET | `/api/v1/templates` | 列出模板 | 返回可用模板列表 |

#### 系统管理

| 方法 | 端点 | 功能 | 说明 |
|------|------|------|------|
| GET | `/health` | 健康检查 | 存活探针 |
| GET | `/ready` | 就绪检查 | 就绪探针 |
| GET | `/metrics` | Prometheus 指标 | 监控指标 |
| GET | `/api/v1/stats` | 系统统计 | Sandbox 数量、资源使用 |
| POST | `/api/v1/gc` | 手动 GC | 清理过期资源 |

### 5.2 gRPC 接口概览

**AgentService.Connect** - 双向流

Agent → Server 消息：
- `RegisterRequest` - 注册
- `Heartbeat` - 心跳 + 资源上报
- `CommandOutput` - 命令输出
- `PTYOutput` - PTY 输出

Server → Agent 消息：
- `RegisterResponse` - 注册响应
- `RunCommandRequest` - 执行命令
- `KillCommandRequest` - 终止命令
- `CreatePTYRequest` - 创建 PTY
- `PTYInput` - PTY 输入
- `ResizePTYRequest` - 调整大小
- `KillPTYRequest` - 关闭 PTY

### 5.3 NFS 导出

| 模式 | 导出路径格式 | 挂载命令 |
|------|-------------|---------|
| embedded | `/{sandbox_id}` | `mount -t nfs -o vers=3,nolock server:/{id} /mnt` |
| system | `/data/workspaces/{sandbox_id}` | `mount -t nfs server:/data/workspaces/{id} /mnt` |

---

## 6. 关键设计细节

### 6.1 Agent 连接管理

**连接池结构：**
```
AgentConnPool
├── connections: Map<SandboxId, AgentConnection>
├── pending_commands: Map<CommandId, CommandContext>
└── pending_ptys: Map<PtyId, PtyContext>

AgentConnection
├── sandbox_id: String
├── grpc_stream: BiDirectionalStream
├── last_heartbeat: Timestamp
├── resource_usage: ResourceUsage (最近上报)
└── state: Connected | Disconnected

CommandContext
├── command_id: String
├── sandbox_id: String
├── http_response_tx: mpsc::Sender<StreamEvent>  # SSE 输出通道
├── created_at: Timestamp
└── timeout_ms: u32

PtyContext
├── pty_id: String
├── sandbox_id: String
├── websocket_tx: mpsc::Sender<PtyOutput>  # WebSocket 输出通道
└── created_at: Timestamp
```

**命令输出路由机制：**

```
                              AgentConnPool
                    ┌─────────────────────────────────┐
                    │                                 │
   Agent gRPC ────> │  1. 接收 CommandOutput          │
   (sandbox_id,     │     {command_id, type, data}   │
    command_id)     │                                 │
                    │  2. 查找 pending_commands       │
                    │     [command_id]               │
                    │                                 │
                    │  3. 获取 http_response_tx       │
                    │                                 │
                    │  4. tx.send(StreamEvent)       │
                    │                                 │
                    └──────────────┬──────────────────┘
                                   │
                                   ▼
                    ┌─────────────────────────────────┐
                    │     HTTP SSE Response           │
                    │     (Client 接收)               │
                    └─────────────────────────────────┘
```

**连接生命周期：**
1. Agent 启动后读取 `WORKSPACE_SERVER` 环境变量
2. Agent 发起 gRPC Connect，发送 RegisterRequest
3. Server 验证 sandbox_id，加入连接池
4. Agent 每 30s 发送心跳 (含 ResourceUsage)
5. Server 90s 未收到心跳则标记连接断开
6. 连接断开时，Server 标记 Sandbox 为 error，发送 Webhook

**连接断开处理：**
```
检测到 Agent 断开
    │
    ▼
清理该 Sandbox 的所有 pending_commands
    │ (向对应的 HTTP 连接发送 error 事件)
    ▼
清理该 Sandbox 的所有 pending_ptys
    │ (向对应的 WebSocket 发送关闭消息)
    ▼
更新 Sandbox 状态为 error
    │
    ▼
发送 agent.disconnected Webhook
```

### 6.2 命令输出路由

```
Agent                   Server                      Client
  │                        │                           │
  │ CommandOutput          │                           │
  │ {command_id, stdout}   │                           │
  │───────────────────────>│                           │
  │                        │                           │
  │                        │ 根据 command_id 查找       │
  │                        │ 对应的 HTTP 连接           │
  │                        │                           │
  │                        │ SSE push                  │
  │                        │──────────────────────────>│
```

### 6.3 超时清理机制

```
定时任务 (每 60s)
    │
    ▼
查询 expires_at < now() 的 Sandbox
    │
    ▼
对每个过期 Sandbox:
    ├─ 1. 发送 Webhook 通知 (sandbox.expired)
    ├─ 2. 断开 Agent 连接
    ├─ 3. 停止并删除容器
    ├─ 4. 移除 NFS 导出
    ├─ 5. 删除 workspace 目录
    └─ 6. 更新 DB 状态为 stopped
```

### 6.4 NFS 路径安全

**路径遍历防护：**
1. 客户端请求路径如 `/../../../etc/passwd`
2. NFS VFS 拼接完整路径: `/data/workspaces/sbx-123/../../../etc/passwd`
3. 调用 `canonicalize()` 解析为: `/etc/passwd`
4. 验证结果是否以 `/data/workspaces/sbx-123/` 开头
5. 不匹配则拒绝访问，返回权限错误

---

## 7. 部署架构

### 7.1 单机部署

```
┌─────────────────────────────────────────────────────────────┐
│                        部署节点                              │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              workspace-server 容器                   │   │
│  │                                                      │   │
│  │   :8080   HTTP API + WebSocket                      │   │
│  │   :50051  gRPC (Agent 通信)                         │   │
│  │   :2049   NFS Server                                │   │
│  │   :9090   Metrics                                   │   │
│  │                                                      │   │
│  │   Volumes:                                          │   │
│  │     - /var/run/docker.sock (Docker 控制)            │   │
│  │     - /data/workspaces (Sandbox 文件)               │   │
│  │     - /data/workspace.db (SQLite)                   │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌───────────┐ ┌───────────┐ ┌───────────┐                │
│  │ Sandbox 1 │ │ Sandbox 2 │ │ Sandbox N │  (动态创建)    │
│  └───────────┘ └───────────┘ └───────────┘                │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 7.2 网络配置

#### 7.2.1 端口说明

| 端口 | 协议 | 用途 | 可配置 |
|------|------|------|--------|
| 8080 | HTTP/WS | REST API + WebSocket | 是 (API_PORT) |
| 50051 | gRPC | Agent 通信 | 是 (GRPC_PORT) |
| 2049 | NFS | 文件访问 | 是 (NFS_PORT) |
| 9090 | HTTP | Prometheus 指标 | 是 (METRICS_PORT) |

#### 7.2.2 Agent 访问 Server 配置

根据部署方式不同，Agent 需要不同的 Server 地址配置：

| 部署方式 | WORKSPACE_SERVER 环境变量 | 说明 |
|---------|--------------------------|------|
| Server 宿主机部署 (macOS/Windows) | `host.docker.internal:50051` | Docker Desktop 内置 DNS |
| Server 宿主机部署 (Linux) | `172.17.0.1:50051` | Docker 网桥网关 IP |
| Server 容器化 (同一网络) | `workspace-server:50051` | Docker 服务名解析 |
| Server 容器化 (host 网络) | `localhost:50051` | 直接访问宿主机 |

**配置注入流程：**

```
SandboxService.create()
    │
    ▼
根据配置确定 WORKSPACE_SERVER 值
    │
    ├─ 配置项 AGENT_SERVER_HOST 存在 → 使用配置值
    │
    └─ 自动检测
          ├─ Linux + 宿主机部署 → 172.17.0.1:${GRPC_PORT}
          ├─ 容器化 + 同网络    → workspace-server:${GRPC_PORT}
          └─ 其他              → host.docker.internal:${GRPC_PORT}
    │
    ▼
创建容器时注入环境变量:
  WORKSPACE_SERVER=${computed_value}
  SANDBOX_ID=${sandbox_id}
```

#### 7.2.3 防火墙配置

**入站规则：**

| 端口 | 来源 | 说明 |
|------|------|------|
| 8080 | SDK 客户端 | API 访问 |
| 2049 | NFS 客户端 | 文件操作 |
| 50051 | Docker 容器网络 | Agent 通信 |
| 9090 | 监控系统 | 指标采集 |

**出站规则：**

| 目标 | 说明 |
|------|------|
| Docker API | 容器管理 |
| Webhook URL | 事件通知 |
| 镜像仓库 | 拉取模板镜像 |

---

## 8. SDK 设计

### 8.1 SDK 结构

```
WorkspaceClient
├── config (apiUrl, apiKey, timeout, retries)
│
├── sandbox: SandboxService
│   ├── create(params) → Sandbox
│   ├── get(id) → Sandbox
│   ├── list(params) → ListResponse
│   ├── delete(id, options)
│   ├── batchDelete(ids, options) → BatchDeleteResponse
│   ├── extend(id, seconds) → Sandbox
│   ├── logs(id, options) → LogsResponse
│   └── stats(id) → SandboxStats
│
├── templates: TemplateService
│   └── list() → Template[]
│
└── system: SystemService
    ├── stats() → SystemStats
    └── gc() → GCResult

Sandbox (返回对象)
├── id, name, state, nfs, ...
│
├── process: ProcessService
│   ├── run(cmd, options) → CommandResult
│   ├── runStream(cmd, options) → AsyncIterator<StreamEvent>
│   └── kill(commandId, signal)
│
└── pty: PTYService
    ├── create(options) → PTYHandle
    ├── resize(ptyId, cols, rows)
    └── kill(ptyId)
```

### 8.2 错误类型映射

SDK 将服务端错误码映射为类型化异常：

| 错误码范围 | 错误名称 | TypeScript 类 | Python 类 |
|-----------|---------|---------------|-----------|
| 1001 | UNAUTHORIZED | `UnauthorizedError` | `UnauthorizedError` |
| 1002 | FORBIDDEN | `ForbiddenError` | `ForbiddenError` |
| 2001 | SANDBOX_NOT_FOUND | `SandboxNotFoundError` | `SandboxNotFoundError` |
| 2002 | TEMPLATE_NOT_FOUND | `TemplateNotFoundError` | `TemplateNotFoundError` |
| 2003 | SANDBOX_LIMIT_EXCEEDED | `SandboxLimitExceededError` | `SandboxLimitExceededError` |
| 2004 | SANDBOX_NOT_RUNNING | `SandboxNotRunningError` | `SandboxNotRunningError` |
| 4001 | PROCESS_TIMEOUT | `ProcessTimeoutError` | `ProcessTimeoutError` |
| 4002 | COMMAND_FAILED | `CommandFailedError` | `CommandFailedError` |
| 4003 | COMMAND_NOT_FOUND | `CommandNotFoundError` | `CommandNotFoundError` |
| 4101 | PTY_NOT_FOUND | `PTYNotFoundError` | `PTYNotFoundError` |
| 4102 | PTY_LIMIT_EXCEEDED | `PTYLimitExceededError` | `PTYLimitExceededError` |
| 9001 | INTERNAL_ERROR | `InternalError` | `InternalError` |
| 9002 | RATE_LIMITED | `RateLimitedError` | `RateLimitedError` |

**错误继承结构：**

```
WorkspaceError (基类)
├── code: number
├── name: string
├── message: string
└── details?: object

├── AuthError
│   ├── UnauthorizedError
│   └── ForbiddenError
│
├── SandboxError
│   ├── SandboxNotFoundError
│   ├── TemplateNotFoundError
│   ├── SandboxLimitExceededError
│   └── SandboxNotRunningError
│
├── ProcessError
│   ├── ProcessTimeoutError
│   ├── CommandFailedError
│   └── CommandNotFoundError
│
├── PTYError
│   ├── PTYNotFoundError
│   └── PTYLimitExceededError
│
└── SystemError
    ├── InternalError
    └── RateLimitedError
```

### 8.3 使用模式

**基础使用：**
```
client = WorkspaceClient(url, key)
sandbox = client.sandbox.create({template: "python:3.11"})
result = sandbox.process.run("python --version")
client.sandbox.delete(sandbox.id)
```

**流式输出：**
```
for event in sandbox.process.runStream("pip install numpy"):
    if event.type == "stdout":
        print(event.data)
    elif event.type == "exit":
        print("Exit code:", event.exitCode)
```

**资源自动清理 (TypeScript)：**
```
await using sandbox = await client.sandbox.create({...})
// 作用域结束自动删除
```

---

## 9. 实现计划

### Phase 1: 基础框架 (2周)
- Rust 项目结构 (workspace-server, workspace-agent)
- 配置管理 (环境变量、配置文件)
- HTTP 框架 (axum) + 认证中间件
- SQLite 状态存储
- 日志系统 (tracing)

### Phase 2: Docker 集成 (1周)
- 容器 CRUD 操作 (bollard)
- Workspace 目录管理
- 容器 stats 获取
- 容器日志获取

### Phase 3: gRPC 通信 (1.5周)
- Proto 定义 (agent.proto)
- Server 端 gRPC 服务
- Agent 连接池实现
- 命令路由机制
- 基础 Agent 实现 (gRPC 客户端、心跳)

### Phase 4: 核心功能 (2.5周)
- Sandbox 完整生命周期 (create/get/list/delete)
- Sandbox 扩展功能 (extend/logs/stats)
- **批量删除 (batchDelete)**
- Process 执行 (同步+流式)
- **Process.kill 命令终止**
- 超时清理定时任务
- **Webhook 事件通知机制**

### Phase 5: PTY 服务 (1周)
- Agent 端 PTY 实现
- WebSocket 桥接
- PTY resize/kill

### Phase 6: NFS 服务 (1周)
- 内置模式 (nfsserve)
- 系统模式支持
- 路径安全验证
- 导出管理

### Phase 7: SDK 开发 (2周)
- TypeScript SDK
  - 核心客户端
  - 流式支持 (SSE)
  - 错误类型映射
  - 资源自动清理 (using)
- Python SDK
  - 异步版本 (async)
  - 同步版本 (sync)
  - 流式支持
  - 错误类型映射
- 测试和示例

### Phase 8: 系统管理与部署 (1周)
- 系统统计 API (/api/v1/stats)
- 手动 GC API (/api/v1/gc)
- 模板列表 API
- Docker 镜像构建
- Docker Compose 配置
- 模板镜像制作

### Phase 9: 文档与测试 (0.5周)
- API 文档
- 部署文档
- 运维文档
- E2E 测试用例

**总计：约 12.5 周**

### 关键里程碑

| 里程碑 | 完成标志 | 预计时间 |
|--------|---------|---------|
| M1: 基础可用 | Sandbox 创建、命令执行、NFS 访问 | Phase 1-4 (7周) |
| M2: 功能完整 | PTY、SDK、Webhook | Phase 5-7 (4周) |
| M3: 生产就绪 | 系统管理、文档、测试 | Phase 8-9 (1.5周) |

---

## 10. 风险与缓解

### 10.1 技术风险

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| nfsserve 稳定性未经验证 | NFS 不可用 | 中 | 准备系统模式 fallback，充分测试 |
| Agent 连接不稳定 | 命令执行失败 | 中 | 自动重连 + 状态恢复，连接超时检测 |
| 容器启动慢 | 用户体验差 | 低 | 镜像预热，精简基础镜像 |
| NFS 性能不足 | 大文件操作慢 | 低 | 使用 SSD，或切换系统模式 |
| Webhook 接收方不可用 | 通知丢失 | 中 | 重试机制 + 指数退避，可配置重试次数 |
| 命令输出过大 | 内存溢出 | 低 | maxOutputSize 限制 + 截断 |
| gRPC 连接数过多 | 资源耗尽 | 低 | 连接池大小限制，空闲超时回收 |
| SQLite 并发写入 | 性能瓶颈 | 低 | WAL 模式，批量写入优化 |

### 10.2 运维风险

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| Docker 单点故障 | 所有 Sandbox 不可用 | 监控告警，后续支持多 Docker host |
| 磁盘空间耗尽 | 服务不可用 | 磁盘监控告警，自动 GC |
| 孤立容器/目录 | 资源泄露 | 定期 GC，启动时清理 |
| 配置错误 | 服务启动失败 | 配置验证，友好错误提示 |

### 10.3 安全风险

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| NFS 路径遍历 | 数据泄露 | 路径规范化验证，沙箱隔离 |
| 命令注入 | 容器逃逸 | 参数转义，权限限制 |
| API Key 泄露 | 未授权访问 | HTTPS，Key 轮换机制 |
| 容器逃逸 | 宿主机入侵 | 非 root 运行，Seccomp/AppArmor |

### 10.4 依赖风险

| 依赖 | 风险 | 缓解措施 |
|------|------|---------|
| nfsserve crate | 维护不活跃 | 准备系统模式替代方案 |
| bollard crate | API 变更 | 锁定版本，关注更新 |
| Docker API | 版本兼容性 | 支持多版本 API |
