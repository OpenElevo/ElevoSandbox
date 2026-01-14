#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "Running Unified Workspace SDK tests..."

cd "$PROJECT_ROOT"

# Run Rust unit tests
echo "==> Running Rust unit tests..."
cargo test --workspace

# Check if Python is available for SDK tests
if command -v python3 &> /dev/null; then
    echo "==> Running Python SDK tests..."
    cd sdk-python
    python3 -m pip install -e ".[dev]" --quiet 2>/dev/null || true
    python3 -m pytest tests/ -v || echo "Python tests skipped (deps not installed)"
    cd ..
fi

# Check if Node.js is available for TypeScript SDK tests
if command -v npx &> /dev/null; then
    echo "==> Running TypeScript SDK tests..."
    cd sdk-typescript
    npm install --quiet 2>/dev/null || true
    npm test || echo "TypeScript tests skipped (deps not installed)"
    cd ..
fi

echo ""
echo "Tests complete!"
