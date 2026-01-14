# Unified Workspace SDK 接口文档

统一工作空间 SDK 接口设计文档，用于内部 Agent 开发平台的 Sandbox/Workspace 服务实现。

---

## 文档结构

```
unified-workspace-sdk/
├── docs/
│   ├── api/                          # API 接口文档
│   │   ├── 00-rest-api.md            # REST API 规范
│   │   ├── 01-sandbox.md             # Sandbox 服务
│   │   ├── 02-filesystem.md          # FileSystem 服务
│   │   ├── 03-process.md             # Process 服务
│   │   ├── 04-pty.md                 # PTY 服务
│   │   ├── 05-git.md                 # Git 服务
│   │   ├── 06-lsp.md                 # LSP 服务
│   │   └── 07-storage.md             # Snapshot 和 Volume 服务
│   └── types/
│       └── common.md                 # 通用类型定义
└── README.md                         # 本文件
```

---

## 服务概览

| 服务 | 描述 | 文档 |
|-----|------|------|
| **Sandbox** | 沙盒生命周期管理 | [01-sandbox.md](docs/api/01-sandbox.md) |
| **FileSystem** | 文件系统操作 | [02-filesystem.md](docs/api/02-filesystem.md) |
| **Process** | 进程/命令执行 | [03-process.md](docs/api/03-process.md) |
| **PTY** | 伪终端交互 | [04-pty.md](docs/api/04-pty.md) |
| **Git** | Git 版本控制 | [05-git.md](docs/api/05-git.md) |
| **LSP** | 语言服务协议 | [06-lsp.md](docs/api/06-lsp.md) |
| **Snapshot** | 快照管理 | [07-storage.md](docs/api/07-storage.md) |
| **Volume** | 持久化存储卷 | [07-storage.md](docs/api/07-storage.md) |

---

## 快速开始

### TypeScript SDK

```typescript
import { WorkspaceClient } from '@workspace-sdk/typescript'

async function main() {
  // 创建客户端
  const client = new WorkspaceClient({
    apiUrl: 'https://api.example.com',
    apiKey: process.env.WORKSPACE_API_KEY
  })

  // 创建 Sandbox
  const sandbox = await client.sandbox.create({
    name: 'my-sandbox',
    template: 'python:3.11',
    resources: { cpu: 2, memoryMB: 4096 }
  })

  try {
    // 文件操作
    await sandbox.fs.write('/app/main.py', 'print("Hello World")')

    // 执行命令
    const result = await sandbox.process.run('python /app/main.py')
    console.log(result.stdout) // Hello World

    // Git 操作
    await sandbox.git.init('/app')
    await sandbox.git.add('/app', ['.'])
    await sandbox.git.commit('/app', { message: 'Initial commit' })

  } finally {
    await sandbox.delete()
  }
}
```

### Python SDK

```python
from workspace_sdk import WorkspaceClient

def main():
    client = WorkspaceClient()

    # 使用上下文管理器自动清理
    with client.sandbox.create(
        name='my-sandbox',
        template='python:3.11',
        resources={'cpu': 2, 'memory_mb': 4096}
    ) as sandbox:
        # 文件操作
        sandbox.fs.write('/app/main.py', 'print("Hello World")')

        # 执行命令
        result = sandbox.process.run('python /app/main.py')
        print(result.stdout)  # Hello World

        # Git 操作
        sandbox.git.init('/app')
        sandbox.git.add('/app', ['.'])
        sandbox.git.commit('/app', message='Initial commit')

if __name__ == '__main__':
    main()
```

---

## 核心功能

### 1. Sandbox 生命周期

```
┌─────────┐    create    ┌──────────┐
│ (none)  │─────────────►│ creating │
└─────────┘              └────┬─────┘
                              │
                              ▼
┌─────────┐    stop     ┌──────────┐
│ stopped │◄────────────│ running  │
└────┬────┘             └────┬─────┘
     │                       │
     │ start                 │ pause
     │                       ▼
     │               ┌──────────┐
     └──────────────►│  paused  │
                     └──────────┘
```

