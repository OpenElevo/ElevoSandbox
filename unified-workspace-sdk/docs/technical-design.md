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

---

## 11. 测试设计

### 11.1 测试架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                          E2E 测试                                    │
│              (完整业务流程，真实环境，SDK 调用)                        │
│                                                                      │
│   测试场景：创建 Sandbox → NFS 文件操作 → 命令执行 → PTY → 清理       │
├─────────────────────────────────────────────────────────────────────┤
│                         集成测试                                     │
│                (多模块联合，真实 Docker/NFS)                          │
│                                                                      │
│   测试范围：API + Docker + NFS + gRPC + SQLite                       │
├─────────────────────────────────────────────────────────────────────┤
│                         单元测试                                     │
│                   (单模块，Mock 外部依赖)                             │
│                                                                      │
│   测试范围：各 Service、Manager、工具函数                             │
└─────────────────────────────────────────────────────────────────────┘
```

### 11.2 单元测试设计

#### 11.2.1 SandboxService 测试

| 测试用例 | 输入 | 预期结果 | 覆盖场景 |
|---------|------|---------|---------|
| test_create_sandbox_success | 有效模板、配置 | 返回 Sandbox，状态 starting | 正常创建 |
| test_create_sandbox_invalid_template | 不存在的模板 | TemplateNotFoundError | 模板验证 |
| test_create_sandbox_limit_exceeded | 超过最大数量 | SandboxLimitExceededError | 资源限制 |
| test_get_sandbox_not_found | 不存在的 ID | SandboxNotFoundError | 查询失败 |
| test_delete_sandbox_running | running 状态 | 状态变为 stopping → 删除成功 | 正常删除 |
| test_delete_sandbox_not_found | 不存在的 ID | SandboxNotFoundError | 删除失败 |
| test_extend_sandbox_success | 有效 ID，1800s | expires_at 增加 1800s | 延长超时 |
| test_extend_sandbox_max_limit | 超过最大延长 | 参数错误 | 限制验证 |
| test_batch_delete_partial_failure | 部分无效 ID | 返回成功和失败列表 | 批量删除 |

**状态机测试：**

| 测试用例 | 初始状态 | 触发事件 | 预期状态 |
|---------|---------|---------|---------|
| test_state_starting_to_running | starting | Agent 注册 | running |
| test_state_starting_to_error_timeout | starting | 连接超时 | error |
| test_state_running_to_stopping | running | 调用 delete | stopping |
| test_state_running_to_error | running | Agent 断开 | error |
| test_state_running_to_stopped | running | 超时过期 | stopped |

#### 11.2.2 ProcessService 测试

| 测试用例 | 输入 | 预期结果 | 覆盖场景 |
|---------|------|---------|---------|
| test_run_command_sync | "echo hello" | exitCode=0, stdout="hello\n" | 同步执行 |
| test_run_command_with_env | cmd + envs | 环境变量生效 | 环境变量 |
| test_run_command_timeout | sleep 100, timeout=1s | ProcessTimeoutError | 超时处理 |
| test_run_command_large_output | 大输出命令 | 输出截断，truncated=true | 输出限制 |
| test_run_command_stream | 长时间命令 | 收到多个 stdout 事件 | 流式输出 |
| test_kill_command_sigterm | 运行中命令 | 命令终止，exit_code=-15 | 正常终止 |
| test_kill_command_sigkill | 运行中命令 | 命令强制终止 | 强制终止 |
| test_kill_command_not_found | 不存在的 cmdId | CommandNotFoundError | 命令不存在 |

#### 11.2.3 PTYService 测试

| 测试用例 | 输入 | 预期结果 | 覆盖场景 |
|---------|------|---------|---------|
| test_create_pty_success | cols=80, rows=24 | 返回 PTY ID 和 WS URL | 正常创建 |
| test_create_pty_limit_exceeded | 超过限制数量 | PTYLimitExceededError | 数量限制 |
| test_pty_input_output | 输入 "ls\n" | 收到目录列表输出 | 输入输出 |
| test_pty_resize | 新 cols/rows | resize 成功 | 调整大小 |
| test_pty_kill | 有效 PTY ID | PTY 关闭，WS 断开 | 关闭 PTY |

#### 11.2.4 AgentConnPool 测试

| 测试用例 | 输入 | 预期结果 | 覆盖场景 |
|---------|------|---------|---------|
| test_register_agent | sandbox_id + stream | 加入连接池 | 注册成功 |
| test_register_agent_duplicate | 重复 sandbox_id | 替换旧连接 | 重复注册 |
| test_get_connection_not_found | 不存在的 sandbox_id | None | 连接不存在 |
| test_heartbeat_update | 心跳消息 | 更新 last_heartbeat | 心跳处理 |
| test_heartbeat_timeout | 90s 无心跳 | 连接标记断开 | 超时检测 |
| test_command_routing | CommandOutput | 路由到正确的 HTTP 连接 | 输出路由 |
| test_connection_cleanup | 连接断开 | 清理 pending_commands | 清理逻辑 |

#### 11.2.5 NfsProvider 测试

| 测试用例 | 输入 | 预期结果 | 覆盖场景 |
|---------|------|---------|---------|
| test_add_export | sandbox_id, path | 返回 NfsExportInfo | 添加导出 |
| test_remove_export | sandbox_id | 导出移除成功 | 移除导出 |
| test_path_traversal_blocked | "/../../../etc" | 拒绝访问 | 路径遍历防护 |
| test_path_normalization | "./foo/../bar" | 规范化为 "/bar" | 路径规范化 |

#### 11.2.6 WebhookDispatcher 测试

| 测试用例 | 输入 | 预期结果 | 覆盖场景 |
|---------|------|---------|---------|
| test_send_webhook_success | 事件数据 | HTTP 200，发送成功 | 正常发送 |
| test_send_webhook_retry | 服务器 500 | 重试 3 次 | 重试机制 |
| test_send_webhook_timeout | 响应超时 | 重试，最终失败 | 超时处理 |
| test_webhook_signature | 事件 + secret | 正确的 HMAC 签名 | 签名验证 |

### 11.3 集成测试设计

#### 11.3.1 Sandbox 生命周期集成测试

```
测试环境：真实 Docker + SQLite + Mock NFS

