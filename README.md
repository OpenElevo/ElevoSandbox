# ElevoSandbox

[![CI](https://github.com/OpenElevo/ElevoSandbox/actions/workflows/ci.yml/badge.svg)](https://github.com/OpenElevo/ElevoSandbox/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

[中文文档](README_zh.md)

ElevoSandbox is a unified sandbox workspace service that provides secure, isolated code execution environments for AI Agents.

## Features

- **Container Isolation**: Docker-based secure sandbox environments
- **Multi-SDK Support**: Go, Python, and TypeScript SDKs
- **MCP Protocol**: Native support for Model Context Protocol
- **NFS Sharing**: Mount workspace files locally via NFS
- **PTY Terminal**: Real-time interactive terminal via WebSocket

## Project Structure

```
ElevoSandbox/
├── server/                 # Rust server (HTTP API + MCP)
├── agent/                  # Rust Agent (runs inside containers)
├── sdk-go/                 # Go SDK
├── sdk-python/             # Python SDK
├── sdk-typescript/         # TypeScript SDK
├── docker/                 # Docker configuration
├── images/                 # Container images
├── proto/                  # gRPC Proto definitions
├── scripts/                # Build and deployment scripts
├── tests/                  # Tests
└── docs/                   # Documentation
```

## Implemented Features

| Service | Description | Status |
|---------|-------------|--------|
| **Workspace** | Workspace management (persistent storage, 1:N relationship with Sandbox) | ✅ Implemented |
| **Sandbox** | Sandbox lifecycle management (create/delete/list, bound to Workspace) | ✅ Implemented |
| **FileSystem** | File system operations (read/write/list/mkdir/delete via Workspace API) | ✅ Implemented |
| **Process** | Process execution (sync/streaming output) | ✅ Implemented |
| **PTY** | Pseudo-terminal interaction (WebSocket) | ✅ Implemented |
| **MCP** | Model Context Protocol support | ✅ Implemented |
| **NFS** | Network file system sharing (Workspace level) | ✅ Implemented |
| Git | Git version control | ⏳ Planned |
| LSP | Language Server Protocol | ⏳ Planned |
| Snapshot | Snapshot management | ⏳ Planned |

## Core Concepts

### Workspace and Sandbox Relationship

- **Workspace**: Persistent working directory with independent lifecycle management; NFS mounts belong here
- **Sandbox**: Temporary execution environment that must be bound to a Workspace when created
- **Relationship**: 1:N (one Workspace can be used by multiple Sandboxes simultaneously)

```
┌─────────────────────────────────────────────────┐
│                   Workspace                       │
│  ┌─────────────────────────────────────────────┐ │
│  │  /workspace (persistent storage)             │ │
│  │  - File operations via Workspace API         │ │
│  │  - NFS mount to local machine               │ │
│  └─────────────────────────────────────────────┘ │
│           ▲              ▲              ▲        │
│           │              │              │        │
│    ┌──────┴──┐    ┌──────┴──┐    ┌──────┴──┐    │
│    │ Sandbox │    │ Sandbox │    │ Sandbox │    │
│    │   #1    │    │   #2    │    │   #3    │    │
│    └─────────┘    └─────────┘    └─────────┘    │
└─────────────────────────────────────────────────┘
```

## Quick Start

### Starting the Service

```bash
# Development
cd server && cargo run

# Production (Docker)
docker-compose -f docker/docker-compose.prod.yml up -d
```

### Using Docker Images

```bash
# Pull the server image
docker pull ghcr.io/openelevo/elevosandbox-server:latest

# Pull the base image
docker pull ghcr.io/openelevo/elevosandbox-base:latest

# Start the server
docker run -d \
  --name elevosandbox-server \
  -p 8080:8080 \
  -p 9090:9090 \
  -v /var/run/docker.sock:/var/run/docker.sock:ro \
  ghcr.io/openelevo/elevosandbox-server:latest
```

### Environment Variables

```bash
# Service configuration
WORKSPACE_HTTP_PORT=8080
WORKSPACE_GRPC_PORT=9090
WORKSPACE_DATABASE_URL=sqlite://data/workspace.db

# Docker configuration
WORKSPACE_DOCKER_HOST=unix:///var/run/docker.sock
WORKSPACE_DOCKER_NETWORK=workspace-network

# NFS configuration
WORKSPACE_NFS_PORT=2049           # NFS service port
WORKSPACE_NFS_HOST=your-server-ip # NFS external access address

# MCP configuration
WORKSPACE_MCP_MODE=http           # disabled / stdio / http
WORKSPACE_MCP_PATH=/mcp           # HTTP mode endpoint path prefix
WORKSPACE_MCP_PROFILE=developer   # executor / developer / full
```

### NFS File Sharing

Each Workspace's `/workspace` directory can be mounted locally via NFS for bidirectional file synchronization.

**Server Configuration**

```bash
export WORKSPACE_NFS_HOST=your-server-ip
export WORKSPACE_NFS_PORT=2049
```

**Client Mount**

```bash
SERVER_IP=your-server-ip

# Create workspace
WORKSPACE_ID=$(curl -s -X POST http://${SERVER_IP}:8080/api/v1/workspaces \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}' | jq -r '.id')

# Mount NFS (requires nfs-common package)
sudo mkdir -p /mnt/workspace
sudo mount -t nfs -o nfsvers=3,tcp,nolock,port=2049,mountport=2049 \
  ${SERVER_IP}:/${WORKSPACE_ID} /mnt/workspace

# Now you can read/write /mnt/workspace, shared with all sandboxes bound to this workspace
echo "Hello" > /mnt/workspace/test.txt

# Unmount
sudo umount /mnt/workspace
```

**Notes**
- Use `port=2049,mountport=2049` parameters as portmapper is not implemented
- NFSv3 recommended (`nfsvers=3`)
- Use `nolock` option to avoid lock service dependency

## MCP (Model Context Protocol)

### MCP Profiles

| Profile | Tools | Use Case |
|---------|-------|----------|
| `executor` | 1 | Script execution only, sandbox managed by program |
| `developer` | 6 | Regular development, includes file and process operations |
| `full` | 14 | Full functionality, includes all sandbox management |

**executor** (1 tool):
- `process_run` - Execute command

**developer** (6 tools):
- `process_run` - Execute command
- `file_read` - Read file
- `file_write` - Write file
- `file_list` - List directory
- `file_mkdir` - Create directory
- `file_remove` - Delete file/directory

**full** (14 tools):
- All sandbox_* operations
- All process_* operations
- All file_* operations

### HTTP Mode (Recommended)

```bash
export WORKSPACE_MCP_MODE=http
export WORKSPACE_MCP_PATH=/mcp
cargo run
```

Available endpoints:
| Endpoint | Tools | Use Case |
|----------|-------|----------|
| `http://localhost:8080/mcp/executor` | 1 | Script execution only |
| `http://localhost:8080/mcp/developer` | 6 | Regular development |
| `http://localhost:8080/mcp/full` | 14 | Full functionality |

**MCP Client Example (Python)**

```python
from mcp import ClientSession
from mcp.client.streamable_http import streamablehttp_client

async def main():
    async with streamablehttp_client("http://localhost:8080/mcp/developer") as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()

            tools = await session.list_tools()
            print(f"Available tools: {[t.name for t in tools.tools]}")

            result = await session.call_tool(
                "process_run",
                arguments={
                    "sandbox_id": "your-sandbox-id",
                    "command": "echo",
                    "args": ["Hello, World!"]
                }
            )
            print(result)
```

### Stdio Mode

For local CLI integration like Claude Desktop.

**Claude Desktop Configuration**

Edit `~/.config/claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "elevo-workspace": {
      "command": "/path/to/workspace-server",
      "env": {
        "WORKSPACE_MCP_MODE": "stdio",
        "WORKSPACE_MCP_PROFILE": "developer",
        "WORKSPACE_DOCKER_HOST": "unix:///var/run/docker.sock"
      }
    }
  }
}
```

## SDK Usage

### Go SDK

```bash
go get github.com/OpenElevo/ElevoSandbox/sdk-go
```

```go
package main

import (
    "context"
    "fmt"
    "log"

    workspace "github.com/OpenElevo/ElevoSandbox/sdk-go"
)

func main() {
    client := workspace.NewClient("http://localhost:8080")
    ctx := context.Background()

    // Create workspace (persistent storage)
    ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{
        Name: "my-workspace",
    })
    if err != nil {
        log.Fatal(err)
    }
    defer client.Workspace.Delete(ctx, ws.ID)

    // Create sandbox bound to workspace
    sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
        WorkspaceID: ws.ID,
        Template:    "workspace-test:latest",
    })
    if err != nil {
        log.Fatal(err)
    }
    defer client.Sandbox.Delete(ctx, sandbox.ID, true)

    // Execute command
    result, err := client.Process.Run(ctx, sandbox.ID, "echo", &workspace.RunCommandOptions{
        Args: []string{"Hello", "World"},
    })
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Output: %s", result.Stdout)

    // File operations via workspace API
    client.Workspace.WriteFileString(ctx, ws.ID, "hello.txt", "Hello!")
    content, _ := client.Workspace.ReadFileString(ctx, ws.ID, "hello.txt")
    fmt.Printf("File: %s\n", content)
}
```

### Python SDK

```bash
pip install -e sdk-python
```

```python
from workspace_sdk import WorkspaceClient, CreateWorkspaceParams, CreateSandboxParams

with WorkspaceClient("http://localhost:8080") as client:
    # Create workspace (persistent storage)
    workspace = client.workspace.create(CreateWorkspaceParams(name="my-workspace"))

    # Create sandbox bound to workspace
    sandbox = client.sandbox.create(CreateSandboxParams(
        workspace_id=workspace.id,
        template="workspace-test:latest"
    ))

    try:
        # Execute command
        result = client.process.run(sandbox.id, "echo", args=["Hello", "World"])
        print(result.stdout)

        # File operations via workspace API
        client.workspace.write_file(workspace.id, "hello.txt", "Hello!")
        content = client.workspace.read_file(workspace.id, "hello.txt")
        print(f"File: {content}")
    finally:
        client.sandbox.delete(sandbox.id, force=True)
        client.workspace.delete(workspace.id)
```

### TypeScript SDK

```typescript
import { WorkspaceClient } from '@openelevo/workspace-sdk'

const client = new WorkspaceClient({ apiUrl: 'http://localhost:8080' })

// Create workspace (persistent storage)
const workspace = await client.workspace.create({ name: 'my-workspace' })

// Create sandbox bound to workspace
const sandbox = await client.sandbox.create({
  workspaceId: workspace.id,
  template: 'workspace-test:latest'
})

try {
  // Execute command
  const result = await client.process.run(sandbox.id, 'echo', {
    args: ['Hello', 'World']
  })
  console.log(result.stdout)

  // File operations via workspace API
  await client.workspace.writeFile(workspace.id, 'hello.txt', 'Hello!')
  const content = await client.workspace.readFile(workspace.id, 'hello.txt')
  console.log(`File: ${content}`)
} finally {
  await client.sandbox.delete(sandbox.id, true)
  await client.workspace.delete(workspace.id)
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Client SDK                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │     Go      │  │   Python    │  │  TypeScript │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└────────────────────────────┬────────────────────────────────┘
                             │
                    HTTP API / WebSocket / MCP
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                    Workspace Server (Rust)                   │
│  ┌──────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐          │
│  │Workspace │ │ Sandbox │ │ Process │ │   PTY   │          │
│  │(persist) │ │ (temp)  │ │         │ │         │          │
│  └──────────┘ └─────────┘ └─────────┘ └─────────┘          │
│  ┌─────────────────────────────────────────────┐           │
│  │              MCP Handler                     │           │
│  │  (executor / developer / full profiles)     │           │
│  └─────────────────────────────────────────────┘           │
│  ┌─────────────────────────────────────────────┐           │
│  │              NFS Server                      │           │
│  │  (Workspace-level file sharing)             │           │
│  └─────────────────────────────────────────────┘           │
└────────────────────────────┬────────────────────────────────┘
                             │
                        gRPC (internal)
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                    Docker Container                          │
│  ┌─────────────────────────────────────────────┐           │
│  │           Workspace Agent (Rust)             │           │
│  │  - Process execution                         │           │
│  │  - PTY management                            │           │
│  │  - /workspace (mounted Workspace directory)  │           │
│  └─────────────────────────────────────────────┘           │
└─────────────────────────────────────────────────────────────┘
```

## Development

### Building

```bash
# Build all components
./scripts/build.sh

# Build server only
cd server && cargo build --release

# Build agent only
cd agent && cargo build --release
```

### Testing

```bash
# Run tests
cargo test --workspace --lib --bins

# Integration tests (requires running server)
./scripts/run-integration-tests.sh
```

### Deployment

```bash
# Build and push images
./scripts/build-and-push.sh

# Deploy
./scripts/deploy.sh
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

MIT
