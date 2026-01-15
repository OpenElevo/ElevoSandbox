#!/bin/bash
# Elevo Workspace - 生产环境部署脚本
# Usage: ./deploy.sh [start|stop|restart|status|logs|cleanup]

set -e

# 获取脚本所在目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_FILE="${SCRIPT_DIR}/config.env"

# 加载配置文件
if [ -f "$CONFIG_FILE" ]; then
    source "$CONFIG_FILE"
else
    echo "错误: 配置文件不存在: $CONFIG_FILE"
    exit 1
fi

# 默认值
CONTAINER_NAME="${CONTAINER_NAME:-elevo-workspace-server}"
HTTP_PORT="${HTTP_PORT:-8080}"
GRPC_PORT="${GRPC_PORT:-9090}"
WORKSPACE_HOST_DIR="${WORKSPACE_HOST_DIR:-/var/lib/elevo-workspace/workspaces}"
SERVER_IMAGE="${SERVER_IMAGE:-docker.easyops.local/elevo/workspace-server:latest}"
BASE_IMAGE="${BASE_IMAGE:-docker.easyops.local/elevo/workspace-base:latest}"
MCP_MODE="${MCP_MODE:-http}"
MCP_PATH="${MCP_PATH:-/mcp}"
AGENT_TIMEOUT="${AGENT_TIMEOUT:-60}"
MAX_IDLE_TIME="${MAX_IDLE_TIME:-7200}"
LOG_LEVEL="${LOG_LEVEL:-info}"

# 颜色
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# 检查 Docker
check_docker() {
    if ! command -v docker &> /dev/null; then
        log_error "Docker 未安装"
        exit 1
    fi
}

# 拉取镜像
pull_images() {
    log_info "拉取镜像..."
    docker pull "$SERVER_IMAGE"
    docker pull "$BASE_IMAGE"
    log_success "镜像拉取完成"
}

# 准备目录
prepare_dirs() {
    log_info "准备目录: $WORKSPACE_HOST_DIR"
    sudo mkdir -p "$WORKSPACE_HOST_DIR"
    sudo chmod 755 "$WORKSPACE_HOST_DIR"
}

# 启动服务
start() {
    check_docker

    # 检查是否已运行
    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log_warn "服务已在运行中"
        return 0
    fi

    # 删除已停止的容器
    if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log_info "删除已停止的容器..."
        docker rm "$CONTAINER_NAME"
    fi

    prepare_dirs
    pull_images

    log_info "启动服务..."

    docker run -d \
        --name "$CONTAINER_NAME" \
        --restart unless-stopped \
        -p "${HTTP_PORT}:8080" \
        -p "${GRPC_PORT}:9090" \
        -v /var/run/docker.sock:/var/run/docker.sock:ro \
        -v "${WORKSPACE_HOST_DIR}:/data/workspaces" \
        -e RUST_LOG="${LOG_LEVEL},workspace_server=${LOG_LEVEL}" \
        -e WORKSPACE_DATABASE_URL=sqlite:/data/workspace.db?mode=rwc \
        -e WORKSPACE_WORKSPACE_DIR=/data/workspaces \
        -e WORKSPACE_WORKSPACE_HOST_DIR="$WORKSPACE_HOST_DIR" \
        -e WORKSPACE_BASE_IMAGE="$BASE_IMAGE" \
        -e WORKSPACE_AGENT_SERVER_ADDR="http://172.17.0.1:${GRPC_PORT}" \
        -e WORKSPACE_SANDBOX_EXTRA_HOSTS=host.docker.internal:host-gateway \
        -e WORKSPACE_AGENT_TIMEOUT="$AGENT_TIMEOUT" \
        -e WORKSPACE_MAX_IDLE_TIME="$MAX_IDLE_TIME" \
        -e WORKSPACE_MCP_MODE="$MCP_MODE" \
        -e WORKSPACE_MCP_PATH="$MCP_PATH" \
        --add-host=host.docker.internal:host-gateway \
        "$SERVER_IMAGE"

    log_info "等待服务启动..."
    sleep 3

    # 健康检查
    if curl -sf "http://localhost:${HTTP_PORT}/api/v1/health" > /dev/null 2>&1; then
        log_success "服务启动成功!"
        echo ""
        echo "=========================================="
        echo "  HTTP API: http://localhost:${HTTP_PORT}"
        echo "  gRPC:     localhost:${GRPC_PORT}"
        echo "  Health:   http://localhost:${HTTP_PORT}/api/v1/health"
        if [ "$MCP_MODE" = "http" ]; then
            echo ""
            echo "  MCP 端点:"
            echo "    http://localhost:${HTTP_PORT}${MCP_PATH}/executor   (1 tool)"
            echo "    http://localhost:${HTTP_PORT}${MCP_PATH}/developer  (6 tools)"
            echo "    http://localhost:${HTTP_PORT}${MCP_PATH}/full       (14 tools)"
        fi
        echo "=========================================="
        echo ""
    else
        log_warn "服务已启动但健康检查失败，请查看日志: $0 logs"
    fi
}

