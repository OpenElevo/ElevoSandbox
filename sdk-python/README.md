# Workspace SDK for Python

Python SDK for the Elevo Workspace service. Provides both synchronous and asynchronous APIs for managing sandboxes, executing commands, and interacting with containerized development environments.

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
from workspace_sdk import WorkspaceClient, CreateSandboxParams

# Using context manager (recommended)
with WorkspaceClient("http://localhost:8080") as client:
    # Create a sandbox
    sandbox = client.sandbox.create(CreateSandboxParams(
        template="workspace-test:latest"
    ))
    print(f"Created sandbox: {sandbox.id}")

    # Run a command
    result = client.process.run(sandbox.id, "echo",
        RunCommandOptions(args=["Hello", "World"]))
    print(f"Output: {result.stdout}")

    # Cleanup
    client.sandbox.delete(sandbox.id, force=True)
```

### Asynchronous Client

```python
import asyncio
from workspace_sdk import AsyncWorkspaceClient, CreateSandboxParams

async def main():
    async with AsyncWorkspaceClient("http://localhost:8080") as client:
        # Create a sandbox
        sandbox = await client.sandbox.create(CreateSandboxParams(
            template="workspace-test:latest"
        ))
        print(f"Created sandbox: {sandbox.id}")

        # Run a command
        result = await client.process.run(sandbox.id, "echo",
            RunCommandOptions(args=["Hello", "World"]))
        print(f"Output: {result.stdout}")

        # Cleanup
        await client.sandbox.delete(sandbox.id, force=True)

asyncio.run(main())
```

## Features

- **Dual API Support**: Both synchronous and asynchronous clients
- **Sandbox Management**: Create, list, get, and delete sandboxes
- **Process Execution**: Run commands with full control over args, env, and working directory
- **Streaming Output**: Real-time stdout/stderr streaming via SSE
- **PTY Support**: Interactive terminal sessions via WebSocket
- **File System**: Read, write, and manage files in sandboxes
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

### Sandbox Service

```python
from workspace_sdk import CreateSandboxParams