test_sandbox_full_lifecycle:
    ┌─────────────────────────────────────────────────────────────┐
    │ 1. 创建 Sandbox                                              │
    │    - POST /api/v1/sandboxes                                  │
    │    - 验证：DB 记录创建，容器启动，状态 starting → running     │
    ├─────────────────────────────────────────────────────────────┤
    │ 2. 查询 Sandbox                                              │
    │    - GET /api/v1/sandboxes/{id}                              │
    │    - 验证：返回正确信息，NFS 路径有效                         │
    ├─────────────────────────────────────────────────────────────┤
    │ 3. 列表查询                                                  │
    │    - GET /api/v1/sandboxes?state=running                     │
    │    - 验证：包含刚创建的 Sandbox                               │
    ├─────────────────────────────────────────────────────────────┤
    │ 4. 延长超时                                                  │
    │    - POST /api/v1/sandboxes/{id}/extend                      │
    │    - 验证：expires_at 更新                                   │
    ├─────────────────────────────────────────────────────────────┤
    │ 5. 获取资源统计                                              │
    │    - GET /api/v1/sandboxes/{id}/stats                        │
    │    - 验证：返回 CPU/Memory/Disk 数据                         │
    ├─────────────────────────────────────────────────────────────┤
    │ 6. 删除 Sandbox                                              │
    │    - DELETE /api/v1/sandboxes/{id}                           │
    │    - 验证：容器删除，DB 记录更新，workspace 清理              │
    └─────────────────────────────────────────────────────────────┘
```

#### 11.3.2 命令执行集成测试

```
test_process_execution:
    ┌─────────────────────────────────────────────────────────────┐
    │ 前置条件：创建 running 状态的 Sandbox                        │
    ├─────────────────────────────────────────────────────────────┤
    │ 1. 同步执行简单命令                                          │
    │    - POST /process/run {command: "echo hello"}               │
    │    - 验证：exitCode=0, stdout 包含 "hello"                   │
    ├─────────────────────────────────────────────────────────────┤
    │ 2. 同步执行带环境变量                                        │
    │    - POST /process/run {command: "echo $FOO", envs: {FOO:x}} │
    │    - 验证：stdout 包含 "x"                                   │
    ├─────────────────────────────────────────────────────────────┤
    │ 3. 流式执行长命令                                            │
    │    - POST /process/run {command: "for i in 1 2 3; do ..."}   │
    │    - 验证：收到多个 SSE 事件，顺序正确                        │
    ├─────────────────────────────────────────────────────────────┤
    │ 4. 命令超时                                                  │
    │    - POST /process/run {command: "sleep 100", timeout: 1000} │
    │    - 验证：收到 error 事件，进程被终止                        │
    ├─────────────────────────────────────────────────────────────┤
    │ 5. 终止运行中命令                                            │
    │    - 启动长命令，调用 POST /process/{cmdId}/kill             │
    │    - 验证：命令终止，收到 exit 事件                          │
    └─────────────────────────────────────────────────────────────┘
```

#### 11.3.3 PTY 集成测试

```
test_pty_interaction:
    ┌─────────────────────────────────────────────────────────────┐
    │ 1. 创建 PTY                                                  │
    │    - POST /pty {cols: 80, rows: 24}                          │
    │    - 验证：返回 pty_id 和 websocket_url                      │
    ├─────────────────────────────────────────────────────────────┤
    │ 2. WebSocket 连接                                            │
    │    - 连接返回的 websocket_url                                │
    │    - 验证：连接成功，收到 shell 提示符                        │
    ├─────────────────────────────────────────────────────────────┤
    │ 3. 输入输出                                                  │
    │    - 发送 {type: "input", data: base64("ls\n")}              │
    │    - 验证：收到目录列表输出                                   │
    ├─────────────────────────────────────────────────────────────┤
    │ 4. 调整大小                                                  │
    │    - 发送 {type: "resize", cols: 120, rows: 40}              │
    │    - 验证：resize 成功（可通过 stty size 验证）               │
    ├─────────────────────────────────────────────────────────────┤
    │ 5. 关闭 PTY                                                  │
    │    - DELETE /pty/{ptyId}                                     │
    │    - 验证：WebSocket 断开，收到关闭消息                       │
    └─────────────────────────────────────────────────────────────┘
