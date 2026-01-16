# Workspace SDK for Go

Go SDK for the Elevo Workspace service. Provides a simple, idiomatic Go API for managing workspaces, sandboxes, executing commands, and interacting with containerized development environments.

## Installation

```bash
go get github.com/OpenElevo/ElevoSandbox/sdk-go
```

## Quick Start

```go
package main

import (
    "context"
    "fmt"
    "log"

    workspace "github.com/OpenElevo/ElevoSandbox/sdk-go"
)

func main() {
    // Create client
    client := workspace.NewClient("http://localhost:8080")
    ctx := context.Background()

    // Create a workspace (persistent storage)
    ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{
        Name: "my-workspace",
    })
    if err != nil {
        log.Fatal(err)
    }
    defer client.Workspace.Delete(ctx, ws.ID)

    fmt.Printf("Created workspace: %s\n", ws.ID)

    // Create a sandbox bound to the workspace
    sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
        WorkspaceID: ws.ID,
        Template:    "workspace-test:latest",
    })
    if err != nil {
        log.Fatal(err)
    }
    defer client.Sandbox.Delete(ctx, sandbox.ID, true)

    fmt.Printf("Created sandbox: %s\n", sandbox.ID)

    // Run a command
    result, err := client.Process.Run(ctx, sandbox.ID, "echo", &workspace.RunCommandOptions{
        Args: []string{"Hello", "World"},
    })
    if err != nil {
        log.Fatal(err)
    }

    fmt.Printf("Output: %s", result.Stdout)

    // Write a file to workspace
    err = client.Workspace.WriteFileString(ctx, ws.ID, "hello.txt", "Hello, World!")
    if err != nil {
        log.Fatal(err)
    }

    // Read the file
    content, err := client.Workspace.ReadFileString(ctx, ws.ID, "hello.txt")
    if err != nil {
        log.Fatal(err)
    }

    fmt.Printf("File content: %s\n", content)
}
```

## Features

- **Workspace Management**: Create, list, get, and delete workspaces (persistent storage)
- **Sandbox Management**: Create, list, get, and delete sandboxes (bound to workspaces)
- **Process Execution**: Run commands with full control over args, env, and working directory
- **Streaming Output**: Real-time stdout/stderr streaming via SSE
- **PTY Support**: Interactive terminal sessions via WebSocket
- **File System**: Read, write, and manage files in workspaces
- **NFS Mount**: Mount workspaces via NFS for local development
- **Concurrent Operations**: Thread-safe client for parallel operations
- **Error Handling**: Rich error types with detailed information

## API Reference

### Client

```go
// Create a new client
client := workspace.NewClient("http://localhost:8080")

// With options
client := workspace.NewClient("http://localhost:8080", workspace.ClientOptions{
    APIKey:  "your-api-key",
    Timeout: 60 * time.Second,
    NfsHost: "192.168.1.100",  // Optional: NFS server host for mounting
    NfsPort: 2049,             // Optional: NFS server port (default: 2049)
})

// Health check
err := client.Health(ctx)
```

### Workspace Service

```go
// Create a workspace
ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{
    Name:     "my-workspace",
    Metadata: map[string]string{"project": "demo"},
})

// Get workspace by ID
ws, err := client.Workspace.Get(ctx, "workspace-id")

// List all workspaces
workspaces, err := client.Workspace.List(ctx)

// Check if workspace exists
exists, err := client.Workspace.Exists(ctx, "workspace-id")

// Delete workspace (fails if sandboxes are using it)
err := client.Workspace.Delete(ctx, "workspace-id")

// File operations on workspace
err := client.Workspace.WriteFileString(ctx, ws.ID, "src/main.py", `print("hello")`)
content, err := client.Workspace.ReadFileString(ctx, ws.ID, "src/main.py")
files, err := client.Workspace.ListFiles(ctx, ws.ID, "src")
err := client.Workspace.Mkdir(ctx, ws.ID, "src/components")
err := client.Workspace.CopyFile(ctx, ws.ID, "src/main.py", "src/backup.py")
err := client.Workspace.MoveFile(ctx, ws.ID, "src/backup.py", "src/old.py")
err := client.Workspace.DeleteFile(ctx, ws.ID, "src/old.py", false)
info, err := client.Workspace.GetFileInfo(ctx, ws.ID, "src/main.py")
exists, err := client.Workspace.FileExists(ctx, ws.ID, "src/main.py")
```

### Sandbox Service

