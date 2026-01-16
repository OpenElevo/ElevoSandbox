# Workspace SDK for Python

Python SDK for the Elevo Workspace service. Provides both synchronous and asynchronous APIs for managing workspaces, sandboxes, executing commands, and interacting with containerized development environments.

## Installation

```bash
pip install workspace-sdk
```

Or install from source:

```bash
pip install -e ./sdk-python
```

### Dependencies

- Python 3.10+
- httpx
- websockets

## Quick Start

### Synchronous Client

```python
from workspace_sdk import WorkspaceClient, CreateWorkspaceParams, CreateSandboxParams, RunCommandOptions

# Using context manager (recommended)
with WorkspaceClient("http://localhost:8080") as client:
    # Create a workspace (persistent storage)
    workspace = client.workspace.create(CreateWorkspaceParams(
        name="my-workspace"
    ))
    print(f"Created workspace: {workspace.id}")

    # Create a sandbox bound to the workspace
    sandbox = client.sandbox.create(CreateSandboxParams(
        workspace_id=workspace.id,
        template="workspace-test:latest"
    ))
    print(f"Created sandbox: {sandbox.id}")

    # Run a command
    result = client.process.run(sandbox.id, "echo",
        RunCommandOptions(args=["Hello", "World"]))
    print(f"Output: {result.stdout}")

    # Write a file to workspace
    client.workspace.write_file(workspace.id, "hello.txt", "Hello, World!")

    # Read the file
    content = client.workspace.read_file(workspace.id, "hello.txt")
    print(f"File content: {content}")

    # Cleanup
    client.sandbox.delete(sandbox.id, force=True)
    client.workspace.delete(workspace.id)
```

### Asynchronous Client

```python
import asyncio
from workspace_sdk import AsyncWorkspaceClient, CreateWorkspaceParams, CreateSandboxParams, RunCommandOptions

async def main():
    async with AsyncWorkspaceClient("http://localhost:8080") as client:
        # Create a workspace (persistent storage)
        workspace = await client.workspace.create(CreateWorkspaceParams(
            name="my-workspace"
        ))
        print(f"Created workspace: {workspace.id}")

        # Create a sandbox bound to the workspace
        sandbox = await client.sandbox.create(CreateSandboxParams(
            workspace_id=workspace.id,
            template="workspace-test:latest"
        ))
        print(f"Created sandbox: {sandbox.id}")

        # Run a command
        result = await client.process.run(sandbox.id, "echo",
            RunCommandOptions(args=["Hello", "World"]))
        print(f"Output: {result.stdout}")

        # Write a file to workspace
        await client.workspace.write_file(workspace.id, "hello.txt", "Hello, World!")

        # Read the file
        content = await client.workspace.read_file(workspace.id, "hello.txt")
        print(f"File content: {content}")

        # Cleanup
        await client.sandbox.delete(sandbox.id, force=True)
        await client.workspace.delete(workspace.id)

asyncio.run(main())
```

## Features

- **Dual API Support**: Both synchronous and asynchronous clients
- **Workspace Management**: Create, list, get, and delete workspaces (persistent storage)
- **Sandbox Management**: Create, list, get, and delete sandboxes (bound to workspaces)
- **Process Execution**: Run commands with full control over args, env, and working directory
- **Streaming Output**: Real-time stdout/stderr streaming via SSE
- **PTY Support**: Interactive terminal sessions via WebSocket
- **File System**: Read, write, and manage files in workspaces
- **NFS Mount**: Mount workspaces via NFS for local development
- **Type Safety**: Full type hints with dataclasses
- **Error Handling**: Rich error types with detailed information

## API Reference

### Client Initialization

```python
# Sync client
from workspace_sdk import WorkspaceClient

with WorkspaceClient(
    api_url="http://localhost:8080",
    api_key="your-api-key",  # Optional
    timeout=60.0,            # Request timeout in seconds
    nfs_host="192.168.1.100",  # Optional: NFS server host for mounting
    nfs_port=2049,           # Optional: NFS server port (default: 2049)
) as client:
    # Use client...
    pass

# Async client
from workspace_sdk import AsyncWorkspaceClient

async with AsyncWorkspaceClient(
    api_url="http://localhost:8080",
    api_key="your-api-key",
    timeout=60.0,
) as client:
    # Use client...
    pass
```