```

#### 11.3.4 NFS 集成测试

```
test_nfs_operations:
    ┌─────────────────────────────────────────────────────────────┐
    │ 1. NFS 挂载                                                  │
    │    - mount -t nfs server:/{sandbox_id} /mnt/test             │
    │    - 验证：挂载成功                                          │
    ├─────────────────────────────────────────────────────────────┤
    │ 2. 文件写入                                                  │
    │    - echo "test" > /mnt/test/file.txt                        │
    │    - 验证：文件创建成功                                       │
    ├─────────────────────────��───────────────────────────────────┤
    │ 3. 容器内可见                                                │
    │    - 在容器内执行 cat /workspace/file.txt                    │
    │    - 验证：内容为 "test"                                     │
    ├─────────────────────────────────────────────────────────────┤
    │ 4. 容器内写入                                                │
    │    - 在容器内执行 echo "from container" > /workspace/new.txt │
    │    - 验证：NFS 客户端可读取                                   │
    ├─────────────────────────────────────────────────────────────┤
    │ 5. 大文件传输                                                │
    │    - dd if=/dev/zero of=/mnt/test/large.bin bs=1M count=100  │
    │    - 验证：传输成功，性能达标                                 │
    └─────────────────────────────────────────────────────────────┘
```

#### 11.3.5 Agent 连接集成测试

```
test_agent_connection:
    ┌─────────────────────────────────────────────────────────────┐
    │ 1. Agent 注册                                                │
    │    - 容器启动后 Agent 发送 RegisterRequest                   │
    │    - 验证：Server 收到注册，Sandbox 状态变为 running          │
    ├─────────────────────────────────────────────────────────────┤
    │ 2. 心跳机制                                                  │
    │    - Agent 每 30s 发送心跳                                   │
    │    - 验证：Server 更新 last_heartbeat                        │
    ├─────────────────────────────────────────────────────────────┤
    │ 3. 连接断开检测                                              │
    │    - 强制停止 Agent 进程                                     │
    │    - 验证：90s 后 Sandbox 状态变为 error                     │
    ├─────────────────────────────────────────────────────────────┤
    │ 4. 重连恢复                                                  │
    │    - 重启 Agent，重新注册                                    │
    │    - 验证：连接恢复，命令可执行                               │
    └─────────────────────────────────────────────────────────────┘
```

#### 11.3.6 服务重启恢复测试

```
test_service_restart_recovery:
    ┌─────────────────────────────────────────────────────────────┐
    │ 1. 准备状态                                                  │
    │    - 创建 3 个 running 状态的 Sandbox                        │
    │    - 1 个正在执行长命令                                      │
    ├─────────────────────────────────────────────────────────────┤
    │ 2. 重启 Server                                               │
    │    - 停止 Server 进程                                        │
    │    - 重新启动 Server                                         │
    ├─────────────────────────────────────────────────────────────┤
    │ 3. 验证恢复                                                  │
    │    - DB 中 Sandbox 记录保留                                  │
    │    - 容器仍在运行的 Sandbox 等待 Agent 重连                   │
    │    - Agent 重连后状态恢复为 running                          │
    │    - 容器已停止的 Sandbox 标记为 stopped/error               │
    └─────────────────────────────────────────────────────────────┘
```

#### 11.3.7 Webhook 集成测试

```
test_webhook_events:
    ┌─────────────────────────────────────────────────────────────┐
    │ 前置条件：配置 Webhook URL (Mock Server)                     │
    ├─────────────────────────────────────────────────────────────┤
    │ 1. sandbox.created 事件                                      │
    │    - 创建 Sandbox                                            │
    │    - 验证：收到 sandbox.created webhook                      │
    ├─────────────────────────────────────────────────────────────┤
    │ 2. sandbox.running 事件                                      │
    │    - Agent 连接就绪                                          │
    │    - 验证：收到 sandbox.running webhook                      │
    ├─────────────────────────────────────────────────────────────┤
    │ 3. agent.connected / disconnected 事件                       │
    │    - Agent 连接/断开                                         │
    │    - 验证：收到对应 webhook                                   │
    ├─────────────────────────────────────────────────────────────┤
    │ 4. sandbox.deleted 事件                                      │
    │    - 删除 Sandbox                                            │
    │    - 验证：收到 sandbox.deleted webhook                      │
    ├─────────────────────────────────────────────────────────────┤
    │ 5. Webhook 重试                                              │
    │    - Mock Server 返回 500                                    │
    │    - 验证：重试 3 次，间隔符合指数退避                        │
    └─────────────────────────────────────────────────────────────┘
```

### 11.4 E2E 测试设计

#### 11.4.1 测试环境

```
┌─────────────────────────────────────────────────────────────────┐
│                        E2E 测试环境                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   ┌──────────────┐      ┌──────────────┐      ┌──────────────┐  │
│   │  Test Runner │      │   Workspace  │      │   Docker     │  │
│   │  (pytest/    │ ───> │   Server     │ ───> │   Engine     │  │
│   │   jest)      │      │              │      │              │  │
│   └──────────────┘      └──────────────┘      └──────────────┘  │
│          │                     │                     │          │
│          │              ┌──────┴──────┐              │          │
│          │              │             │              │          │
│          ▼              ▼             ▼              ▼          │
│   ┌──────────────┐ ┌─────────┐ ┌──────────┐ ┌──────────────┐   │
│   │  SDK Client  │ │  NFS    │ │  SQLite  │ │  Sandbox     │   │
│   │  (TS/Python) │ │  Server │ │          │ │  Containers  │   │
│   └──────────────┘ └─────────┘ └──────────┘ └──────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

#### 11.4.2 E2E 测试用例 (TypeScript)

