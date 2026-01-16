#!/bin/bash
# Elevo Workspace - Production Deployment Script
# Usage: ./deploy.sh [start|stop|restart|status|logs]

set -e

# Configuration
CONTAINER_NAME="elevo-workspace-server"
SERVER_IMAGE="${SERVER_IMAGE:-ghcr.io/openelevo/elevosandbox-server:latest}"
BASE_IMAGE="${BASE_IMAGE:-ghcr.io/openelevo/elevosandbox-base:latest}"
WORKSPACE_HOST_DIR="${WORKSPACE_HOST_DIR:-/var/lib/elevo-workspace/workspaces}"
HTTP_PORT="${HTTP_PORT:-8080}"
GRPC_PORT="${GRPC_PORT:-9090}"

# MCP Configuration
MCP_MODE="${MCP_MODE:-http}"           # disabled, stdio, http
MCP_PATH="${MCP_PATH:-/mcp}"           # HTTP endpoint path prefix

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Check if Docker is available
check_docker() {
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed"
        exit 1
    fi
}

# Pull latest images
pull_images() {
    log_info "Pulling latest images..."
    docker pull "$SERVER_IMAGE"
    docker pull "$BASE_IMAGE"
    log_success "Images pulled"
}

# Prepare directories
prepare_dirs() {
    log_info "Preparing directories..."
    sudo mkdir -p "$WORKSPACE_HOST_DIR"
    sudo chmod 755 "$WORKSPACE_HOST_DIR"
    log_success "Directories ready: $WORKSPACE_HOST_DIR"
}

# Start the service
start() {
    check_docker

    # Check if already running
    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log_warn "Container $CONTAINER_NAME is already running"
        return 0
    fi

    # Remove stopped container if exists
    if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log_info "Removing stopped container..."
        docker rm "$CONTAINER_NAME"
    fi

    prepare_dirs
    pull_images

    log_info "Starting $CONTAINER_NAME..."

    docker run -d \
        --name "$CONTAINER_NAME" \
        --restart unless-stopped \
        -p "${HTTP_PORT}:8080" \
        -p "${GRPC_PORT}:9090" \
        -v /var/run/docker.sock:/var/run/docker.sock:ro \
        -v "${WORKSPACE_HOST_DIR}:/data/workspaces" \
        -e RUST_LOG=info,workspace_server=info \
        -e WORKSPACE_DATABASE_URL=sqlite:/data/workspace.db?mode=rwc \
        -e WORKSPACE_WORKSPACE_DIR=/data/workspaces \
        -e WORKSPACE_WORKSPACE_HOST_DIR="$WORKSPACE_HOST_DIR" \
        -e WORKSPACE_BASE_IMAGE="$BASE_IMAGE" \
        -e WORKSPACE_AGENT_SERVER_ADDR="http://172.17.0.1:${GRPC_PORT}" \
        -e WORKSPACE_SANDBOX_EXTRA_HOSTS=host.docker.internal:host-gateway \
        -e WORKSPACE_AGENT_TIMEOUT=60 \
        -e WORKSPACE_MAX_IDLE_TIME=7200 \
        -e WORKSPACE_MCP_MODE="$MCP_MODE" \
        -e WORKSPACE_MCP_PATH="$MCP_PATH" \
        --add-host=host.docker.internal:host-gateway \
        "$SERVER_IMAGE"

    # Wait for startup
    log_info "Waiting for service to start..."
    sleep 3

    # Health check
    if curl -sf "http://localhost:${HTTP_PORT}/api/v1/health" > /dev/null 2>&1; then
        log_success "Service started successfully!"
        echo ""
        echo "  HTTP API: http://localhost:${HTTP_PORT}"
        echo "  gRPC:     localhost:${GRPC_PORT}"
        echo "  Health:   http://localhost:${HTTP_PORT}/api/v1/health"
        if [ "$MCP_MODE" = "http" ]; then
            echo ""
            echo "  MCP Endpoints:"
            echo "    - http://localhost:${HTTP_PORT}${MCP_PATH}/executor   (1 tool)"
            echo "    - http://localhost:${HTTP_PORT}${MCP_PATH}/developer  (6 tools)"
            echo "    - http://localhost:${HTTP_PORT}${MCP_PATH}/full       (14 tools)"
        fi
        echo ""
    else
        log_warn "Service started but health check failed. Check logs with: $0 logs"
    fi
}

