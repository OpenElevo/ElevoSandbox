# Workspace SDK for Go

Go SDK for the Unified Workspace service. Provides a simple, idiomatic Go API for managing sandboxes, executing commands, and interacting with containerized development environments.

## Installation

```bash
go get github.com/anthropic/workspace-sdk-go
```

## Quick Start

```go
package main

import (
    "context"
    "fmt"
    "log"

    workspace "github.com/anthropic/workspace-sdk-go"
)

func main() {
    // Create client
    client := workspace.NewClient("http://localhost:8080")
    ctx := context.Background()

    // Create a sandbox
    sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
        Template: "workspace-test:latest",
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
}
```

## Features

- **Sandbox Management**: Create, list, get, and delete sandboxes
- **Process Execution**: Run commands with full control over args, env, and working directory
- **Streaming Output**: Real-time stdout/stderr streaming via SSE
- **PTY Support**: Interactive terminal sessions via WebSocket
- **File System**: Read, write, and manage files in sandboxes
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
})

// Health check
err := client.Health(ctx)
```

### Sandbox Service

```go
// Create a sandbox
sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
    Template: "workspace-test:latest",
    Name:     "my-sandbox",
    Env:      map[string]string{"APP_ENV": "development"},
    Metadata: map[string]string{"project": "demo"},
    Timeout:  3600, // seconds
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
            fmt.Printf("Exited with code: %d\n", *event.Code)
            return
        }
    case err := <-errCh:
        if err != nil {
            log.Printf("Stream error: %v", err)
        }
        return
    }
}

// Kill a process
err := client.Process.Kill(ctx, sandboxID, pid, 15) // SIGTERM
```

### PTY Service

```go
// Create PTY handle only
handle, err := client.Pty.Create(ctx, sandboxID, &workspace.PtyOptions{
    Cols:    120,
    Rows:    40,
    Command: "/bin/bash",
})

// Create and connect to PTY via WebSocket
session, err := client.Pty.Connect(ctx, sandboxID, &workspace.PtyOptions{
    Cols: 80,
    Rows: 24,
})
defer session.Close()

// Write to PTY
session.WriteString("ls -la\n")

// Read from PTY
go func() {
    for data := range session.Read() {
        fmt.Print(string(data))
    }
}()

// Resize PTY
session.Resize(100, 50)

// Handle errors
go func() {
    for err := range session.Errors() {
        log.Printf("PTY error: %v", err)
    }
}()

// Wait for session to end
<-session.Done()
```

### FileSystem Service

```go
// Read file
content, err := client.FileSystem.Read(ctx, sandboxID, "/workspace/config.json")

// Read as string
text, err := client.FileSystem.ReadString(ctx, sandboxID, "/workspace/README.md")

// Write file
err := client.FileSystem.Write(ctx, sandboxID, "/workspace/data.bin", []byte{0x00, 0x01})

// Write string
err := client.FileSystem.WriteString(ctx, sandboxID, "/workspace/hello.txt", "Hello, World!")

// Create directory
err := client.FileSystem.Mkdir(ctx, sandboxID, "/workspace/src/components", true) // recursive

// Create directory (shorthand)
err := client.FileSystem.MkdirAll(ctx, sandboxID, "/workspace/src/components")

// List directory
files, err := client.FileSystem.List(ctx, sandboxID, "/workspace")
for _, f := range files {
    fmt.Printf("%s %d %s\n", f.Mode, f.Size, f.Name)
}

// Get file info
info, err := client.FileSystem.Stat(ctx, sandboxID, "/workspace/file.txt")

// Check if exists
exists, err := client.FileSystem.Exists(ctx, sandboxID, "/workspace/file.txt")

// Remove file
err := client.FileSystem.Remove(ctx, sandboxID, "/workspace/temp.txt", false)

// Remove directory recursively
err := client.FileSystem.RemoveAll(ctx, sandboxID, "/workspace/old_dir")

// Move/rename
err := client.FileSystem.Move(ctx, sandboxID, "/workspace/old.txt", "/workspace/new.txt")

// Copy
err := client.FileSystem.Copy(ctx, sandboxID, "/workspace/src.txt", "/workspace/dst.txt")
```

## Error Handling

```go
result, err := client.Sandbox.Get(ctx, "invalid-id")
if err != nil {
    // Check error type
    if workspace.IsNotFound(err) {
        fmt.Println("Sandbox not found")
    } else if workspace.IsTimeout(err) {
        fmt.Println("Request timed out")
    } else if e, ok := err.(*workspace.Error); ok {
        fmt.Printf("API error [%d]: %s\n", e.StatusCode, e.Message)
    } else {
        fmt.Printf("Unknown error: %v\n", err)
    }
}

// Process errors
output, err := client.Process.Exec(ctx, sandboxID, "invalid-command")
if err != nil {
    if e, ok := err.(*workspace.ProcessError); ok {
        fmt.Printf("Command '%s' failed: %s\n", e.Command, e.Message)
    }
}
```

## Concurrent Usage

The client is safe for concurrent use:

```go
client := workspace.NewClient("http://localhost:8080")

var wg sync.WaitGroup
for i := 0; i < 10; i++ {
    wg.Add(1)
    go func(id int) {
        defer wg.Done()

        sandbox, _ := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
            Template: "workspace-test:latest",
        })
        defer client.Sandbox.Delete(ctx, sandbox.ID, true)

        result, _ := client.Process.Run(ctx, sandbox.ID, "echo", &workspace.RunCommandOptions{
            Args: []string{fmt.Sprintf("Worker %d", id)},
        })
        fmt.Printf("Worker %d: %s", id, result.Stdout)
    }(i)
}
wg.Wait()
```

## Context Support

All methods accept a context for cancellation and timeouts:

```go
// With timeout
ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
defer cancel()

result, err := client.Process.Run(ctx, sandboxID, "sleep", &workspace.RunCommandOptions{
    Args: []string{"60"},
})
if err != nil {
    if ctx.Err() == context.DeadlineExceeded {
        fmt.Println("Command timed out")
    }
}

// With cancellation
ctx, cancel := context.WithCancel(context.Background())

go func() {
    time.Sleep(5 * time.Second)
    cancel() // Cancel after 5 seconds
}()

eventCh, _ := client.Process.RunStream(ctx, sandboxID, "tail", &workspace.RunCommandOptions{
    Args: []string{"-f", "/var/log/app.log"},
})
```

## License

MIT License