```typescript
// e2e/sandbox.test.ts

describe('Sandbox E2E Tests', () => {
  let client: WorkspaceClient;

  beforeAll(() => {
    client = new WorkspaceClient({
      apiUrl: process.env.WORKSPACE_API_URL,
      apiKey: process.env.WORKSPACE_API_KEY,
    });
  });

  describe('完整工作流测试', () => {
    test('创建 Sandbox → 执行命令 → 删除', async () => {
      // 1. 创建 Sandbox
      const sandbox = await client.sandbox.create({
        template: 'python:3.11',
        name: 'e2e-test-basic',
      });

      expect(sandbox.id).toMatch(/^sbx-/);
      expect(sandbox.state).toBe('running');
      expect(sandbox.nfs.mountCommand).toBeTruthy();

      try {
        // 2. 执行简单命令
        const result = await sandbox.process.run('python --version');
        expect(result.exitCode).toBe(0);
        expect(result.stdout).toContain('Python 3.11');

        // 3. 执行带环境变量的命令
        const envResult = await sandbox.process.run('echo $MY_VAR', {
          envs: { MY_VAR: 'hello-e2e' },
        });
        expect(envResult.stdout.trim()).toBe('hello-e2e');

      } finally {
        // 4. 清理
        await client.sandbox.delete(sandbox.id);
      }

      // 5. 验证删除
      await expect(client.sandbox.get(sandbox.id))
        .rejects.toThrow(SandboxNotFoundError);
    });

    test('流式命令执行', async () => {
      const sandbox = await client.sandbox.create({
        template: 'python:3.11',
      });

      try {
        const events: StreamEvent[] = [];

        // 执行会产生多行输出的命令
        for await (const event of sandbox.process.runStream(
          'for i in 1 2 3; do echo "line $i"; sleep 0.1; done'
        )) {
          events.push(event);
        }

        // 验证收到多个 stdout 事件
        const stdoutEvents = events.filter(e => e.type === 'stdout');
        expect(stdoutEvents.length).toBeGreaterThanOrEqual(3);

        // 验证最后收到 exit 事件
        const exitEvent = events.find(e => e.type === 'exit');
        expect(exitEvent).toBeDefined();
        expect(exitEvent!.exitCode).toBe(0);

      } finally {
        await client.sandbox.delete(sandbox.id);
      }
    });

    test('命令超时处理', async () => {
      const sandbox = await client.sandbox.create({
        template: 'python:3.11',
      });

      try {
        await expect(
          sandbox.process.run('sleep 100', { timeout: 1000 })
        ).rejects.toThrow(ProcessTimeoutError);

      } finally {
        await client.sandbox.delete(sandbox.id);
      }
    });

    test('命令终止', async () => {
      const sandbox = await client.sandbox.create({
        template: 'python:3.11',
      });

      try {
        const events: StreamEvent[] = [];
        let commandId: string | undefined;

        // 启动长时间运行的命令
        const streamPromise = (async () => {
          for await (const event of sandbox.process.runStream('sleep 100')) {
            events.push(event);
            if (!commandId && event.commandId) {
              commandId = event.commandId;
            }
          }
        })();

        // 等待命令开始
        await new Promise(resolve => setTimeout(resolve, 500));

        // 终止命令
        expect(commandId).toBeDefined();
        await sandbox.process.kill(commandId!);

        await streamPromise;

        // 验证收到 exit 事件
        const exitEvent = events.find(e => e.type === 'exit');
        expect(exitEvent).toBeDefined();

      } finally {
        await client.sandbox.delete(sandbox.id);
      }
    });
  });

  describe('NFS 文件操作测试', () => {
    test('通过 NFS 写入文件，容器内执行', async () => {
      const sandbox = await client.sandbox.create({
        template: 'python:3.11',
      });

      try {
        // 1. 挂载 NFS (测试环境中预挂载)
        const mountPoint = `/tmp/e2e-${sandbox.id}`;
        await execAsync(`mkdir -p ${mountPoint}`);
        await execAsync(sandbox.nfs.mountCommand.replace('/mnt', mountPoint));

        try {
          // 2. 写入 Python 文件
          const code = `