### Workspace Service

```python
from workspace_sdk import CreateWorkspaceParams

# Create a workspace
workspace = client.workspace.create(CreateWorkspaceParams(
    name="my-workspace",
    metadata={"project": "demo"},
))

# Get workspace by ID
workspace = client.workspace.get("workspace-id")

# List all workspaces
workspaces = client.workspace.list()

# Delete workspace (fails if sandboxes are using it)
client.workspace.delete("workspace-id")

# File operations on workspace
client.workspace.write_file(workspace.id, "src/main.py", 'print("hello")')
content = client.workspace.read_file(workspace.id, "src/main.py")
files = client.workspace.list_files(workspace.id, "src")
client.workspace.mkdir(workspace.id, "src/components")
client.workspace.copy_file(workspace.id, "src/main.py", "src/backup.py")
client.workspace.move_file(workspace.id, "src/backup.py", "src/old.py")
client.workspace.delete_file(workspace.id, "src/old.py")
info = client.workspace.get_file_info(workspace.id, "src/main.py")
exists = client.workspace.exists(workspace.id, "src/main.py")
```

### Sandbox Service

```python
from workspace_sdk import CreateSandboxParams

# Create a sandbox bound to a workspace
sandbox = client.sandbox.create(CreateSandboxParams(
    workspace_id=workspace.id,  # Required: workspace to bind to
    template="workspace-test:latest",
    name="my-sandbox",
    env={"APP_ENV": "development"},
    metadata={"project": "demo"},
    timeout=3600,  # seconds
))

# Get sandbox by ID
sandbox = client.sandbox.get("sandbox-id")

# List all sandboxes
sandboxes = client.sandbox.list()

# List with state filter
sandboxes = client.sandbox.list(state="running")

# Check if sandbox exists
exists = client.sandbox.exists("sandbox-id")

# Delete sandbox
client.sandbox.delete("sandbox-id", force=True)

# Wait for sandbox state
sandbox = client.sandbox.wait_for_state("sandbox-id", "running")
```

### Process Service

```python
from workspace_sdk import RunCommandOptions

# Run a command and wait for completion
result = client.process.run(
    sandbox_id,
    "ls",
    RunCommandOptions(
        args=["-la", "/workspace"],
        env={"LC_ALL": "C"},
        cwd="/workspace",
        timeout=30,
    )
)
print(f"Exit code: {result.exit_code}")
print(f"Stdout: {result.stdout}")
print(f"Stderr: {result.stderr}")

# Simple exec helper
output = client.process.exec(sandbox_id, "cat", "/etc/hostname")

# Run shell script
result = client.process.shell(
    sandbox_id,
    """
    for i in 1 2 3; do
        echo "Item $i"
    done
    """,
    env={"DEBUG": "true"}
)

# Stream command output (async)
async for event in client.process.run_stream(
    sandbox_id,
    "tail",
    RunCommandOptions(args=["-f", "/var/log/app.log"])
):
    if event.type == "stdout":
        print(event.data, end="")
    elif event.type == "stderr":
        print(event.data, end="", file=sys.stderr)
    elif event.type == "exit":
        print(f"\nExited with code: {event.code}")
    elif event.type == "error":
        print(f"Error: {event.message}")

# Kill a process
client.process.kill(sandbox_id, pid, signal=15)  # SIGTERM
```

### PTY Service (async only)

```python
from workspace_sdk import PtyOptions

# Create and connect to PTY
pty = await client.pty.connect(
    sandbox_id,
    PtyOptions(
        cols=120,
        rows=40,
        shell="/bin/bash",
        env={"TERM": "xterm-256color"},
    )
)

# Handle output
pty.on_data(lambda data: print(data.decode(), end=""))

# Handle close
pty.on_close(lambda: print("PTY closed"))

# Write to PTY
await pty.write("ls -la\n")

# Resize PTY
await pty.resize(100, 50)

# Kill PTY
await pty.kill()
```

### NFS Service

```python
# Mount a workspace via NFS
mount_point = client.nfs.mount(workspace.id, "/mnt/workspace")

# Check if mounted
is_mounted = client.nfs.is_mounted("/mnt/workspace")

# Unmount
client.nfs.unmount("/mnt/workspace")
```