# Create a sandbox
sandbox = client.sandbox.create(CreateSandboxParams(
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

# Stream command output (sync)
for event in client.process.run_stream(sandbox_id, "tail",
    RunCommandOptions(args=["-f", "/var/log/app.log"])):
    if event.type == "stdout":
        print(event.data, end="")
    elif event.type == "stderr":
        print(event.data, end="", file=sys.stderr)
    elif event.type == "exit":
        print(f"\nExited with code: {event.code}")
        break

# Stream command output (async)
async for event in client.process.run_stream(sandbox_id, "tail",
    RunCommandOptions(args=["-f", "/var/log/app.log"])):
    if event.type == "stdout":
        print(event.data, end="")
    elif event.type == "exit":
        break

# Kill a process
client.process.kill(sandbox_id, pid, signal=15)  # SIGTERM
```

### PTY Service

```python
from workspace_sdk import PtyOptions

# Async client required for PTY
async with AsyncWorkspaceClient("http://localhost:8080") as client:
    # Create and connect to PTY
    pty = await client.pty.connect(sandbox_id, PtyOptions(
        cols=120,
        rows=40,
        shell="/bin/bash",
    ))

    # Register data callback
    def on_data(data: bytes):
        print(data.decode(), end="")

    pty.on_data(on_data)

    # Write to PTY
    await pty.write("ls -la\n")

    # Resize PTY
    await pty.resize(100, 50)

    # Kill PTY
    await pty.kill()
```

### FileSystem Service

```python
# Read file as bytes
content = client.filesystem.read(sandbox_id, "/workspace/config.json")

# Read file as string
text = client.filesystem.read_string(sandbox_id, "/workspace/README.md")

# Write file from bytes
client.filesystem.write(sandbox_id, "/workspace/data.bin", b"\x00\x01\x02")

# Write file from string
client.filesystem.write_string(sandbox_id, "/workspace/hello.txt", "Hello, World!")

# Create directory
client.filesystem.mkdir(sandbox_id, "/workspace/src/components", recursive=True)

# List directory
files = client.filesystem.list(sandbox_id, "/workspace")
for f in files:
    print(f"{f.type:10} {f.size:8} {f.name}")

# Get file info
info = client.filesystem.stat(sandbox_id, "/workspace/file.txt")

# Check if exists
exists = client.filesystem.exists(sandbox_id, "/workspace/file.txt")

# Remove file
client.filesystem.remove(sandbox_id, "/workspace/temp.txt")

# Remove directory recursively
client.filesystem.remove(sandbox_id, "/workspace/old_dir", recursive=True)

# Move/rename
client.filesystem.move(sandbox_id, "/workspace/old.txt", "/workspace/new.txt")

# Copy
client.filesystem.copy(sandbox_id, "/workspace/src.txt", "/workspace/dst.txt")
```

## Error Handling

```python
from workspace_sdk import (
    WorkspaceError,
    SandboxNotFoundError,
    TemplateNotFoundError,
    FileNotFoundError,
    PermissionDeniedError,
    ProcessTimeoutError,
    PtyNotFoundError,
    AgentNotConnectedError,
)

try:
    sandbox = client.sandbox.get("invalid-id")
except SandboxNotFoundError as e:
    print(f"Sandbox not found: {e.sandbox_id}")
except WorkspaceError as e:
    print(f"API error [{e.code}]: {e.message}")

# Process errors
try:
    result = client.process.run(sandbox_id, "invalid-command")
except ProcessTimeoutError:
    print("Command timed out")
except WorkspaceError as e:
    print(f"Error: {e}")
```

## Type Definitions

### Sandbox

```python
@dataclass
class Sandbox:
    id: str
    template: str
    state: SandboxState  # "starting" | "running" | "stopping" | "stopped" | "error"
    created_at: str
    updated_at: str
    name: Optional[str] = None
    env: Optional[Dict[str, str]] = None
    metadata: Optional[Dict[str, str]] = None
    nfs_url: Optional[str] = None
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

### ProcessEvent

```python
# Union type for streaming events
ProcessEvent = StdoutEvent | StderrEvent | ExitEvent | ErrorEvent

@dataclass
class StdoutEvent:
    type: Literal["stdout"]
    data: str

@dataclass
class StderrEvent:
    type: Literal["stderr"]
    data: str

@dataclass
class ExitEvent:
    type: Literal["exit"]
    code: int

@dataclass
class ErrorEvent:
    type: Literal["error"]
    message: str
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

### Sync Client with Threading

```python
import threading
from workspace_sdk import WorkspaceClient, CreateSandboxParams

def worker(worker_id: int):
    with WorkspaceClient("http://localhost:8080") as client:
        sandbox = client.sandbox.create(CreateSandboxParams(
            template="workspace-test:latest",
        ))
        try:
            result = client.process.run(
                sandbox.id,
                "echo",
                RunCommandOptions(args=[f"Worker {worker_id}"])
            )
            print(f"Worker {worker_id}: {result.stdout.strip()}")
        finally:
            client.sandbox.delete(sandbox.id, force=True)

threads = []
for i in range(5):
    t = threading.Thread(target=worker, args=(i,))
    threads.append(t)
    t.start()

for t in threads:
    t.join()
```

### Async Client with asyncio

```python
import asyncio
from workspace_sdk import AsyncWorkspaceClient, CreateSandboxParams

async def worker(client, worker_id: int):
    sandbox = await client.sandbox.create(CreateSandboxParams(
        template="workspace-test:latest",
    ))
    try:
        result = await client.process.run(
            sandbox.id,
            "echo",
            RunCommandOptions(args=[f"Worker {worker_id}"])
        )
        print(f"Worker {worker_id}: {result.stdout.strip()}")
    finally:
        await client.sandbox.delete(sandbox.id, force=True)

async def main():
    async with AsyncWorkspaceClient("http://localhost:8080") as client:
        tasks = [worker(client, i) for i in range(5)]
        await asyncio.gather(*tasks)

asyncio.run(main())
```

## Timeout Handling

```python
import asyncio
from workspace_sdk import AsyncWorkspaceClient, RunCommandOptions

async def main():
    async with AsyncWorkspaceClient("http://localhost:8080", timeout=60.0) as client:
        sandbox = await client.sandbox.create()

        try:
            # Command-level timeout
            result = await client.process.run(
                sandbox.id,
                "sleep",
                RunCommandOptions(args=["10"], timeout=5)
            )
        except Exception as e:
            print(f"Command timed out: {e}")

        # Using asyncio timeout
        try:
            async with asyncio.timeout(2.0):
                result = await client.process.run(
                    sandbox.id,
                    "sleep",
                    RunCommandOptions(args=["60"])
                )
        except asyncio.TimeoutError:
            print("Operation timed out")

        await client.sandbox.delete(sandbox.id, force=True)

asyncio.run(main())
```

## Examples

See the `examples/` directory for more usage examples:

- `examples/basic.py` - Basic usage with sandbox and process operations
- `examples/async_example.py` - Async client usage
- `examples/streaming.py` - Streaming command output
- `examples/concurrent.py` - Concurrent operations
- `examples/error_handling.py` - Error handling patterns

## License

MIT License
