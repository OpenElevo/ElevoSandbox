#!/bin/bash
# Integration and E2E test runner for Elevo Workspace
#
# Usage: ./run-tests.sh [options]
#
# Options:
#   --integration    Run only integration tests
#   --e2e           Run only E2E tests
#   --cleanup       Only cleanup (no tests)
#   --no-server     Don't start the server (use existing)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Default configuration
export WORKSPACE_WORKSPACE_DIR="${WORKSPACE_WORKSPACE_DIR:-/tmp/workspace-sdk-test/workspace}"
export WORKSPACE_DATABASE_URL="${WORKSPACE_DATABASE_URL:-sqlite:/tmp/workspace-sdk-test/data/test.db?mode=rwc}"
export WORKSPACE_BASE_IMAGE="${WORKSPACE_BASE_IMAGE:-docker.easyops.local/ci/rust-builder:1.85.0-centos7}"
export WORKSPACE_AGENT_TIMEOUT="${WORKSPACE_AGENT_TIMEOUT:-5}"
export WORKSPACE_TEST_URL="${WORKSPACE_TEST_URL:-http://127.0.0.1:8080}"
export WORKSPACE_TEST_TIMEOUT="${WORKSPACE_TEST_TIMEOUT:-60}"

RUN_INTEGRATION=true
RUN_E2E=true
START_SERVER=true
SERVER_PID=""

cleanup() {
    echo "Cleaning up..."

    # Kill server if we started it
    if [ -n "$SERVER_PID" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
    fi

    # Kill any workspace-server processes
    pkill -9 -f workspace-server 2>/dev/null || true

    # Remove test containers
    docker ps -a --filter "label=workspace.sandbox.id" -q 2>/dev/null | xargs -r docker rm -f 2>/dev/null || true

    echo "Cleanup complete"
}

trap cleanup EXIT

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --integration)
            RUN_E2E=false
            shift
            ;;
        --e2e)
            RUN_INTEGRATION=false
            shift
            ;;
        --cleanup)
            cleanup
            exit 0
            ;;
        --no-server)
            START_SERVER=false
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Setup test directories
echo "Setting up test environment..."
rm -rf /tmp/workspace-sdk-test
mkdir -p "$WORKSPACE_WORKSPACE_DIR"
mkdir -p "$(dirname "$WORKSPACE_DATABASE_URL" | sed 's/sqlite://')"

# Start server if needed
if [ "$START_SERVER" = true ]; then
    echo "Building and starting server..."
    cd "$PROJECT_ROOT"
    cargo build --package workspace-server 2>&1 | tail -5

    # Start server in background
    cargo run --package workspace-server 2>&1 &
    SERVER_PID=$!
    echo "Server started with PID: $SERVER_PID"

    # Wait for server to be ready
    echo "Waiting for server to be ready..."
    for i in {1..30}; do
        if curl -s "$WORKSPACE_TEST_URL/api/v1/health" > /dev/null 2>&1; then
            echo "Server is ready!"
            break
        fi
        if [ $i -eq 30 ]; then
            echo "Server failed to start"
            exit 1
        fi
        sleep 1
    done
fi

# Run integration tests
if [ "$RUN_INTEGRATION" = true ]; then
    echo ""
    echo "============================================"
    echo "Running Integration Tests (Rust)"
    echo "============================================"
    cd "$PROJECT_ROOT"
    cargo test --package integration-tests -- --test-threads=1
fi

# Run E2E tests
if [ "$RUN_E2E" = true ]; then
    echo ""
    echo "============================================"
    echo "Running E2E Tests (Python)"
    echo "============================================"
    python3 "$PROJECT_ROOT/tests/e2e-python/test_sandbox.py"
fi

echo ""
echo "============================================"
echo "All tests completed successfully!"
echo "============================================"