import json
print(json.dumps({"status": "ok", "source": "nfs"}))
`;
          await fs.writeFile(`${mountPoint}/test.py`, code);

          // 3. 在容器内执行
          const result = await sandbox.process.run('python /workspace/test.py');
          expect(result.exitCode).toBe(0);

          const output = JSON.parse(result.stdout);
          expect(output.status).toBe('ok');
          expect(output.source).toBe('nfs');

        } finally {
          await execAsync(`umount ${mountPoint}`);
          await execAsync(`rmdir ${mountPoint}`);
        }

      } finally {
        await client.sandbox.delete(sandbox.id);
      }
    });
  });

  describe('PTY 交互测试', () => {
    test('PTY 创建和基本交互', async () => {
      const sandbox = await client.sandbox.create({
        template: 'python:3.11',
      });

      try {
        // 1. 创建 PTY
        const pty = await sandbox.pty.create({
          cols: 80,
          rows: 24,
        });
        expect(pty.id).toBeTruthy();
        expect(pty.websocketUrl).toContain('/ws');

        // 2. 连接 WebSocket
        const ws = new WebSocket(pty.websocketUrl);
        const outputs: string[] = [];

        await new Promise<void>((resolve, reject) => {
          ws.onopen = () => {
            // 发送命令
            ws.send(JSON.stringify({
              type: 'input',
              data: Buffer.from('echo "pty-test"\n').toString('base64'),
            }));
          };

          ws.onmessage = (event) => {
            const msg = JSON.parse(event.data);
            if (msg.type === 'output') {
              outputs.push(Buffer.from(msg.data, 'base64').toString());
            }
          };

          // 等待输出
          setTimeout(() => {
            ws.close();
            resolve();
          }, 2000);

          ws.onerror = reject;
        });

        // 3. 验证输出
        const allOutput = outputs.join('');
        expect(allOutput).toContain('pty-test');

        // 4. 关闭 PTY
        await sandbox.pty.kill(pty.id);

      } finally {
        await client.sandbox.delete(sandbox.id);
      }
    });
  });

  describe('资源管理测试', () => {
    test('资源自动清理 (using)', async () => {
      let sandboxId: string;

      {
        await using sandbox = await client.sandbox.create({
          template: 'python:3.11',
        });
        sandboxId = sandbox.id;

        const result = await sandbox.process.run('echo test');
        expect(result.exitCode).toBe(0);
      }
      // 退出作用域后自动删除

      // 验证已删除
      await expect(client.sandbox.get(sandboxId))
        .rejects.toThrow(SandboxNotFoundError);
    });

    test('批量删除', async () => {
      // 创建多个 Sandbox
      const sandboxes = await Promise.all([
        client.sandbox.create({ template: 'python:3.11' }),
        client.sandbox.create({ template: 'python:3.11' }),
        client.sandbox.create({ template: 'python:3.11' }),
      ]);

      const ids = sandboxes.map(s => s.id);

      // 批量删除
      const result = await client.sandbox.batchDelete(ids);

      expect(result.succeeded).toHaveLength(3);
      expect(result.failed).toHaveLength(0);

      // 验证全部删除
      for (const id of ids) {
        await expect(client.sandbox.get(id))
          .rejects.toThrow(SandboxNotFoundError);
      }
    });

    test('超时自动清理', async () => {
      const sandbox = await client.sandbox.create({
        template: 'python:3.11',
        timeout_seconds: 5, // 5 秒超时
      });

      // 等待超时
      await new Promise(resolve => setTimeout(resolve, 70000)); // 等待清理任务执行

      // 验证已清理
      const info = await client.sandbox.get(sandbox.id);
      expect(info.state).toBe('stopped');
    }, 80000);
  });

  describe('错误处理测试', () => {
    test('无效模板', async () => {
      await expect(
        client.sandbox.create({ template: 'nonexistent:latest' })
      ).rejects.toThrow(TemplateNotFoundError);
    });

    test('未授权访问', async () => {
      const invalidClient = new WorkspaceClient({
        apiUrl: process.env.WORKSPACE_API_URL,
        apiKey: 'invalid-key',
      });

      await expect(
        invalidClient.sandbox.create({ template: 'python:3.11' })
      ).rejects.toThrow(UnauthorizedError);
    });

    test('Sandbox 不存在', async () => {
      await expect(
        client.sandbox.get('sbx-nonexistent')
      ).rejects.toThrow(SandboxNotFoundError);
    });
  });
});
```

#### 11.4.3 E2E 测试用例 (Python)