```go
// Create a sandbox bound to a workspace
sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
    WorkspaceID: ws.ID,              // Required: workspace to bind to
    Template:    "workspace-test:latest",
    Name:        "my-sandbox",
    Env:         map[string]string{"APP_ENV": "development"},
    Metadata:    map[string]string{"project": "demo"},
    Timeout:     3600, // seconds
})

// Get sandbox by ID
sandbox, err := client.Sandbox.Get(ctx, "sandbox-id")

// List all sandboxes
sandboxes, err := client.Sandbox.List(ctx)

// List with state filter
sandboxes, err := client.Sandbox.ListWithFilter(ctx, workspace.SandboxStateRunning)

// Check if sandbox exists
exists, err := client.Sandbox.Exists(ctx, "sandbox-id")

// Delete sandbox
err := client.Sandbox.Delete(ctx, "sandbox-id", true) // force=true

// Wait for sandbox state
sandbox, err := client.Sandbox.WaitForState(ctx, "sandbox-id", workspace.SandboxStateRunning)
```

### Process Service

```go
// Run a command and wait for completion
result, err := client.Process.Run(ctx, sandboxID, "ls", &workspace.RunCommandOptions{
    Args:    []string{"-la", "/workspace"},
    Env:     map[string]string{"LC_ALL": "C"},
    Cwd:     "/workspace",
    Timeout: 30,
})
fmt.Printf("Exit code: %d\n", result.ExitCode)
fmt.Printf("Stdout: %s\n", result.Stdout)
fmt.Printf("Stderr: %s\n", result.Stderr)

// Simple exec helper (returns stdout, errors on non-zero exit)
output, err := client.Process.Exec(ctx, sandboxID, "cat", "/etc/hostname")

// Run shell script
result, err := client.Process.Shell(ctx, sandboxID, `
    for i in 1 2 3; do
        echo "Item $i"
    done
`, map[string]string{"DEBUG": "true"})

// Stream command output
eventCh, errCh := client.Process.RunStream(ctx, sandboxID, "tail", &workspace.RunCommandOptions{
    Args: []string{"-f", "/var/log/app.log"},
})

for {
    select {
    case event, ok := <-eventCh:
        if !ok {
            return
        }
        switch event.Type {
        case workspace.ProcessEventStdout:
            fmt.Print(event.Data)
        case workspace.ProcessEventStderr:
            fmt.Fprint(os.Stderr, event.Data)
        case workspace.ProcessEventExit:
            fmt.Printf("\nExited with code: %d\n", *event.Code)
        case workspace.ProcessEventError:
            fmt.Printf("Error: %s\n", event.Message)
        }
    case err := <-errCh:
        if err != nil {
            log.Fatal(err)
        }
    }
}

// Kill a process
err := client.Process.Kill(ctx, sandboxID, pid, 15) // SIGTERM
```

### PTY Service

```go
// Create and connect to PTY
pty, err := client.Pty.Connect(ctx, sandboxID, &workspace.PtyOptions{
    Cols:    120,
    Rows:    40,
    Command: "/bin/bash",
    Env:     map[string]string{"TERM": "xterm-256color"},
})

// Handle output
pty.OnData(func(data []byte) {
    os.Stdout.Write(data)
})

// Handle close
pty.OnClose(func() {
    fmt.Println("PTY closed")
})

// Write to PTY
err := pty.Write([]byte("ls -la\n"))

// Resize PTY
err := pty.Resize(100, 50)

// Kill PTY
err := pty.Kill()
```

### NFS Service

```go
// Mount a workspace via NFS
mountPoint, err := client.Nfs.Mount(ws.ID, "/mnt/workspace")

// Check if mounted
isMounted, err := client.Nfs.IsMounted("/mnt/workspace")

// Unmount
err := client.Nfs.Unmount("/mnt/workspace")

// Get NFS URL for a workspace
nfsURL := client.Nfs.GetNfsURL(ws.ID)
// Returns: nfs://192.168.1.100:2049/workspace-id
```

## Error Handling

```go
sandbox, err := client.Sandbox.Get(ctx, "invalid-id")
if err != nil {
    if workspace.IsSandboxNotFound(err) {
        fmt.Println("Sandbox not found")
    } else if workspace.IsNotFound(err) {
        fmt.Println("Resource not found")
    } else if wsErr, ok := err.(*workspace.Error); ok {
        fmt.Printf("API error [%d]: %s\n", wsErr.StatusCode, wsErr.Message)
    } else {
        fmt.Printf("Unknown error: %v\n", err)
    }
}

// Workspace deletion protection
err := client.Workspace.Delete(ctx, "workspace-with-sandboxes")
if err != nil {
    // Error: Workspace has active sandboxes (code: 7002)
    fmt.Println(err)
}
```

## Type Definitions

### Workspace

```go
type Workspace struct {
    ID        string            `json:"id"`
    Name      *string           `json:"name,omitempty"`
    NfsURL    *string           `json:"nfs_url,omitempty"`  // NFS mount URL
    Metadata  map[string]string `json:"metadata,omitempty"`
    CreatedAt time.Time         `json:"created_at"`
    UpdatedAt time.Time         `json:"updated_at"`
}
```