# 停止服务
stop() {
    check_docker

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log_info "停止服务..."
        docker stop "$CONTAINER_NAME"
        docker rm "$CONTAINER_NAME"
        log_success "服务已停止"
    else
        log_warn "服务未运行"
    fi
}

# 重启服务
restart() {
    stop
    start
}

# 查看状态
status() {
    check_docker

    echo ""
    echo "=== Elevo Workspace 状态 ==="
    echo ""

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log_success "服务: 运行中"
        docker ps --filter "name=${CONTAINER_NAME}" --format "table {{.Status}}\t{{.Ports}}"
        echo ""

        if curl -sf "http://localhost:${HTTP_PORT}/api/v1/health" 2>/dev/null; then
            echo ""
            log_success "健康检查: OK"
        else
            log_warn "健康检查: 失败"
        fi
    else
        log_warn "服务: 未运行"
    fi

    echo ""
    echo "=== Sandbox 容器 ==="
    SANDBOX_COUNT=$(docker ps --filter "label=workspace.sandbox.id" -q | wc -l)
    echo "运行中的 Sandbox: $SANDBOX_COUNT"

    if [ "$SANDBOX_COUNT" -gt 0 ]; then
        docker ps --filter "label=workspace.sandbox.id" --format "table {{.Names}}\t{{.Status}}\t{{.CreatedAt}}"
    fi
    echo ""
}

# 查看日志
logs() {
    check_docker

    if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        docker logs -f "$CONTAINER_NAME"
    else
        log_error "容器不存在"
        exit 1
    fi
}

# 清理 Sandbox 容器
cleanup() {
    check_docker

    log_info "清理 Sandbox 容器..."
    SANDBOX_IDS=$(docker ps -a --filter "label=workspace.sandbox.id" -q)

    if [ -n "$SANDBOX_IDS" ]; then
        echo "$SANDBOX_IDS" | xargs docker rm -f
        log_success "已清理 Sandbox 容器"
    else
        log_info "没有需要清理的 Sandbox 容器"
    fi
}

# 显示帮助
usage() {
    echo "Elevo Workspace - 生产环境部署脚本"
    echo ""
    echo "用法: $0 <命令>"
    echo ""
    echo "命令:"
    echo "  start     启动服务"
    echo "  stop      停止服务"
    echo "  restart   重启服务"
    echo "  status    查看状态"
    echo "  logs      查看日志"
    echo "  cleanup   清理所有 Sandbox 容器"
    echo ""
    echo "配置文件: ${CONFIG_FILE}"
    echo ""
}

# 主入口
case "${1:-}" in
    start)   start ;;
    stop)    stop ;;
    restart) restart ;;
    status)  status ;;
    logs)    logs ;;
    cleanup) cleanup ;;
    *)       usage; exit 1 ;;
esac