### 2. 文件系统操作

- 读写文件（文本/二进制/流）
- 目录操作（创建/列出/删除）
- 文件搜索（glob/grep）
- 实时监听变化

### 3. 进程执行

- 同步执行命令
- 异步进程管理
- 代码执行（多语言）
- 会话管理

### 4. 交互式终端

- 完整 PTY 支持
- xterm.js 集成
- 特殊键处理

### 5. Git 集成

- 克隆/初始化仓库
- 分支管理
- 提交/推送/拉取
- 差异查看

### 6. 语言服务 (LSP)

- 自动补全
- 转到定义
- 查找引用
- 代码诊断
- 重命名

### 7. 持久化存储

- 快照创建/恢复
- 卷挂载/共享
- 数据持久化

---

## 架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                         Client SDK                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐           │
│  │  TypeScript  │  │    Python    │  │     Sync     │           │
│  │     SDK      │  │  Async SDK   │  │  Python SDK  │           │
│  └──────────────┘  └──────────────┘  └──────────────┘           │
└────────────────────────────┬────────────────────────────────────┘
                             │
                    REST API / WebSocket
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                       API Gateway                                │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  认证  │  限流  │  路由  │  日志  │  指标                │    │
│  └─────────────────────────────────────────────────────────┘    │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Service Layer                               │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐   │
│  │ Sandbox │ │   FS    │ │ Process │ │   PTY   │ │   Git   │   │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘   │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐                            │
│  │   LSP   │ │Snapshot │ │ Volume  │                            │
│  └─────────┘ └─────────┘ └─────────┘                            │
└────────────────────────────┬────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Container Runtime                            │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  Docker  │  Kubernetes  │  Firecracker  │  ...          │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

---

## 功能对比

本 SDK 设计参考了 E2B 和 Daytona 两个项目的接口设计：

| 功能 | E2B | Daytona | Unified SDK |
|-----|-----|---------|-------------|
| Sandbox 管理 | ✓ | ✓ | ✓ |
| 文件系统 | ✓ | ✓ | ✓ |
| 进程执行 | ✓ | ✓ | ✓ |
| PTY 终端 | ✓ | ✓ | ✓ |
| Git 集成 | ✗ | ✓ | ✓ |
| LSP 支持 | ✗ | ✓ | ✓ |
| 快照 | ✗ | ✓ | ✓ |
| 持久化卷 | ✗ | ✓ | ✓ |
| 网络隔离 | ✓ | ✗ | ✓ |
| 代码执行 | ✓ | ✓ | ✓ |
| 会话管理 | ✗ | ✓ | ✓ |

---

## 实现建议

### 后端实现

1. **容器运行时**: 推荐使用 Kubernetes 或 Firecracker
2. **API 框架**: Go (Gin/Echo) 或 Node.js (Fastify)
3. **WebSocket**: 使用 gorilla/websocket 或 socket.io
4. **数据库**: PostgreSQL (元数据) + Redis (缓存)
5. **消息队列**: Kafka 或 NATS (事件通知)

### SDK 生成

1. 使用 OpenAPI Generator 从 `openapi.yaml` 生成基础代码
2. 手动实现 WebSocket 和流式操作
3. 添加重试、超时、错误处理逻辑

### 测试策略

1. 单元测试: 各服务接口测试
2. 集成测试: 完整工作流测试
3. 压力测试: 并发创建/执行测试
4. E2E 测试: SDK 端到端测试

---

## 错误码速查

| 范围 | 类别 |
|-----|------|
| 1000-1999 | 认证和授权 |
| 2000-2999 | Sandbox |
| 3000-3999 | FileSystem |
| 4000-4099 | Process |
| 4100-4199 | PTY |
| 5000-5999 | Git |
| 6000-6999 | LSP |
| 7000-7099 | Snapshot |
| 7100-7199 | Volume |
| 9000-9999 | 系统 |

详见 [通用类型定义](docs/types/common.md#5-错误码)

---

## 许可证

Apache 2.0
