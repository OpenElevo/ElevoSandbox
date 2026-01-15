# Elevo Workspace 生产环境部署指南

## 概述

Elevo Workspace 是一个统一的 Sandbox/Workspace 服务，为 AI Agent 开发平台提供容器化的开发环境管理。

## 架构

```
┌──────────────────────────────────────────────────────────────────┐
│                          宿主机                                   │
│                                                                  │
│  ┌─────────────────────┐      ┌─────────────────────┐           │
│  │  workspace-server   │      │   sandbox 容器       │           │
│  │  (Docker 容器)       │◄────│   (workspace-base)  │           │
│  │                     │ gRPC │                     │           │
│  │  - HTTP API :8080   │      │  - workspace-agent  │           │
│  │  - gRPC    :9090    │      │  - Node.js/Python   │           │
│  └──────────┬──────────┘      └──────────┬──────────┘           │
│             │                            │                       │
│             │ Docker Socket              │ Volume Mount          │
│             ▼                            ▼                       │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │            /var/lib/elevo-workspace/workspaces              ││
│  │                     (宿主机共享目录)                          ││
│  └─────────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────┘
```

## 镜像列表

| 镜像 | 说明 |
|------|------|
| `docker.easyops.local/elevo/workspace-server:latest` | 服务端镜像 |
| `docker.easyops.local/elevo/workspace-base:latest` | Sandbox 基础镜像 |

## 构建镜像

如果需要从源码构建镜像，请按以下步骤操作：

### 前置要求

- Docker（用于编译和构建镜像）
- 访问 `docker.easyops.local` 镜像仓库

### 构建步骤

```bash
# 克隆仓库
git clone https://github.com/elevo/elevo-workspace.git
cd elevo-workspace

# 使用构建脚本（推荐）
# 脚本会自动：
# 1. 使用 Docker 容器编译 Rust 二进制（确保 glibc 兼容性）
# 2. 构建 Docker 镜像
# 3. 推送到镜像仓库
./scripts/build-and-push.sh

# 或指定版本号
./scripts/build-and-push.sh v1.0.0
```

**注意**：构建脚本使用 `docker.easyops.local/ci/rust-builder:1.92.0-centos7` 镜像进行编译，
确保生成的二进制文件与生产环境的 glibc 版本兼容。

## 快速部署

### 1. 准备工作

```bash
# 创建工作目录
sudo mkdir -p /var/lib/elevo-workspace/workspaces
sudo chmod 755 /var/lib/elevo-workspace

# 拉取镜像
docker pull docker.easyops.local/elevo/workspace-server:latest
docker pull docker.easyops.local/elevo/workspace-base:latest
```

### 2. 创建 docker-compose.yml

```yaml
version: '3.8'

services:
  workspace-server:
    image: docker.easyops.local/elevo/workspace-server:latest
    container_name: elevo-workspace-server
    restart: unless-stopped
    ports:
      - "8080:8080"  # HTTP API
      - "9090:9090"  # gRPC
    environment:
      - RUST_LOG=info,workspace_server=info
      - WORKSPACE_DATABASE_URL=sqlite:/data/workspace.db?mode=rwc
      - WORKSPACE_DOCKER_SOCKET=/var/run/docker.sock
      - WORKSPACE_WORKSPACE_DIR=/data/workspaces
      - WORKSPACE_WORKSPACE_HOST_DIR=/var/lib/elevo-workspace/workspaces
      - WORKSPACE_BASE_IMAGE=docker.easyops.local/elevo/workspace-base:latest
      - WORKSPACE_AGENT_SERVER_ADDR=http://host.docker.internal:9090
      - WORKSPACE_SANDBOX_EXTRA_HOSTS=host.docker.internal:host-gateway
      - WORKSPACE_AGENT_TIMEOUT=60
      - WORKSPACE_MAX_IDLE_TIME=7200
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - elevo-workspace-data:/data
      - /var/lib/elevo-workspace/workspaces:/data/workspaces
    extra_hosts:
      - "host.docker.internal:host-gateway"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/api/v1/health"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  elevo-workspace-data:
```

### 3. 启动服务

```bash
docker-compose up -d
```

### 4. 验证服务

```bash
# 检查健康状态
curl http://localhost:8080/api/v1/health

# 查看日志
docker logs -f elevo-workspace-server
```

## API 使用示例

### 创建 Sandbox

