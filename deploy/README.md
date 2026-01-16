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

# 存储
WORKSPACE_HOST_DIR=/var/lib/elevo-workspace/workspaces

# 镜像
SERVER_IMAGE=ghcr.io/openelevo/elevosandbox-server:latest
BASE_IMAGE=ghcr.io/openelevo/elevosandbox-base:latest

# MCP
MCP_MODE=http           # disabled 或 http
MCP_PATH=/mcp           # MCP 端点路径前缀
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
