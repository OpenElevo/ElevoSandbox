#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "Building Elevo Workspace..."

cd "$PROJECT_ROOT"

# Build Rust binaries
echo "==> Building Rust binaries..."
cargo build --release

# Build Docker images
echo "==> Building Docker images..."

# Build server image
echo "  -> workspace-server"
docker build -f docker/Dockerfile.server -t workspace-server:latest .

# Build base image for sandboxes
echo "  -> workspace-base"
docker build -f images/workspace-base/Dockerfile -t workspace-base:latest images/workspace-base/

echo "Build complete!"
echo ""
echo "Images built:"
echo "  - workspace-server:latest"
echo "  - workspace-base:latest"