```bash
curl -X POST http://localhost:8080/api/v1/sandboxes \
  -H "Content-Type: application/json" \
  -d '{
    "template": "docker.easyops.local/elevo/workspace-base:latest",
    "name": "my-sandbox"
  }'
```

### 列出 Sandbox

```bash
curl http://localhost:8080/api/v1/sandboxes
```

### 执行命令

```bash
curl -X POST http://localhost:8080/api/v1/sandboxes/{sandbox_id}/process/run \
  -H "Content-Type: application/json" \
  -d '{
    "command": "python",
    "args": ["--version"]
  }'
```

### 删除 Sandbox

```bash
curl -X DELETE http://localhost:8080/api/v1/sandboxes/{sandbox_id}?force=true
```

## 配置说明

### 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `WORKSPACE_HTTP_PORT` | 8080 | HTTP API 端口 |
| `WORKSPACE_GRPC_PORT` | 9090 | gRPC 端口 |
| `WORKSPACE_DATABASE_URL` | sqlite:data/workspace.db | 数据库连接 |
| `WORKSPACE_WORKSPACE_DIR` | /var/lib/workspace | 容器内工作目录 |
| `WORKSPACE_WORKSPACE_HOST_DIR` | - | 宿主机工作目录（必须配置） |
| `WORKSPACE_BASE_IMAGE` | workspace-base:latest | 默认 Sandbox 镜像 |
| `WORKSPACE_AGENT_SERVER_ADDR` | http://172.17.0.1:9090 | Agent 连接地址 |
| `WORKSPACE_SANDBOX_EXTRA_HOSTS` | - | Sandbox 额外 hosts |
| `WORKSPACE_AGENT_TIMEOUT` | 30 | Agent 连接超时（秒） |
| `WORKSPACE_MAX_IDLE_TIME` | 3600 | Sandbox 最大空闲时间（秒） |

### 关键配置说明

#### WORKSPACE_WORKSPACE_HOST_DIR

这是最重要的配置项。当 Server 在 Docker 中运行时：
- Server 容器挂载 `HOST_DIR` 到 `/data/workspaces`
- 创建 Sandbox 时，需要从**宿主机路径**挂载卷到 Sandbox 容器
- 因此必须配置 `WORKSPACE_HOST_DIR` 为宿主机上的实际路径

#### WORKSPACE_AGENT_SERVER_ADDR

Sandbox 容器内的 Agent 需要连接回 Server。推荐配置：
- Linux: `http://host.docker.internal:9090`（需配合 extra_hosts）
- 或使用 Docker 网桥 IP: `http://172.17.0.1:9090`

## 运维操作

### 查看日志

```bash
# Server 日志
docker logs -f elevo-workspace-server

# Sandbox 日志
docker logs -f workspace-{sandbox_id前8位}
```

### 清理资源

```bash
# 停止服务
docker-compose down

# 清理所有 Sandbox 容器
docker ps -a --filter "label=workspace.sandbox.id" -q | xargs -r docker rm -f

# 清理数据（谨慎操作）
sudo rm -rf /var/lib/elevo-workspace/workspaces/*
docker volume rm elevo-workspace-data
```

### 升级服务

```bash
# 拉取新镜像
docker pull docker.easyops.local/elevo/workspace-server:latest
docker pull docker.easyops.local/elevo/workspace-base:latest

# 重启服务
docker-compose down
docker-compose up -d
```

## 故障排查

### Agent 连接失败

1. 检查 `WORKSPACE_AGENT_SERVER_ADDR` 配置
2. 确保 `extra_hosts` 配置正确
3. 检查防火墙是否开放 9090 端口
4. 查看 Sandbox 容器日志

### Sandbox 创建失败

1. 检查 Docker socket 权限
2. 确保基础镜像已拉取
3. 检查 `WORKSPACE_WORKSPACE_HOST_DIR` 目录权限
4. 查看 Server 日志

### 文件操作失败

1. 检查 `WORKSPACE_WORKSPACE_HOST_DIR` 配置
2. 确保目录挂载正确
3. 检查目录权限（需要 Server 和 Sandbox 都能读写）

## 监控

### 健康检查端点

- `GET /api/v1/health` - 服务健康状态

### Prometheus 指标

（待实现）

## 安全建议

1. **Docker Socket** - 生产环境建议使用 Docker proxy 或 socket 权限控制
2. **网络隔离** - 建议使用专用 Docker 网络
3. **资源限制** - 配置 Sandbox 的 CPU/内存限制
4. **日志审计** - 保留 API 调用日志

## 联系方式

如有问题，请联系 Elevo 团队。
