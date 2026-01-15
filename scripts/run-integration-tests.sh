#!/bin/bash
#
# Run integration tests
#
# Usage: ./scripts/run-integration-tests.sh
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

# Configuration
export WORKSPACE_HTTP_HOST=127.0.0.1
export WORKSPACE_HTTP_PORT=8080
export WORKSPACE_GRPC_HOST=127.0.0.1
export WORKSPACE_GRPC_PORT=9090
export WORKSPACE_DATABASE_URL=sqlite:data/test-workspace.db?mode=rwc
export RUST_LOG=info

SERVER_PID=""

cleanup() {
    echo "Cleaning up..."
    if [ -n "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
    # Clean up test database
    rm -f data/test-workspace.db*
}

trap cleanup EXIT

echo "=== Building server ==="
cargo build --release --package workspace-server

echo "=== Creating data directory ==="
mkdir -p data

echo "=== Starting server ==="
./target/release/workspace-server &
SERVER_PID=$!

# Wait for server to be ready
echo "Waiting for server to start..."
for i in {1..30}; do
    if curl -s http://127.0.0.1:8080/api/v1/health > /dev/null 2>&1; then
        echo "Server is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "ERROR: Server failed to start"
        exit 1
    fi
    sleep 1
done

echo ""
echo "=== Running integration tests ==="
cd tests/integration
cargo test -- --test-threads=1 --nocapture

echo ""
echo "=== All tests completed successfully ==="