## Error Handling

```python
from workspace_sdk.errors import (
    WorkspaceError,
    SandboxNotFoundError,
    WorkspaceNotFoundError,
    TemplateNotFoundError,
    FileNotFoundError,
    PermissionDeniedError,
    ProcessTimeoutError,
    PtyNotFoundError,
    AgentNotConnectedError,
)

try:
    sandbox = client.sandbox.get("invalid-id")
except SandboxNotFoundError:
    print("Sandbox not found")
except WorkspaceNotFoundError:
    print("Workspace not found")
except WorkspaceError as e:
    print(f"API error [{e.code}]: {e.message}")

# Workspace deletion protection
try:
    client.workspace.delete("workspace-with-sandboxes")
except WorkspaceError as e:
    # Error: Workspace has active sandboxes (code: 7002)
    print(e.message)
```

## Type Definitions

### Workspace

```python
@dataclass
class Workspace:
    id: str
    created_at: str
    updated_at: str
    name: Optional[str] = None
    nfs_url: Optional[str] = None  # NFS mount URL
    metadata: Optional[Dict[str, str]] = None
```

### Sandbox

```python
@dataclass
class Sandbox:
    id: str
    workspace_id: str  # Bound workspace ID
    template: str
    state: SandboxState  # "starting" | "running" | "stopping" | "stopped" | "error"
    created_at: str
    updated_at: str
    name: Optional[str] = None
    env: Optional[Dict[str, str]] = None
    metadata: Optional[Dict[str, str]] = None
    timeout: Optional[int] = None
    error_message: Optional[str] = None
```

### CommandResult

```python
@dataclass
class CommandResult:
    exit_code: int
    stdout: str
    stderr: str
```

### FileInfo

```python
@dataclass
class FileInfo:
    name: str
    path: str
    type: FileType  # "file" | "directory" | "symlink"
    size: int
    modified_at: Optional[str] = None
```

## Concurrent Usage

```python
import asyncio
from workspace_sdk import AsyncWorkspaceClient, CreateWorkspaceParams, CreateSandboxParams

async def main():
    async with AsyncWorkspaceClient("http://localhost:8080") as client:
        # Create a shared workspace
        workspace = await client.workspace.create(CreateWorkspaceParams(
            name="shared-workspace"
        ))

        # Create multiple sandboxes sharing the same workspace
        sandboxes = await asyncio.gather(
            client.sandbox.create(CreateSandboxParams(
                workspace_id=workspace.id,
                template="workspace-test:latest"
            )),
            client.sandbox.create(CreateSandboxParams(
                workspace_id=workspace.id,
                template="workspace-test:latest"
            )),
            client.sandbox.create(CreateSandboxParams(
                workspace_id=workspace.id,
                template="workspace-test:latest"
            )),
        )

        print(f"Created {len(sandboxes)} sandboxes sharing workspace {workspace.id}")

        # Run commands in all sandboxes concurrently
        results = await asyncio.gather(
            *[client.process.run(s.id, "echo", RunCommandOptions(args=[f"Worker {i}"]))
              for i, s in enumerate(sandboxes)]
        )

        for i, result in enumerate(results):
            print(f"Worker {i}: {result.stdout.strip()}")

        # Cleanup all sandboxes first
        await asyncio.gather(
            *[client.sandbox.delete(s.id, force=True) for s in sandboxes]
        )

        # Then delete the workspace
        await client.workspace.delete(workspace.id)

asyncio.run(main())
```

## Examples

See the `examples/` directory for more usage examples:

- `examples/basic.py` - Basic usage with workspace, sandbox and process operations
- `examples/async_basic.py` - Async version of basic usage
- `examples/streaming.py` - Streaming command output
- `examples/concurrent.py` - Concurrent operations with asyncio
- `examples/error_handling.py` - Error handling patterns
- `examples/pty_session.py` - Interactive PTY sessions
- `examples/nfs_mount.py` - NFS mounting for local development

## Building from Source

```bash
# Install dependencies
pip install -e ".[dev]"

# Run tests
pytest

# Type check
mypy src

# Format code
black src tests
```

## License

MIT License
