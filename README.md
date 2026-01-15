# Elevo Workspace

Elevo Workspace 是一个统一的沙盒工作空间服务，为 AI Agent 提供安全隔离的代码执行环境。

## 项目结构

```
elevo-workspace/
├── server/                 # Rust 服务端 (HTTP API + MCP)
├── agent/                  # Rust Agent (运行在容器内)
├── sdk-go/                 # Go SDK
├── sdk-python/             # Python SDK
├── sdk-typescript/         # TypeScript SDK
├── docker/                 # Docker 配置
├── images/                 # 容器镜像
├── proto/                  # gRPC Proto 定义
├── scripts/                # 构建和部署脚本
├── tests/                  # 测试
└── docs/                   # 文档
```

## 已实现功能

| 服务 | 描述 | 状态 |
|-----|------|------|
| **Sandbox** | 沙盒生命周期管理 (创建/删除/列表) | ✅ 已实现 |
| **FileSystem** | 文件系统操作 (读/写/列表/创建目录/删除) | ✅ 已实现 |
| **Process** | 进程执行 (同步/流式输出) | ✅ 已实现 |
| **PTY** | 伪终端交互 (WebSocket) | ✅ 已实现 |
| **MCP** | Model Context Protocol 支持 | ✅ 已实现 |
| Git | Git 版本控制 | ⏳ 规划中 |
| LSP | 语言服务协议 | ⏳ 规划中 |
| Snapshot | 快照管理 | ⏳ 规划中 |

## 快速开始

### 启动服务

```bash
# 开发环境
cd server && cargo run

# 生产环境 (Docker)
docker-compose -f docker/docker-compose.prod.yml up -d
```

### 环境变量

```bash
# 服务配置
WORKSPACE_HTTP_PORT=8080
WORKSPACE_GRPC_PORT=9090
WORKSPACE_DATABASE_URL=sqlite://data/workspace.db

# Docker 配置
WORKSPACE_DOCKER_HOST=unix:///var/run/docker.sock
WORKSPACE_DOCKER_NETWORK=workspace-network

# NFS 配置 (可选)
WORKSPACE_NFS_ENABLED=false
WORKSPACE_NFS_SERVER=nfs.example.com
WORKSPACE_NFS_PATH=/exports/workspace

# MCP 配置
WORKSPACE_MCP_ENABLED=true
WORKSPACE_MCP_PROFILE=developer  # executor / developer / full
```

### MCP Profile

MCP 支持三种 profile，适用于不同场景：

| Profile | 工具数量 | 适用场景 |
|---------|---------|---------|
| `executor` | 1 | 仅执行脚本，sandbox 由程序管理 |
| `developer` | 6 | 常规开发，包含文件和进程操作 |
| `full` | 14 | 完整功能，包含所有 sandbox 管理 |

**executor** (1 tool):
- `process_run` - 执行命令

**developer** (6 tools):
- `process_run` - 执行命令
- `file_read` - 读取文件
- `file_write` - 写入文件
- `file_list` - 列出目录
- `file_mkdir` - 创建目录
- `file_remove` - 删除文件/目录

**full** (14 tools):
- 所有 sandbox_* 操作
- 所有 process_* 操作
- 所有 file_* 操作

### MCP 使用

MCP 服务通过 stdio 方式运行，可以集成到支持 MCP 协议的 AI 客户端中。

**Claude Desktop 配置**

编辑 `~/.config/claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "elevo-workspace": {
      "command": "/path/to/workspace-server",
      "args": ["--mcp"],
      "env": {
        "WORKSPACE_MCP_PROFILE": "developer",
        "WORKSPACE_DOCKER_HOST": "unix:///var/run/docker.sock"
      }
    }
  }
}
```

**Docker 方式运行 MCP**