```python
# e2e/test_sandbox.py

import pytest
import asyncio
import os
from workspace_sdk import WorkspaceClient, CreateSandboxParams
from workspace_sdk.errors import (
    SandboxNotFoundError,
    TemplateNotFoundError,
    ProcessTimeoutError,
    UnauthorizedError,
)


@pytest.fixture
async def client():
    return WorkspaceClient(
        api_url=os.environ["WORKSPACE_API_URL"],
        api_key=os.environ["WORKSPACE_API_KEY"],
    )


class TestSandboxE2E:
    """Sandbox E2E 测试"""

    @pytest.mark.asyncio
    async def test_full_workflow(self, client):
        """完整工作流：创建 → 执行 → 删除"""
        # 1. 创建 Sandbox
        sandbox = await client.sandbox.create(
            CreateSandboxParams(
                template="python:3.11",
                name="e2e-test-python",
            )
        )

        assert sandbox.id.startswith("sbx-")
        assert sandbox.state == "running"

        try:
            # 2. 执行命令
            result = await sandbox.process.run("python --version")
            assert result.exit_code == 0
            assert "Python 3.11" in result.stdout

            # 3. 带环境变量执行
            env_result = await sandbox.process.run(
                "echo $TEST_VAR",
                envs={"TEST_VAR": "hello-python"}
            )
            assert env_result.stdout.strip() == "hello-python"

        finally:
            # 4. 清理
            await client.sandbox.delete(sandbox.id)

        # 5. 验证删除
        with pytest.raises(SandboxNotFoundError):
            await client.sandbox.get(sandbox.id)

    @pytest.mark.asyncio
    async def test_stream_execution(self, client):
        """流式命令执行"""
        sandbox = await client.sandbox.create(
            CreateSandboxParams(template="python:3.11")
        )

        try:
            events = []
            async for event in sandbox.process.run_stream(
                'for i in 1 2 3; do echo "line $i"; sleep 0.1; done'
            ):
                events.append(event)

            # 验证收到多个事件
            stdout_events = [e for e in events if e.type == "stdout"]
            assert len(stdout_events) >= 3

            # 验证 exit 事件
            exit_events = [e for e in events if e.type == "exit"]
            assert len(exit_events) == 1
            assert exit_events[0].exit_code == 0

        finally:
            await client.sandbox.delete(sandbox.id)

    @pytest.mark.asyncio
    async def test_command_timeout(self, client):
        """命令超时处理"""
        sandbox = await client.sandbox.create(
            CreateSandboxParams(template="python:3.11")
        )

        try:
            with pytest.raises(ProcessTimeoutError):
                await sandbox.process.run("sleep 100", timeout=1000)

        finally:
            await client.sandbox.delete(sandbox.id)

    @pytest.mark.asyncio
    async def test_command_kill(self, client):
        """命令终止"""
        sandbox = await client.sandbox.create(
            CreateSandboxParams(template="python:3.11")
        )

        try:
            events = []
            command_id = None

            async def collect_events():
                nonlocal command_id
                async for event in sandbox.process.run_stream("sleep 100"):
                    events.append(event)
                    if command_id is None and hasattr(event, 'command_id'):
                        command_id = event.command_id

            # 启动命令
            task = asyncio.create_task(collect_events())

            # 等待命令开始
            await asyncio.sleep(0.5)

            # 终止命令
            assert command_id is not None
            await sandbox.process.kill(command_id)

            await task

            # 验证收到 exit 事件
            exit_events = [e for e in events if e.type == "exit"]
            assert len(exit_events) == 1

        finally:
            await client.sandbox.delete(sandbox.id)


class TestPTYE2E:
    """PTY E2E 测试"""

    @pytest.mark.asyncio
    async def test_pty_basic_interaction(self, client):
        """PTY 基本交互"""
        sandbox = await client.sandbox.create(
            CreateSandboxParams(template="python:3.11")
        )

        try:
            # 创建 PTY
            pty = await sandbox.pty.create(cols=80, rows=24)
            assert pty.id
            assert pty.websocket_url

            # 通过 WebSocket 交互
            import websockets

            outputs = []
            async with websockets.connect(pty.websocket_url) as ws:
                # 发送命令
                await ws.send(json.dumps({
                    "type": "input",
                    "data": base64.b64encode(b"echo pty-test\n").decode()
                }))

                # 收集输出
                try:
                    async for msg in asyncio.timeout(2):
                        data = json.loads(msg)
                        if data["type"] == "output":
                            outputs.append(
                                base64.b64decode(data["data"]).decode()
                            )
                except asyncio.TimeoutError:
                    pass

            # 验证输出
            all_output = "".join(outputs)
            assert "pty-test" in all_output

            # 关闭 PTY
            await sandbox.pty.kill(pty.id)

        finally:
            await client.sandbox.delete(sandbox.id)


class TestResourceManagement:
    """资源管理测试"""

    @pytest.mark.asyncio
    async def test_batch_delete(self, client):
        """批量删除"""
        # 创建多个 Sandbox
        sandboxes = await asyncio.gather(*[
            client.sandbox.create(CreateSandboxParams(template="python:3.11"))
            for _ in range(3)
        ])

        ids = [s.id for s in sandboxes]

        # 批量删除
        result = await client.sandbox.batch_delete(ids)

        assert len(result.succeeded) == 3
        assert len(result.failed) == 0

        # 验证全部删除
        for sandbox_id in ids:
            with pytest.raises(SandboxNotFoundError):
                await client.sandbox.get(sandbox_id)

    @pytest.mark.asyncio
    async def test_sandbox_stats(self, client):
        """资源统计"""
        sandbox = await client.sandbox.create(
            CreateSandboxParams(template="python:3.11")
        )

        try:
            # 执行一些操作产生资源使用
            await sandbox.process.run("dd if=/dev/zero of=/tmp/test bs=1M count=10")

            # 获取统计
            stats = await client.sandbox.stats(sandbox.id)

            assert stats.cpu.usage >= 0
            assert stats.memory.usage > 0
            assert stats.disk.usage > 0

        finally:
            await client.sandbox.delete(sandbox.id)


class TestErrorHandling:
    """错误处理测试"""

    @pytest.mark.asyncio
    async def test_invalid_template(self, client):
        """无效模板"""
        with pytest.raises(TemplateNotFoundError):
            await client.sandbox.create(
                CreateSandboxParams(template="nonexistent:latest")
            )

    @pytest.mark.asyncio
    async def test_unauthorized(self):
        """未授权访问"""
        invalid_client = WorkspaceClient(
            api_url=os.environ["WORKSPACE_API_URL"],
            api_key="invalid-key",
        )

        with pytest.raises(UnauthorizedError):
            await invalid_client.sandbox.create(
                CreateSandboxParams(template="python:3.11")
            )

    @pytest.mark.asyncio
    async def test_sandbox_not_found(self, client):
        """Sandbox 不存在"""
        with pytest.raises(SandboxNotFoundError):
            await client.sandbox.get("sbx-nonexistent")
```

### 11.5 性能测试设计

#### 11.5.1 性能测试场景

| 测试场景 | 目标指标 | 测试方法 |
|---------|---------|---------|
| Sandbox 创建延迟 | P99 < 5s | 串行创建 100 个，统计延迟分布 |
| 并发创建能力 | 50 并发成功 | 同时创建 50 个 Sandbox |
| 命令执行 QPS | > 100/s | 压测简单命令 (echo) |
| 流式输出吞吐 | > 10MB/s | 大量输出命令测试 |
| NFS 读写吞吐 | > 100MB/s | dd 读写测试 |
| PTY 响应延迟 | P99 < 50ms | 输入到输出延迟 |
| 长连接稳定性 | 24h 无断开 | 持续运行测试 |

#### 11.5.2 性能测试脚本