# Stop the service
stop() {
    check_docker

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log_info "Stopping $CONTAINER_NAME..."
        docker stop "$CONTAINER_NAME"
        docker rm "$CONTAINER_NAME"
        log_success "Service stopped"
    else
        log_warn "Container $CONTAINER_NAME is not running"
    fi
}

# Restart the service
restart() {
    stop
    start
}

# Show status
status() {
    check_docker

    echo ""
    echo "=== Elevo Workspace Status ==="
    echo ""

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        log_success "Container: Running"
        docker ps --filter "name=${CONTAINER_NAME}" --format "table {{.Status}}\t{{.Ports}}"
        echo ""

        # Health check
        if curl -sf "http://localhost:${HTTP_PORT}/api/v1/health" 2>/dev/null; then
            echo ""
            log_success "Health: OK"
        else
            log_warn "Health: Check failed"
        fi
    else
        log_warn "Container: Not running"
    fi

    # Show sandbox containers
    echo ""
    echo "=== Sandbox Containers ==="
    SANDBOX_COUNT=$(docker ps --filter "label=workspace.sandbox.id" -q | wc -l)
    echo "Running sandboxes: $SANDBOX_COUNT"

    if [ "$SANDBOX_COUNT" -gt 0 ]; then
        docker ps --filter "label=workspace.sandbox.id" --format "table {{.Names}}\t{{.Status}}\t{{.CreatedAt}}"
    fi
    echo ""
}

# Show logs
logs() {
    check_docker

    if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        docker logs -f "$CONTAINER_NAME"
    else
        log_error "Container $CONTAINER_NAME does not exist"
        exit 1
    fi
}

# Cleanup all sandbox containers
cleanup() {
    check_docker

    log_info "Cleaning up sandbox containers..."
    SANDBOX_IDS=$(docker ps -a --filter "label=workspace.sandbox.id" -q)

    if [ -n "$SANDBOX_IDS" ]; then
        echo "$SANDBOX_IDS" | xargs docker rm -f
        log_success "Cleaned up sandbox containers"
    else
        log_info "No sandbox containers to clean"
    fi
}

# Show usage
usage() {
    echo "Elevo Workspace - Production Deployment Script"
    echo ""
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  start     Start the workspace server"
    echo "  stop      Stop the workspace server"
    echo "  restart   Restart the workspace server"
    echo "  status    Show service status"
    echo "  logs      Follow service logs"
    echo "  cleanup   Remove all sandbox containers"
    echo ""
    echo "Environment variables:"
    echo "  WORKSPACE_HOST_DIR  Host directory for workspaces (default: /var/lib/elevo-workspace/workspaces)"
    echo "  HTTP_PORT           HTTP API port (default: 8080)"
    echo "  GRPC_PORT           gRPC port (default: 9090)"
    echo "  MCP_MODE            MCP mode: disabled, http (default: http)"
    echo "  MCP_PATH            MCP HTTP endpoint path prefix (default: /mcp)"
    echo ""
    echo "Examples:"
    echo "  # Start with default settings (MCP enabled)"
    echo "  $0 start"
    echo ""
    echo "  # Start with custom ports and workspace directory"
    echo "  HTTP_PORT=9000 GRPC_PORT=9001 WORKSPACE_HOST_DIR=/data/workspace $0 start"
    echo ""
    echo "  # Start without MCP"
    echo "  MCP_MODE=disabled $0 start"
    echo ""
}

# Main
case "${1:-}" in
    start)
        start
        ;;
    stop)
        stop
        ;;
    restart)
        restart
        ;;
    status)
        status
        ;;
    logs)
        logs
        ;;
    cleanup)
        cleanup
        ;;
    *)
        usage
        exit 1
        ;;
esac
