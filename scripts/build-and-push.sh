#!/bin/bash
# Build and push ElevoSandbox Docker images to registry
# Usage: ./build-and-push.sh [version]
#
# Prerequisites:
#   - Docker installed and running
#   - Logged in to container registry (run: docker login ghcr.io)

set -e

# Configuration - can be overridden via environment variables
REGISTRY="${REGISTRY:-ghcr.io/openelevo}"
VERSION="${1:-latest}"
CACHE_DIR="${CARGO_CACHE_DIR:-/data/cache}"

# Image names
SERVER_IMAGE="${REGISTRY}/elevosandbox-server"
BASE_IMAGE="${REGISTRY}/elevosandbox-base"

# Rust builder image for compilation (using official Rust image)
RUST_IMAGE="${RUST_IMAGE:-rust:1.85}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo ""
echo "=============================================="
echo "  ElevoSandbox - Build and Push Images"
echo "=============================================="
echo ""
echo "Registry: ${REGISTRY}"
echo "Version:  ${VERSION}"
echo ""

# Check Docker
if ! command -v docker &> /dev/null; then
    log_error "Docker is not installed or not in PATH"
    exit 1
fi

# Create cache directories
mkdir -p "${CACHE_DIR}/cargo/git" 2>/dev/null || true
mkdir -p "${CACHE_DIR}/cargo/registry" 2>/dev/null || true
mkdir -p "${CACHE_DIR}/target" 2>/dev/null || true

# Build Rust binaries using Docker for glibc compatibility
log_info "Building Rust binaries (release mode) in Docker..."
log_info "Using Rust image: ${RUST_IMAGE}"

docker run --rm \
    -v "$PROJECT_ROOT":/workspace \
    -v "${CACHE_DIR}/cargo/git":/usr/local/cargo/git \
    -v "${CACHE_DIR}/cargo/registry":/usr/local/cargo/registry \
    -v "${CACHE_DIR}/target":/workspace/target \
    -w /workspace \
    "$RUST_IMAGE" \
    cargo build --release --package workspace-server --package workspace-agent

if [ $? -ne 0 ]; then
    log_error "Failed to build Rust binaries"
    exit 1
fi
log_success "Built workspace-server and workspace-agent binaries"

# Verify binaries exist (they are in the cache directory)
if [ ! -f "${CACHE_DIR}/target/release/workspace-server" ]; then
    log_error "workspace-server binary not found"
    exit 1
fi

if [ ! -f "${CACHE_DIR}/target/release/workspace-agent" ]; then
    log_error "workspace-agent binary not found"
    exit 1
fi

# Copy binaries to local target for Docker build
log_info "Copying binaries to project target directory..."
mkdir -p target/release
# Remove old binaries if they exist (may be owned by root from previous Docker builds)
rm -f target/release/workspace-server target/release/workspace-agent 2>/dev/null || sudo rm -f target/release/workspace-server target/release/workspace-agent
cp "${CACHE_DIR}/target/release/workspace-server" target/release/
cp "${CACHE_DIR}/target/release/workspace-agent" target/release/

# Check registry login
log_info "Checking registry authentication..."
if ! docker pull "${REGISTRY}/elevosandbox-server:latest" >/dev/null 2>&1&& \
   ! docker manifest inspect "${REGISTRY}/elevosandbox-server:latest" >/dev/null 2>&1; then
    log_warn "May not be logged in to registry. If push fails, run: docker login ${REGISTRY%%/*}"
fi

# Build workspace-server
log_info "Building workspace-server Docker image..."
docker build \
    -t "${SERVER_IMAGE}:${VERSION}" \
    -t "${SERVER_IMAGE}:latest" \
    -f docker/Dockerfile.server \
    .

if [ $? -ne 0 ]; then
    log_error "Failed to build workspace-server"
    exit 1
fi
log_success "Built ${SERVER_IMAGE}:${VERSION}"

# Build workspace-base
log_info "Building workspace-base Docker image..."
docker build \
    -t "${BASE_IMAGE}:${VERSION}" \
    -t "${BASE_IMAGE}:latest" \
    -f images/workspace-base/Dockerfile \
    .

if [ $? -ne 0 ]; then
    log_error "Failed to build workspace-base"
    exit 1
fi
log_success "Built ${BASE_IMAGE}:${VERSION}"

# Push images
log_info "Pushing workspace-server..."
docker push "${SERVER_IMAGE}:${VERSION}"
docker push "${SERVER_IMAGE}:latest"
log_success "Pushed ${SERVER_IMAGE}"

log_info "Pushing workspace-base..."
docker push "${BASE_IMAGE}:${VERSION}"
docker push "${BASE_IMAGE}:latest"
log_success "Pushed ${BASE_IMAGE}"

echo ""
echo "=============================================="
log_success "All images built and pushed successfully!"
echo "=============================================="
echo ""
echo "Images:"
echo "  - ${SERVER_IMAGE}:${VERSION}"
echo "  - ${SERVER_IMAGE}:latest"
echo "  - ${BASE_IMAGE}:${VERSION}"
echo "  - ${BASE_IMAGE}:latest"
echo ""
echo "To deploy, run:"
echo "  docker-compose -f docker/docker-compose.prod.yml pull"
echo "  docker-compose -f docker/docker-compose.prod.yml up -d"
echo ""