```python
# perf/test_performance.py

import asyncio
import time
import statistics
from dataclasses import dataclass


@dataclass
class PerfResult:
    min_ms: float
    max_ms: float
    avg_ms: float
    p50_ms: float
    p95_ms: float
    p99_ms: float
    success_rate: float


async def test_sandbox_creation_latency(client, count=100) -> PerfResult:
    """Sandbox 创建延迟测试"""
    latencies = []
    failures = 0

    for i in range(count):
        start = time.perf_counter()
        try:
            sandbox = await client.sandbox.create(
                CreateSandboxParams(template="python:3.11")
            )
            latency = (time.perf_counter() - start) * 1000
            latencies.append(latency)

            # 清理
            await client.sandbox.delete(sandbox.id)
        except Exception as e:
            failures += 1
            print(f"Creation {i} failed: {e}")

    return PerfResult(
        min_ms=min(latencies),
        max_ms=max(latencies),
        avg_ms=statistics.mean(latencies),
        p50_ms=statistics.median(latencies),
        p95_ms=statistics.quantiles(latencies, n=20)[18],
        p99_ms=statistics.quantiles(latencies, n=100)[98],
        success_rate=(count - failures) / count * 100,
    )


async def test_concurrent_creation(client, concurrency=50) -> PerfResult:
    """并发创建测试"""

    async def create_and_delete():
        start = time.perf_counter()
        sandbox = await client.sandbox.create(
            CreateSandboxParams(template="python:3.11")
        )
        latency = (time.perf_counter() - start) * 1000
        await client.sandbox.delete(sandbox.id)
        return latency

    tasks = [create_and_delete() for _ in range(concurrency)]
    results = await asyncio.gather(*tasks, return_exceptions=True)

    latencies = [r for r in results if isinstance(r, float)]
    failures = len([r for r in results if isinstance(r, Exception)])

    return PerfResult(
        min_ms=min(latencies) if latencies else 0,
        max_ms=max(latencies) if latencies else 0,
        avg_ms=statistics.mean(latencies) if latencies else 0,
        p50_ms=statistics.median(latencies) if latencies else 0,
        p95_ms=statistics.quantiles(latencies, n=20)[18] if len(latencies) >= 20 else 0,
        p99_ms=statistics.quantiles(latencies, n=100)[98] if len(latencies) >= 100 else 0,
        success_rate=(concurrency - failures) / concurrency * 100,
    )


async def test_command_execution_qps(client, duration_seconds=60) -> dict:
    """命令执行 QPS 测试"""
    sandbox = await client.sandbox.create(
        CreateSandboxParams(template="python:3.11")
    )

    try:
        success_count = 0
        failure_count = 0
        start_time = time.perf_counter()

        while (time.perf_counter() - start_time) < duration_seconds:
            try:
                result = await sandbox.process.run("echo test")
                if result.exit_code == 0:
                    success_count += 1
                else:
                    failure_count += 1
            except Exception:
                failure_count += 1

        elapsed = time.perf_counter() - start_time

        return {
            "qps": success_count / elapsed,
            "total_requests": success_count + failure_count,
            "success_rate": success_count / (success_count + failure_count) * 100,
        }

    finally:
        await client.sandbox.delete(sandbox.id)
```

### 11.6 安全测试设计

#### 11.6.1 安全测试用例

| 测试类别 | 测试用例 | 预期结果 |
|---------|---------|---------|
| **路径遍历** | NFS 访问 `/../../../etc/passwd` | 拒绝访问 |
| | NFS 访问 `/workspace/../../../etc/shadow` | 拒绝访问 |
| | 符号链接指向外部 | 拒绝访问 |
| **命令注入** | 命令包含 `; rm -rf /` | 在沙箱内执行，不影响宿主机 |
| | 命令包含 `$(cat /etc/passwd)` | 只能访问容器内文件 |
| | 命令包含 `\`whoami\`` | 返回容器内用户 |
| **资源耗尽** | Fork 炸弹 `:(){ :\|:& };:` | 被 cgroup 限制 |
| | 内存耗尽 `stress --vm 1 --vm-bytes 10G` | 被 OOM kill |
| | 磁盘填满 `dd if=/dev/zero of=/workspace/huge` | 被磁盘限制 |
| **认证绕过** | 无 API Key 访问 | 401 Unauthorized |
| | 错误 API Key 访问 | 401 Unauthorized |
| | 访问其他 Sandbox | 403 Forbidden |
| **容器逃逸** | 挂载宿主机目录 | 失败 |
| | 访问 Docker socket | 失败 |
| | 提权尝试 | 失败 |

#### 11.6.2 安全测试脚本