### Sandbox

```go
type Sandbox struct {
    ID           string            `json:"id"`
    WorkspaceID  string            `json:"workspace_id"`  // Bound workspace ID
    Name         *string           `json:"name,omitempty"`
    Template     string            `json:"template"`
    State        SandboxState      `json:"state"`
    Env          map[string]string `json:"env,omitempty"`
    Metadata     map[string]string `json:"metadata,omitempty"`
    CreatedAt    time.Time         `json:"created_at"`
    UpdatedAt    time.Time         `json:"updated_at"`
    Timeout      int               `json:"timeout,omitempty"`
    ErrorMessage *string           `json:"error_message,omitempty"`
}

type SandboxState string

const (
    SandboxStateStarting SandboxState = "starting"
    SandboxStateRunning  SandboxState = "running"
    SandboxStateStopped  SandboxState = "stopped"
    SandboxStateFailed   SandboxState = "failed"
)
```

### CommandResult

```go
type CommandResult struct {
    ExitCode int    `json:"exit_code"`
    Stdout   string `json:"stdout"`
    Stderr   string `json:"stderr"`
}
```

### FileInfo

```go
type FileInfo struct {
    Name    string    `json:"name"`
    Path    string    `json:"path"`
    Size    int64     `json:"size"`
    Mode    string    `json:"mode"`
    ModTime time.Time `json:"mod_time"`
    IsDir   bool      `json:"is_dir"`
}
```

## Concurrent Usage

```go
package main

import (
    "context"
    "fmt"
    "log"
    "sync"

    workspace "github.com/OpenElevo/ElevoSandbox/sdk-go"
)

func main() {
    client := workspace.NewClient("http://localhost:8080")
    ctx := context.Background()

    // Create a shared workspace
    ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{
        Name: "shared-workspace",
    })
    if err != nil {
        log.Fatal(err)
    }
    defer client.Workspace.Delete(ctx, ws.ID)

    // Create multiple sandboxes sharing the same workspace
    var sandboxes []*workspace.Sandbox
    var mu sync.Mutex
    var wg sync.WaitGroup

    for i := 0; i < 3; i++ {
        wg.Add(1)
        go func(i int) {
            defer wg.Done()
            sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
                WorkspaceID: ws.ID,
                Template:    "workspace-test:latest",
                Name:        fmt.Sprintf("worker-%d", i),
            })
            if err != nil {
                log.Printf("Failed to create sandbox %d: %v", i, err)
                return
            }
            mu.Lock()
            sandboxes = append(sandboxes, sandbox)
            mu.Unlock()
        }(i)
    }
    wg.Wait()

    fmt.Printf("Created %d sandboxes sharing workspace %s\n", len(sandboxes), ws.ID)

    // Run commands in all sandboxes concurrently
    for i, sandbox := range sandboxes {
        wg.Add(1)
        go func(i int, sandbox *workspace.Sandbox) {
            defer wg.Done()
            result, err := client.Process.Run(ctx, sandbox.ID, "echo", &workspace.RunCommandOptions{
                Args: []string{fmt.Sprintf("Worker %d", i)},
            })
            if err != nil {
                log.Printf("Worker %d error: %v", i, err)
                return
            }
            fmt.Printf("Worker %d: %s", i, result.Stdout)
        }(i, sandbox)
    }
    wg.Wait()

    // Cleanup all sandboxes first
    for _, sandbox := range sandboxes {
        wg.Add(1)
        go func(sandbox *workspace.Sandbox) {
            defer wg.Done()
            client.Sandbox.Delete(ctx, sandbox.ID, true)
        }(sandbox)
    }
    wg.Wait()
}
```

## Context and Timeout

```go
// Using context with timeout
ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
defer cancel()

result, err := client.Process.Run(ctx, sandboxID, "sleep", &workspace.RunCommandOptions{
    Args: []string{"60"},
})
if err != nil {
    if ctx.Err() == context.DeadlineExceeded {
        fmt.Println("Request timed out")
    }
}

// Command-level timeout
result, err := client.Process.Run(ctx, sandboxID, "sleep", &workspace.RunCommandOptions{
    Args:    []string{"60"},
    Timeout: 5, // seconds
})
```

## Examples

See the `examples/` directory for more usage examples:

- `examples/basic/main.go` - Basic usage with workspace, sandbox and process operations
- `examples/streaming/main.go` - Streaming command output
- `examples/concurrent/main.go` - Concurrent operations with goroutines
- `examples/error-handling/main.go` - Error handling patterns
- `examples/pty-session/main.go` - Interactive PTY sessions
- `examples/nfs-mount/main.go` - NFS mounting for local development

## Building from Source

```bash
# Get dependencies
go mod download

# Run tests
go test ./...

# Build examples
go build -o bin/ ./examples/...
```

## License

MIT License