```json
{
  "mcpServers": {
    "elevo-workspace": {
      "command": "docker",
      "args": [
        "run", "-i", "--rm",
        "-v", "/var/run/docker.sock:/var/run/docker.sock",
        "-e", "WORKSPACE_MCP_PROFILE=developer",
        "your-registry/elevo-workspace-server:latest",
        "--mcp"
      ]
    }
  }
}
```

**可用工具 (developer profile)**

| 工具 | 描述 |
|-----|------|
| `process_run` | 在 sandbox 中执行命令 |
| `file_read` | 读取文件内容 |
| `file_write` | 写入文件内容 |
| `file_list` | 列出目录内容 |
| `file_mkdir` | 创建目录 |
| `file_remove` | 删除文件或目录 |

## SDK 使用

### Go SDK

```bash
go get git.easyops.local/elevo/elevo-workspace/sdk-go
```

```go
package main

import (
    "context"
    "fmt"
    "log"

    workspace "git.easyops.local/elevo/elevo-workspace/sdk-go"
)

func main() {
    client := workspace.NewClient("http://localhost:8080")
    ctx := context.Background()

    // 创建 sandbox
    sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
        Template: "workspace-test:latest",
    })
    if err != nil {
        log.Fatal(err)
    }
    defer client.Sandbox.Delete(ctx, sandbox.ID, true)

    // 执行命令
    result, err := client.Process.Run(ctx, sandbox.ID, "echo", &workspace.RunCommandOptions{
        Args: []string{"Hello", "World"},
    })
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Output: %s", result.Stdout)
}
```

### Python SDK

```bash
pip install -e sdk-python
```

```python
from workspace_sdk import WorkspaceClient

client = WorkspaceClient(base_url="http://localhost:8080")

# 创建 sandbox
sandbox = client.sandbox.create(template="workspace-test:latest")

try:
    # 执行命令
    result = client.process.run(sandbox.id, "echo", args=["Hello", "World"])
    print(result.stdout)
finally:
    client.sandbox.delete(sandbox.id, force=True)
```

### TypeScript SDK

```typescript
import { WorkspaceClient } from '@elevo/workspace-sdk'

const client = new WorkspaceClient({ baseUrl: 'http://localhost:8080' })

// 创建 sandbox
const sandbox = await client.sandbox.create({
  template: 'workspace-test:latest'
})

try {
  // 执行命令
  const result = await client.process.run(sandbox.id, 'echo', {
    args: ['Hello', 'World']
  })
  console.log(result.stdout)
} finally {
  await client.sandbox.delete(sandbox.id, { force: true })
}
```

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│                      Client SDK                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │     Go      │  │   Python    │  │  TypeScript │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└────────────────────────────┬────────────────────────────────┘
                             │
                    HTTP API / WebSocket / MCP
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                    Workspace Server (Rust)                   │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐           │
│  │ Sandbox │ │   FS    │ │ Process │ │   PTY   │           │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘           │
│  ┌─────────────────────────────────────────────┐           │
│  │              MCP Handler                     │           │
│  │  (executor / developer / full profiles)     │           │
│  └─────────────────────────────────────────────┘           │
└────────────────────────────┬────────────────────────────────┘
                             │
                        gRPC (内部)
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                    Docker Container                          │
│  ┌─────────────────────────────────────────────┐           │
│  │           Workspace Agent (Rust)             │           │
│  │  - 进程执行                                   │           │
│  │  - PTY 管理                                   │           │
│  │  - 文件操作                                   │           │
│  └─────────────────────────────────────────────┘           │
└─────────────────────────────────────────────────────────────┘
```

## 开发

### 构建

```bash
# 构建所有组件
./scripts/build.sh

# 仅构建 server
cd server && cargo build --release

# 仅构建 agent
cd agent && cargo build --release
```

### 测试

```bash
# 运行测试
./scripts/test.sh

# 集成测试
./scripts/run-integration-tests.sh
```

### 部署

```bash
# 构建并推送镜像
./scripts/build-and-push.sh

# 部署
./scripts/deploy.sh
```

## 许可证

Apache 2.0