```python
# security/test_security.py

class TestPathTraversal:
    """路径遍历测试"""

    @pytest.mark.asyncio
    async def test_nfs_path_traversal(self, mounted_sandbox):
        """NFS 路径遍历攻击"""
        mount_point = mounted_sandbox["mount_point"]

        # 尝试访问 /etc/passwd
        traversal_paths = [
            "../../../etc/passwd",
            "..%2f..%2f..%2fetc%2fpasswd",
            "....//....//....//etc/passwd",
        ]

        for path in traversal_paths:
            target = os.path.join(mount_point, path)
            # 应该无法读取或路径被规范化到沙箱内
            assert not os.path.exists(target) or \
                   os.path.realpath(target).startswith(mount_point)


class TestCommandInjection:
    """命令注入测试"""

    @pytest.mark.asyncio
    async def test_command_injection_semicolon(self, sandbox):
        """分号注入"""
        # 即使注入成功，也只影响容器内
        result = await sandbox.process.run("echo hello; whoami")
        assert "root" not in result.stdout or "sandbox" in result.stdout

    @pytest.mark.asyncio
    async def test_command_injection_backtick(self, sandbox):
        """反引号注入"""
        result = await sandbox.process.run("echo `cat /etc/hostname`")
        # 应该是容器的 hostname，不是宿主机
        assert result.exit_code == 0


class TestResourceExhaustion:
    """资源耗尽测试"""

    @pytest.mark.asyncio
    async def test_fork_bomb(self, sandbox):
        """Fork 炸弹"""
        # 应该被 cgroup 限制
        result = await sandbox.process.run(
            ":(){ :|:& };:",
            timeout=5000
        )
        # 命令应该失败或被终止
        assert result.exit_code != 0 or "resource" in result.stderr.lower()

    @pytest.mark.asyncio
    async def test_memory_exhaustion(self, sandbox):
        """内存耗尽"""
        result = await sandbox.process.run(
            "python -c 'x = \"a\" * (1024**3)'",  # 尝试分配 1GB
            timeout=10000
        )
        # 应该被 OOM kill 或返回错误
        assert result.exit_code != 0


class TestAuthentication:
    """认证测试"""

    @pytest.mark.asyncio
    async def test_no_api_key(self):
        """无 API Key"""
        client = WorkspaceClient(
            api_url=os.environ["WORKSPACE_API_URL"],
            api_key="",
        )
        with pytest.raises(UnauthorizedError):
            await client.sandbox.list()

    @pytest.mark.asyncio
    async def test_invalid_api_key(self):
        """无效 API Key"""
        client = WorkspaceClient(
            api_url=os.environ["WORKSPACE_API_URL"],
            api_key="invalid-key-12345",
        )
        with pytest.raises(UnauthorizedError):
            await client.sandbox.list()

    @pytest.mark.asyncio
    async def test_cross_sandbox_access(self, client):
        """跨 Sandbox 访问"""
        sandbox1 = await client.sandbox.create(
            CreateSandboxParams(template="python:3.11")
        )
        sandbox2 = await client.sandbox.create(
            CreateSandboxParams(template="python:3.11")
        )

        try:
            # sandbox1 不应该能访问 sandbox2 的文件
            result = await sandbox1.process.run(
                f"cat /data/workspaces/{sandbox2.id}/test.txt"
            )
            assert result.exit_code != 0  # 应该失败

        finally:
            await client.sandbox.delete(sandbox1.id)
            await client.sandbox.delete(sandbox2.id)
```

### 11.7 测试执行与 CI 集成

#### 11.7.1 测试目录结构

```
tests/
├── unit/                      # 单元测试
│   ├── test_sandbox_service.rs
│   ├── test_process_service.rs
│   ├── test_pty_service.rs
│   ├── test_agent_conn_pool.rs
│   ├── test_nfs_provider.rs
│   └── test_webhook_dispatcher.rs
│
├── integration/               # 集成测试
│   ├── test_sandbox_lifecycle.rs
│   ├── test_process_execution.rs
│   ├── test_pty_interaction.rs
│   ├── test_nfs_operations.rs
│   ├── test_agent_connection.rs
│   ├── test_webhook_events.rs
│   └── test_service_recovery.rs
│
├── e2e/                       # E2E 测试
│   ├── typescript/
│   │   ├── sandbox.test.ts
│   │   ├── process.test.ts
│   │   ├── pty.test.ts
│   │   └── errors.test.ts
│   └── python/
│       ├── test_sandbox.py
│       ├── test_process.py
│       ├── test_pty.py
│       └── test_errors.py
│
├── perf/                      # 性能测试
│   ├── test_creation_latency.py
│   ├── test_command_qps.py
│   └── test_nfs_throughput.py
│
├── security/                  # 安全测试
│   ├── test_path_traversal.py
│   ├── test_command_injection.py
│   ├── test_resource_exhaustion.py
│   └── test_authentication.py
│
└── fixtures/                  # 测试数据
    ├── docker-compose.test.yml
    └── test_files/
```

#### 11.7.2 CI Pipeline

```yaml
# .github/workflows/test.yml

name: Test

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run unit tests
        run: cargo test --lib

  integration-tests:
    runs-on: ubuntu-latest
    services:
      docker:
        image: docker:dind
        options: --privileged
    steps:
      - uses: actions/checkout@v4
      - name: Start test environment
        run: docker-compose -f tests/fixtures/docker-compose.test.yml up -d
      - name: Run integration tests
        run: cargo test --test '*'

  e2e-tests-typescript:
    runs-on: ubuntu-latest
    needs: integration-tests
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - name: Install dependencies
        run: cd sdk-typescript && npm install
      - name: Run E2E tests
        run: npm run test:e2e

  e2e-tests-python:
    runs-on: ubuntu-latest
    needs: integration-tests
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
      - name: Install dependencies
        run: cd sdk-python && pip install -e ".[test]"
      - name: Run E2E tests
        run: pytest tests/e2e/python/

  security-tests:
    runs-on: ubuntu-latest
    needs: integration-tests
    steps:
      - uses: actions/checkout@v4
      - name: Run security tests
        run: pytest tests/security/

  performance-tests:
    runs-on: ubuntu-latest
    needs: integration-tests
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    steps:
      - uses: actions/checkout@v4
      - name: Run performance tests
        run: pytest tests/perf/ --benchmark
```
