# Elevo Workspace 部署

## 目录结构

```
deploy/
├── config.env    # 配置文件
├── deploy.sh     # 部署脚本
└── README.md     # 本文件
```

## 快速开始

1. 修改配置文件 `config.env`
2. 执行 `./deploy.sh start`

## 配置说明

编辑 `config.env`:

```bash
# 端口
HTTP_PORT=8080          # HTTP API 端口
GRPC_PORT=9090          # gRPC 端口 (Agent 连接)
NFS_PORT=2049           # NFS 端口

# 存储
WORKSPACE_HOST_DIR=/var/lib/elevo-workspace/workspaces

# 镜像
SERVER_IMAGE=ghcr.io/openelevo/elevosandbox-server:latest
BASE_IMAGE=ghcr.io/openelevo/elevosandbox-base:latest

# MCP
MCP_MODE=http           # disabled 或 http
MCP_PATH=/mcp           # MCP 端点路径前缀

# FUSE 文件系统 API (可选)
FS_API_TOKEN=your-token # 设置后启用 gRPC FileSystemService
```

## 命令

```bash
./deploy.sh start     # 启动服务
./deploy.sh stop      # 停止服务
./deploy.sh restart   # 重启服务
./deploy.sh status    # 查看状态
./deploy.sh logs      # 查看日志
./deploy.sh cleanup   # 清理所有 Sandbox 容器
```

## MCP 端点

启动后提供三个 MCP 端点:

| 端点 | 工具数 | 说明 |
|-----|-------|------|
| `http://<host>:8080/mcp/executor` | 1 | 仅 process_run |
| `http://<host>:8080/mcp/developer` | 6 | process + file 操作 |
| `http://<host>:8080/mcp/full` | 14 | 全部操作 |

## FUSE 文件系统挂载

启用 `FS_API_TOKEN` 后，可以使用 FUSE 客户端将工作空间挂载到本地文件系统。

### 前置条件

- 安装 FUSE: `apt install fuse` (Linux) 或 `brew install macfuse` (macOS)
- 确保 `/dev/fuse` 存在且有访问权限

### Python SDK 示例

```python
from workspace_sdk import WorkspaceClient

client = WorkspaceClient("http://localhost:8080")

# 创建工作空间
workspace = client.workspaces.create()

# 挂载工作空间
with client.fuse.mount(workspace.id, token="your-fs-api-token") as mount:
    # 通过本地文件系统访问工作空间
    with open(f"{mount.path}/test.txt", "w") as f:
        f.write("Hello from FUSE!")

    # 读取文件
    with open(f"{mount.path}/test.txt", "r") as f:
        print(f.read())

# 退出 with 块后自动卸载
```

### 手动使用 workspace-fuse

```bash
# 下载 workspace-fuse 二进制
curl -L -o workspace-fuse \
  "http://localhost:8080/api/v1/downloads/workspace-fuse/linux/amd64"
chmod +x workspace-fuse

# 挂载工作空间
./workspace-fuse mount \
  --server http://localhost:9090 \
  --workspace <workspace-id> \
  --token <fs-api-token> \
  --target /mnt/workspace

# 卸载
fusermount -u /mnt/workspace
```

## Python MCP Client 示例

```python
from mcp import ClientSession
from mcp.client.streamable_http import streamablehttp_client

async def main():
    async with streamablehttp_client("http://localhost:8080/mcp/developer") as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()

            # 列出工具
            tools = await session.list_tools()
            print([t.name for t in tools.tools])

            # 执行命令
            result = await session.call_tool(
                "process_run",
                arguments={
                    "sandbox_id": "your-sandbox-id",
                    "command": "echo",
                    "args": ["Hello"]
                }
            )
            print(result)
```
